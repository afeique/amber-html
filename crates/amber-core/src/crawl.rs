//! Crawl scope and frontier: the pure bookkeeping for a bounded multi-page
//! crawl — link-following, scope restriction, visited de-duplication, and
//! depth/page budgets. See `Plans.md` (task 3.1).
//!
//! This module is I/O-free. A crawl driver fetches each URL handed out by
//! [`Frontier::next_url`], extracts links (e.g. via [`crate::meta`]), and feeds them
//! back through [`Frontier::add_links`]; the frontier decides what is in scope,
//! unseen, and within budget.

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use url::Url;

use crate::cache::Cache;
use crate::robots::Robots;
use crate::{http, meta};

/// Which URLs a crawl is allowed to follow.
#[derive(Debug, Clone)]
pub struct CrawlScope {
    /// Lowercased host the crawl is anchored to.
    host: String,
    /// Also follow subdomains of `host`.
    include_subdomains: bool,
    /// Restrict to URLs whose path starts with this prefix.
    path_prefix: Option<String>,
}

impl CrawlScope {
    /// Scope a crawl to the seed URL's exact host.
    pub fn same_host(seed: &Url) -> Self {
        Self {
            host: seed.host_str().unwrap_or_default().to_ascii_lowercase(),
            include_subdomains: false,
            path_prefix: None,
        }
    }

    /// Also follow subdomains of the anchor host.
    pub fn including_subdomains(mut self, yes: bool) -> Self {
        self.include_subdomains = yes;
        self
    }

    /// Restrict the crawl to URLs under `prefix` (a path prefix like `/docs/`).
    pub fn under_path(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefix = Some(prefix.into());
        self
    }

    /// Whether `url` may be crawled under this scope.
    pub fn in_scope(&self, url: &Url) -> bool {
        if !matches!(url.scheme(), "http" | "https") {
            return false;
        }
        let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) else {
            return false;
        };
        let host_ok = if self.include_subdomains {
            host == self.host || host.ends_with(&format!(".{}", self.host))
        } else {
            host == self.host
        };
        if !host_ok {
            return false;
        }
        match &self.path_prefix {
            Some(prefix) => url.path().starts_with(prefix.as_str()),
            None => true,
        }
    }
}

/// Bounds and politeness for a crawl.
#[derive(Debug, Clone, Copy)]
pub struct CrawlLimits {
    /// Maximum number of pages handed out by [`Frontier::next_url`].
    pub max_pages: usize,
    /// Maximum link depth from the seed (seed is depth 0).
    pub max_depth: usize,
    /// Minimum delay (ms) between page fetches in [`crawl`]; the effective delay
    /// is the larger of this and any `robots.txt` `Crawl-delay`.
    pub min_delay_ms: u64,
}

impl Default for CrawlLimits {
    fn default() -> Self {
        Self {
            max_pages: 100,
            max_depth: 3,
            min_delay_ms: 0,
        }
    }
}

/// A breadth-first crawl frontier with scope, de-duplication, and budgets.
#[derive(Debug)]
pub struct Frontier {
    scope: CrawlScope,
    limits: CrawlLimits,
    queue: VecDeque<(Url, usize)>,
    seen: HashSet<String>,
    dispatched: usize,
}

impl Frontier {
    /// Create a frontier seeded with `seed` at depth 0.
    pub fn new(seed: Url, scope: CrawlScope, limits: CrawlLimits) -> Self {
        let mut f = Self {
            scope,
            limits,
            queue: VecDeque::new(),
            seen: HashSet::new(),
            dispatched: 0,
        };
        f.enqueue(seed, 0);
        f
    }

    /// Enqueue `url` at `depth`. Returns `true` if it was added — i.e. it is
    /// within the depth budget, in scope, and not already seen.
    pub fn enqueue(&mut self, url: Url, depth: usize) -> bool {
        if depth > self.limits.max_depth || !self.scope.in_scope(&url) {
            return false;
        }
        if !self.seen.insert(normalize(&url)) {
            return false;
        }
        self.queue.push_back((url, depth));
        true
    }

