//! Self-healing selection: try candidate CSS selectors in order and use the
//! first that yields content, so extraction recovers when a page's primary
//! selector drifts (renamed class, restructured DOM, …). See `Plans.md` (6.4).

use scraper::{Html, Selector};

/// Return the trimmed text of the first **non-empty** element matched by any of
/// `candidates`, tried in order. Falls through to the next candidate when one
/// matches nothing (or only empty elements); invalid selectors are skipped.
/// `None` if no candidate yields text.
pub fn select_first_text(html: &str, candidates: &[&str]) -> Option<String> {
    let doc = Html::parse_document(html);
    for cand in candidates {
        let Ok(selector) = Selector::parse(cand) else {
            continue;
        };
        for el in doc.select(&selector) {
            let text = el.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Return the trimmed texts of all elements matched by the **first candidate**
/// that matches anything non-empty (tried in order). Empty otherwise.
pub fn select_all_text(html: &str, candidates: &[&str]) -> Vec<String> {
    let doc = Html::parse_document(html);
    for cand in candidates {
        let Ok(selector) = Selector::parse(cand) else {
            continue;
        };
        let texts: Vec<String> = doc
            .select(&selector)
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if !texts.is_empty() {
            return texts;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<html><body>
        <h1 class="headline">New Title</h1>
        <div class="byline">By Jane</div>
        <ul class="tags"><li>rust</li><li>cli</li></ul>
    </body></html>"#;

    #[test]
    fn uses_primary_selector_when_present() {
        assert_eq!(
            select_first_text(DOC, &["h1.headline", "h1"]).as_deref(),
            Some("New Title")
        );
    }

    #[test]
    fn heals_when_primary_selector_drifts() {
        // ".title" (old class) no longer matches; the chain falls back.
        assert_eq!(
            select_first_text(DOC, &[".title", "h1.headline"]).as_deref(),
            Some("New Title")
        );
    }

    #[test]
    fn returns_none_when_no_candidate_matches() {
        assert_eq!(select_first_text(DOC, &[".missing", "#gone"]), None);
    }

    #[test]
    fn skips_invalid_selectors() {
        // The first selector is invalid CSS and is skipped, not fatal.
        assert_eq!(
            select_first_text(DOC, &[">>bad<<", ".byline"]).as_deref(),
            Some("By Jane")
        );
    }

    #[test]
    fn select_all_text_uses_first_matching_candidate() {
        let tags = select_all_text(DOC, &["p", ".tags li"]);
        assert_eq!(tags, vec!["rust".to_string(), "cli".to_string()]);
    }
}
