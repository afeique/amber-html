//! A `robots.txt` parser and matcher (RFC 9309 + the common `Allow`/wildcard
//! extensions). See `Plans.md` (task 3.2).
//!
//! Pure and I/O-free: [`Robots::parse`] turns a `robots.txt` body into grouped
//! rules, and [`Robots::allows`] / [`Robots::crawl_delay`] answer per-user-agent
//! questions. The crawler is expected to fetch `/robots.txt`, parse it once, and
//! consult it before enqueuing each URL.
//!
//! Matching follows the widely-implemented rules:
//! - The most specific user-agent group applies (longest matching token), else
//!   the `*` group; if none match, everything is allowed.
//! - Within a group the rule with the longest pattern wins; on a tie `Allow`
//!   beats `Disallow`.
//! - Patterns support `*` (any sequence) and a trailing `$` (end-anchor).

/// Parsed `robots.txt` rules, grouped by user-agent.
#[derive(Debug, Default, Clone)]
pub struct Robots {
    groups: Vec<Group>,
}

#[derive(Debug, Default, Clone)]
struct Group {
    /// Lowercased user-agent tokens this group applies to (`*` = catch-all).
    agents: Vec<String>,
    rules: Vec<Rule>,
    crawl_delay: Option<f64>,
}

#[derive(Debug, Clone)]
struct Rule {
    /// `true` for `Allow`, `false` for `Disallow`.
    allow: bool,
    /// The path pattern (may contain `*` and a trailing `$`).
    pattern: String,
}

impl Robots {
    /// Parse a `robots.txt` body. Unknown fields (`Sitemap`, `Host`, …) and
    /// malformed lines are ignored; this never fails.
    pub fn parse(content: &str) -> Robots {
        let mut groups: Vec<Group> = Vec::new();
        let mut cur: Option<Group> = None;
        // Consecutive `User-agent` lines share a group until a rule appears.
        let mut last_was_rule = false;

        for raw in content.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((field, value)) = line.split_once(':') else {
                continue;
            };
            let field = field.trim().to_ascii_lowercase();
            let value = value.trim().to_string();

            match field.as_str() {
                "user-agent" => {
                    if last_was_rule {
                        if let Some(g) = cur.take() {
                            groups.push(g);
                        }
                        last_was_rule = false;
                    }
                    cur.get_or_insert_with(Group::default)
                        .agents
                        .push(value.to_ascii_lowercase());
                }
                "disallow" => {
                    if let Some(g) = cur.as_mut() {
                        g.rules.push(Rule {
                            allow: false,
                            pattern: value,
                        });
                        last_was_rule = true;
                    }
                }
                "allow" => {
                    if let Some(g) = cur.as_mut() {
                        g.rules.push(Rule {
                            allow: true,
                            pattern: value,
                        });
                        last_was_rule = true;
                    }
                }
                "crawl-delay" => {
                    if let Some(g) = cur.as_mut() {
                        g.crawl_delay = value.parse().ok();
                        last_was_rule = true;
                    }
                }
                _ => {} // Sitemap / Host / etc.
            }
        }
        if let Some(g) = cur.take() {
            groups.push(g);
        }
        Robots { groups }
    }

    /// Whether `user_agent` may fetch `path` (e.g. `/foo/bar?x=1`).
    pub fn allows(&self, user_agent: &str, path: &str) -> bool {
        let Some(group) = self.group_for(user_agent) else {
            return true; // no applicable group → unrestricted
        };

        let mut best: Option<(usize, bool)> = None; // (pattern length, is_allow)
        for rule in &group.rules {
            // An empty Disallow means "allow all" — i.e. no constraint.
            if rule.pattern.is_empty() {
                continue;
            }
            if rule_matches(&rule.pattern, path) {
                let spec = rule.pattern.len();
                match best {
                    Some((bspec, _)) if spec < bspec => {}
                    // Tie → Allow wins.
                    Some((bspec, _)) if spec == bspec => {
                        if rule.allow {
                            best = Some((spec, true));
                        }
                    }
                    _ => best = Some((spec, rule.allow)),
                }
            }
        }
        best.is_none_or(|(_, allow)| allow)
    }

    /// The `Crawl-delay` (seconds) for `user_agent`'s applicable group, if any.
    pub fn crawl_delay(&self, user_agent: &str) -> Option<f64> {
        self.group_for(user_agent).and_then(|g| g.crawl_delay)
    }

    /// Pick the most specific group for `user_agent` (longest matching token),
    /// falling back to the `*` group.
    fn group_for(&self, user_agent: &str) -> Option<&Group> {
        let ua = user_agent.to_ascii_lowercase();
        let mut best: Option<(usize, &Group)> = None;
        let mut star: Option<&Group> = None;
        for g in &self.groups {
            for token in &g.agents {
                if token == "*" {
                    star.get_or_insert(g);
                } else if ua.contains(token.as_str()) && best.is_none_or(|(b, _)| token.len() > b) {
                    best = Some((token.len(), g));
                }
            }
        }
        best.map(|(_, g)| g).or(star)
    }
}