    /// Enqueue discovered `links` as children of a page at `parent_depth`,
    /// returning how many were newly added.
    pub fn add_links<I>(&mut self, parent_depth: usize, links: I) -> usize
    where
        I: IntoIterator<Item = Url>,
    {
        links
            .into_iter()
            .filter(|u| self.enqueue(u.clone(), parent_depth + 1))
            .count()
    }

    /// Hand out the next URL to crawl (with its depth), or `None` once the page
    /// budget is exhausted or the queue is empty.
    pub fn next_url(&mut self) -> Option<(Url, usize)> {
        if self.dispatched >= self.limits.max_pages {
            return None;
        }
        let item = self.queue.pop_front()?;
        self.dispatched += 1;
        Some(item)
    }

    /// Number of pages handed out so far.
    pub fn dispatched(&self) -> usize {
        self.dispatched
    }
}

/// Run a bounded breadth-first crawl from `seed`, using `fetch_links` to fetch
/// each page and return the links discovered on it. Returns the crawled URLs in
/// visit order, respecting scope, depth, the page budget, and de-duplication.
///
/// I/O-agnostic: `fetch_links` does whatever fetching/extraction the caller
/// wants (HTTP fetch + link extraction, a browser render, …). A page that
/// fails to fetch simply contributes no links but still counts as visited.
pub fn crawl_with<F>(seed: Url, scope: CrawlScope, limits: CrawlLimits, mut fetch_links: F) -> Vec<Url>
where
    F: FnMut(&Url) -> Vec<Url>,
{
    let mut frontier = Frontier::new(seed, scope, limits);
    let mut visited = Vec::new();
    while let Some((url, depth)) = frontier.next_url() {
        let links = fetch_links(&url);
        frontier.add_links(depth, links);
        visited.push(url);
    }
    visited
}

/// Like [`crawl_with`], but content-addressed: `fetch` returns each page's body
/// bytes alongside its links, and the crawl returns only the URLs whose body
/// **changed** versus `cache` (updating the cache as it goes). On a first run
/// with an empty cache every visited page is "changed"; on a re-run only pages
/// whose bytes differ are returned.
pub fn crawl_incremental_with<F>(
    seed: Url,
    scope: CrawlScope,
    limits: CrawlLimits,
    cache: &mut Cache,
    mut fetch: F,
) -> Vec<Url>
where
    F: FnMut(&Url) -> (Vec<u8>, Vec<Url>),
{
    let mut changed = Vec::new();
    crawl_with(seed, scope, limits, |url| {
        let (body, links) = fetch(url);
        let key = url.as_str();
        if !cache.is_unchanged(key, &body) {
            changed.push(url.clone());
        }
        cache.record(key, &body, None, None);
        links
    });
    changed
}

/// User-Agent for polite multi-page crawling. Unlike the cheap fetch tier (which
/// mimics a desktop browser to avoid UA-based blocking), the crawler identifies
/// itself honestly so site owners can recognize it and apply `robots.txt`.
pub const CRAWL_USER_AGENT: &str = concat!(
    "AmberHTML/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/afeique/amber-html)"
);

/// Run a bounded, polite crawl over HTTP from `seed`:
/// - fetch and honor `{origin}/robots.txt` (allow-all if it can't be fetched),
/// - identify with [`CRAWL_USER_AGENT`],
/// - wait [`politeness_delay`] between fetches (the larger of `min_delay_ms` and
///   the robots `Crawl-delay`),
/// - follow only in-scope, robots-allowed links.
///
/// Returns the crawled URLs in visit order. If the seed itself is disallowed by
/// `robots.txt`, nothing is crawled. See [`crawl_with`] for scope/depth/budget.
pub fn crawl(seed: Url, scope: CrawlScope, limits: CrawlLimits) -> Vec<Url> {
    let robots = fetch_robots(&seed);
    if !robots.allows(CRAWL_USER_AGENT, seed.path()) {
        return Vec::new();
    }
    let delay = politeness_delay(&robots, limits.min_delay_ms);

    let mut first = true;
    crawl_with(seed, scope, limits, |url| {
        if !first && delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        first = false;

        let links = match http::fetch_with_ua(url, CRAWL_USER_AGENT) {
            Ok(page) if http::content_type_is_html(page.content_type.as_deref()) => {
                meta::extract(&page.html, &page.final_url)
                    .links
                    .iter()
                    .filter_map(|l| Url::parse(l).ok())
                    .collect::<Vec<_>>()
            }
            _ => Vec::new(),
        };
        // Only follow robots-allowed links (scope is enforced by the frontier).
        links
            .into_iter()
            .filter(|l| robots.allows(CRAWL_USER_AGENT, l.path()))
            .collect()
    })
}

/// Incremental version of [`crawl`]: re-run a crawl and return only the pages
/// whose content changed since `cache` last saw them (by content hash),
/// updating `cache`. Honors `robots.txt`, the crawl UA, and politeness exactly
/// like [`crawl`]. First run with an empty cache returns every visited page.
pub fn crawl_incremental(
    seed: Url,
    scope: CrawlScope,
    limits: CrawlLimits,
    cache: &mut Cache,
) -> Vec<Url> {
    let robots = fetch_robots(&seed);
    if !robots.allows(CRAWL_USER_AGENT, seed.path()) {
        return Vec::new();
    }
    let delay = politeness_delay(&robots, limits.min_delay_ms);

    let mut first = true;
    crawl_incremental_with(seed, scope, limits, cache, |url| {
        if !first && delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        first = false;

        match http::fetch_with_ua(url, CRAWL_USER_AGENT) {
            Ok(page) if http::content_type_is_html(page.content_type.as_deref()) => {
                let links = meta::extract(&page.html, &page.final_url)
                    .links
                    .iter()
                    .filter_map(|l| Url::parse(l).ok())
                    .filter(|l| robots.allows(CRAWL_USER_AGENT, l.path()))
                    .collect();
                (page.html.into_bytes(), links)
            }
            _ => (Vec::new(), Vec::new()),
        }
    })
}

/// Fetch and parse `{origin}/robots.txt` for `seed`. Any failure (network,
/// non-2xx, …) yields an empty (allow-all) ruleset.
fn fetch_robots(seed: &Url) -> Robots {
    let Ok(robots_url) = seed.join("/robots.txt") else {
        return Robots::parse("");
    };
    match http::fetch_with_ua(&robots_url, CRAWL_USER_AGENT) {
        Ok(page) => Robots::parse(&page.html),
        Err(_) => Robots::parse(""),
    }
}

/// Effective politeness delay (ms): the larger of the configured `floor_ms` and
/// the `robots.txt` `Crawl-delay` (seconds → ms) for [`CRAWL_USER_AGENT`].
fn politeness_delay(robots: &Robots, floor_ms: u64) -> u64 {
    let robots_ms = robots
        .crawl_delay(CRAWL_USER_AGENT)
        .map(|secs| (secs * 1000.0) as u64)
        .unwrap_or(0);
    floor_ms.max(robots_ms)
}