/// Match a robots path `pattern` (with `*` wildcards and an optional trailing
/// `$` anchor) against `path`. Patterns are anchored at the start of the path.
fn rule_matches(pattern: &str, path: &str) -> bool {
    let (pat, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };

    let segments: Vec<&str> = pat.split('*').collect();
    let mut pos = 0usize;
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            continue; // leading/trailing/consecutive '*'
        }
        if i == 0 {
            // The first literal must match at the start of the path.
            if !path[pos..].starts_with(seg) {
                return false;
            }
            pos += seg.len();
        } else {
            // Later literals may appear anywhere after the previous match.
            match path[pos..].find(seg) {
                Some(idx) => pos += idx + seg.len(),
                None => return false,
            }
        }
    }

    if anchored && !pat.ends_with('*') {
        // `$` requires the path to end exactly where the pattern did.
        return pos == path.len();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disallow_prefix_blocks_subpaths() {
        let r = Robots::parse("User-agent: *\nDisallow: /private");
        assert!(r.allows("anybot", "/public"));
        assert!(!r.allows("anybot", "/private"));
        assert!(!r.allows("anybot", "/private/secret"));
    }

    #[test]
    fn allow_overrides_more_specific_disallow() {
        let r = Robots::parse("User-agent: *\nDisallow: /a\nAllow: /a/b");
        assert!(!r.allows("bot", "/a"));
        assert!(r.allows("bot", "/a/b"), "more-specific Allow should win");
        assert!(!r.allows("bot", "/a/c"));
    }

    #[test]
    fn specific_user_agent_group_takes_precedence() {
        let txt = "User-agent: googlebot\nDisallow: /\n\nUser-agent: *\nDisallow: /private";
        let r = Robots::parse(txt);
        // Googlebot is disallowed everything by its own group.
        assert!(!r.allows("Mozilla/5.0 (compatible; Googlebot/2.1)", "/anything"));
        // Other bots fall to the `*` group.
        assert!(r.allows("randombot", "/anything"));
        assert!(!r.allows("randombot", "/private/x"));
    }

    #[test]
    fn empty_disallow_allows_everything() {
        let r = Robots::parse("User-agent: *\nDisallow:");
        assert!(r.allows("bot", "/anything/at/all"));
    }

    #[test]
    fn no_rules_allows_everything() {
        let r = Robots::parse("");
        assert!(r.allows("bot", "/x"));
        let r2 = Robots::parse("Sitemap: https://example.com/sitemap.xml");
        assert!(r2.allows("bot", "/x"));
    }

    #[test]
    fn wildcard_and_end_anchor() {
        let r = Robots::parse("User-agent: *\nDisallow: /*.php$");
        assert!(!r.allows("bot", "/index.php"));
        assert!(!r.allows("bot", "/dir/page.php"));
        // Anchored: trailing text after .php is not matched.
        assert!(r.allows("bot", "/index.phpx"));
        assert!(r.allows("bot", "/page.html"));
    }

    #[test]
    fn crawl_delay_is_parsed_per_group() {
        let txt = "User-agent: *\nCrawl-delay: 2.5\nDisallow: /x";
        let r = Robots::parse(txt);
        assert_eq!(r.crawl_delay("bot"), Some(2.5));
        let none = Robots::parse("User-agent: *\nDisallow: /x");
        assert_eq!(none.crawl_delay("bot"), None);
    }

    #[test]
    fn comments_and_whitespace_are_ignored() {
        let txt = "# a comment\nUser-agent: *   # trailing\n  Disallow: /tmp  # path\n";
        let r = Robots::parse(txt);
        assert!(!r.allows("bot", "/tmp/file"));
        assert!(r.allows("bot", "/ok"));
    }

    #[test]
    fn tie_between_equal_length_allow_and_disallow_favors_allow() {
        // Both "/p" rules match "/p" with equal length; Allow should win.
        let r = Robots::parse("User-agent: *\nDisallow: /p\nAllow: /p");
        assert!(r.allows("bot", "/p"));
    }
}