/// Normalize a URL for de-duplication: drop the fragment (scheme/host are
/// already lowercased by the URL parser; paths stay case-sensitive).
fn normalize(url: &Url) -> String {
    let mut u = url.clone();
    u.set_fragment(None);
    u.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn same_host_scope_excludes_other_hosts_and_schemes() {
        let scope = CrawlScope::same_host(&url("https://example.com/"));
        assert!(scope.in_scope(&url("https://example.com/page")));
        assert!(!scope.in_scope(&url("https://other.com/page")));
        // Subdomains excluded by default.
        assert!(!scope.in_scope(&url("https://blog.example.com/page")));
        // Non-web schemes excluded.
        assert!(!scope.in_scope(&url("ftp://example.com/x")));
    }

    #[test]
    fn subdomains_and_path_prefix_scope() {
        let scope = CrawlScope::same_host(&url("https://example.com/"))
            .including_subdomains(true)
            .under_path("/docs/");
        assert!(scope.in_scope(&url("https://example.com/docs/intro")));
        assert!(scope.in_scope(&url("https://api.example.com/docs/v1")));
        // Right host/subdomain but wrong path.
        assert!(!scope.in_scope(&url("https://example.com/blog/x")));
        // ".example.com" suffix must be a real subdomain, not a look-alike.
        assert!(!scope.in_scope(&url("https://notexample.com/docs/x")));
    }

    #[test]
    fn frontier_seeds_and_dedupes() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let mut f = Frontier::new(seed.clone(), scope, CrawlLimits::default());
        // Seed is queued; re-enqueuing it (or a fragment variant) is a no-op.
        assert!(!f.enqueue(seed.clone(), 0));
        assert!(!f.enqueue(url("https://example.com/#section"), 0));
        let (first, depth) = f.next_url().unwrap();
        assert_eq!(first, seed);
        assert_eq!(depth, 0);
    }

    #[test]
    fn add_links_enqueues_in_scope_children_at_next_depth() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let mut f = Frontier::new(seed, scope, CrawlLimits::default());
        let (_seed, d0) = f.next_url().unwrap();
        let added = f.add_links(
            d0,
            [
                url("https://example.com/a"),
                url("https://other.com/b"), // out of scope
                url("https://example.com/a#frag"), // dup of /a
                url("https://example.com/c"),
            ],
        );
        assert_eq!(added, 2, "only the two unique in-scope links");
        let (_a, d1) = f.next_url().unwrap();
        assert_eq!(d1, 1, "children are one level deeper than the seed");
    }

    #[test]
    fn depth_budget_is_enforced() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let limits = CrawlLimits {
            max_pages: 100,
            max_depth: 1,
            min_delay_ms: 0,
        };
        let mut f = Frontier::new(seed, scope, limits);
        assert!(f.enqueue(url("https://example.com/a"), 1)); // at the limit
        assert!(!f.enqueue(url("https://example.com/b"), 2)); // beyond it
    }

    #[test]
    fn page_budget_caps_dispatch() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let limits = CrawlLimits {
            max_pages: 2,
            max_depth: 5,
            min_delay_ms: 0,
        };
        let mut f = Frontier::new(seed, scope, limits);
        f.enqueue(url("https://example.com/a"), 1);
        f.enqueue(url("https://example.com/b"), 1);
        assert!(f.next_url().is_some()); // seed
        assert!(f.next_url().is_some()); // /a
        assert!(f.next_url().is_none(), "page budget of 2 reached");
        assert_eq!(f.dispatched(), 2);
    }

    // ---- crawl_with (driver, mock fetcher) -------------------------------

    fn paths(urls: &[Url]) -> Vec<&str> {
        urls.iter().map(|u| u.path()).collect()
    }

    #[test]
    fn crawl_with_visits_in_scope_graph_breadth_first() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let fetch = |u: &Url| match u.path() {
            "/" => vec![url("https://example.com/a"), url("https://example.com/b")],
            "/a" => vec![url("https://example.com/c"), url("https://other.com/x")],
            "/b" => vec![url("https://example.com/a")], // duplicate
            _ => vec![],
        };
        let visited = crawl_with(seed, scope, CrawlLimits::default(), fetch);
        // BFS order; the external link and the duplicate are skipped.
        assert_eq!(paths(&visited), vec!["/", "/a", "/b", "/c"]);
    }

    #[test]
    fn crawl_with_respects_page_budget() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let fetch = |u: &Url| match u.path() {
            "/" => vec![url("https://example.com/a"), url("https://example.com/b")],
            _ => vec![],
        };
        let limits = CrawlLimits {
            max_pages: 2,
            max_depth: 5,
            min_delay_ms: 0,
        };
        let visited = crawl_with(seed, scope, limits, fetch);
        assert_eq!(visited.len(), 2);
    }

    #[test]
    fn crawl_with_respects_depth_budget() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let fetch = |u: &Url| match u.path() {
            "/" => vec![url("https://example.com/a")],
            "/a" => vec![url("https://example.com/b")], // would be depth 2
            _ => vec![],
        };
        let limits = CrawlLimits {
            max_pages: 100,
            max_depth: 1,
            min_delay_ms: 0,
        };
        let visited = crawl_with(seed, scope, limits, fetch);
        assert_eq!(paths(&visited), vec!["/", "/a"]);
    }

    #[test]
    fn crawl_with_robots_skips_disallowed_links() {
        // Compose robots filtering with the driver (no network): the fetcher
        // returns only robots-allowed links, exactly as crawl() does.
        let robots = Robots::parse("User-agent: *\nDisallow: /private");
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let fetch = |u: &Url| {
            let links = match u.path() {
                "/" => vec![
                    url("https://example.com/ok"),
                    url("https://example.com/private/secret"),
                ],
                _ => vec![],
            };
            links
                .into_iter()
                .filter(|l| robots.allows(CRAWL_USER_AGENT, l.path()))
                .collect()
        };
        let visited = crawl_with(seed, scope, CrawlLimits::default(), fetch);
        assert_eq!(paths(&visited), vec!["/", "/ok"], "/private/* disallowed");
    }

    #[test]
    fn politeness_delay_takes_max_of_floor_and_robots() {
        let with_cd = Robots::parse("User-agent: *\nCrawl-delay: 2"); // 2000 ms
        assert_eq!(politeness_delay(&with_cd, 500), 2000); // robots wins
        assert_eq!(politeness_delay(&with_cd, 3000), 3000); // floor wins
        let no_cd = Robots::parse("User-agent: *\nDisallow: /x");
        assert_eq!(politeness_delay(&no_cd, 750), 750);
    }

    #[test]
    fn crawl_user_agent_is_identifiable() {
        assert!(CRAWL_USER_AGENT.starts_with("AmberHTML/"));
        assert!(CRAWL_USER_AGENT.contains("github.com/afeique/amber-html"));
    }

    #[test]
    fn incremental_crawl_returns_only_changed_pages_on_rerun() {
        use crate::cache::Cache;

        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let limits = CrawlLimits::default();
        let mut cache = Cache::new();

        // A 2-page site; `/a`'s body is parameterized so we can change it.
        let make_fetch = |a_body: &'static str| {
            move |u: &Url| -> (Vec<u8>, Vec<Url>) {
                match u.path() {
                    "/" => (b"home v1".to_vec(), vec![url("https://example.com/a")]),
                    "/a" => (a_body.as_bytes().to_vec(), vec![]),
                    _ => (Vec::new(), vec![]),
                }
            }
        };

        // First run: empty cache → every visited page is "changed".
        let first =
            crawl_incremental_with(seed.clone(), scope.clone(), limits, &mut cache, make_fetch("a v1"));
        assert_eq!(first.len(), 2);

        // Re-run with identical content → nothing changed.
        let second =
            crawl_incremental_with(seed.clone(), scope.clone(), limits, &mut cache, make_fetch("a v1"));
        assert!(second.is_empty());

        // Re-run with /a changed → only /a is returned.
        let third = crawl_incremental_with(seed, scope, limits, &mut cache, make_fetch("a v2"));
        assert_eq!(paths(&third), vec!["/a"]);
    }

    #[test]
    #[ignore = "performs real network requests; run with --ignored"]
    fn incremental_crawl_skips_unchanged_on_rerun_live() {
        use crate::cache::Cache;
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let mut cache = Cache::new();
        let first = crawl_incremental(seed.clone(), scope.clone(), CrawlLimits::default(), &mut cache);
        assert_eq!(first.len(), 1, "first run captures the (new) home page");
        let second = crawl_incremental(seed, scope, CrawlLimits::default(), &mut cache);
        assert!(second.is_empty(), "static page is unchanged on immediate re-run");
    }

    #[test]
    #[ignore = "performs real network requests; run with --ignored"]
    fn crawl_example_com_stays_in_scope() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let visited = crawl(seed, scope, CrawlLimits::default());
        // example.com links only to iana.org (out of scope), so just the seed.
        assert_eq!(visited.len(), 1);
        assert_eq!(visited[0].path(), "/");
    }
}
