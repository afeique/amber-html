//! Page metadata extraction from an HTML document. See `Plans.md`.
//!
//! [`extract`] pulls the title, document language, meta description, canonical
//! URL, OpenGraph properties, and outbound links out of a captured page. It is
//! best-effort and infallible: missing pieces are simply `None`/empty, and
//! garbage input yields an empty [`PageMetadata`] rather than an error.
//!
//! Relative URLs (canonical, links) are resolved against the page's final URL
//! so callers always receive absolute URLs.

use std::collections::BTreeMap;

use scraper::{Html, Selector};
use url::Url;

/// Structured metadata extracted from a captured page.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PageMetadata {
    /// `<title>` text, trimmed; `None` when absent or empty.
    pub title: Option<String>,
    /// `<html lang="…">`, if present.
    pub lang: Option<String>,
    /// `<meta name="description" content="…">`, if present.
    pub description: Option<String>,
    /// `<link rel="canonical" href="…">`, resolved to an absolute URL.
    pub canonical: Option<String>,
    /// The next page's URL (`rel="next"` on a `<link>` or `<a>`), resolved to
    /// an absolute URL — drives pagination (task 6.6).
    pub next_page: Option<String>,
    /// OpenGraph properties (`og:*`) keyed by their full property name
    /// (e.g. `og:title`), in sorted order.
    pub open_graph: BTreeMap<String, String>,
    /// Outbound `<a href>` links, resolved to absolute http(s) URLs, deduped,
    /// in document order.
    pub links: Vec<String>,
}

/// Parse a selector that is known at authoring time to be valid. A failure here
/// is a programmer error (a bad literal), so it surfaces immediately in tests.
fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector must be valid")
}

/// Extract [`PageMetadata`] from `html`, resolving relative URLs against
/// `base_url` (the page's final URL).
pub fn extract(html: &str, base_url: &Url) -> PageMetadata {
    let doc = Html::parse_document(html);

    let title = doc
        .select(&sel("title"))
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty());

    let lang = doc
        .select(&sel("html"))
        .next()
        .and_then(|el| el.value().attr("lang"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let description = doc
        .select(&sel(r#"meta[name="description"]"#))
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let canonical = doc
        .select(&sel(r#"link[rel="canonical"]"#))
        .next()
        .and_then(|el| el.value().attr("href"))
        .and_then(|href| base_url.join(href.trim()).ok())
        .map(|u| u.to_string());

    // The next page in a paginated sequence: <link rel="next"> or an
    // <a rel="next"> (rel is a space-separated token list, hence `~=`).
    let next_page = doc
        .select(&sel(r#"[rel~="next"][href]"#))
        .next()
        .and_then(|el| el.value().attr("href"))
        .and_then(|href| base_url.join(href.trim()).ok())
        .map(|u| u.to_string());

    let mut open_graph = BTreeMap::new();
    for el in doc.select(&sel(r#"meta[property^="og:"]"#)) {
        if let (Some(prop), Some(content)) =
            (el.value().attr("property"), el.value().attr("content"))
        {
            let prop = prop.trim();
            let content = content.trim();
            // First occurrence wins; OG arrays (e.g. multiple og:image) keep
            // the first, which is the canonical/primary value.
            if !prop.is_empty() && !content.is_empty() {
                open_graph
                    .entry(prop.to_string())
                    .or_insert_with(|| content.to_string());
            }
        }
    }

    let mut links = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for el in doc.select(&sel("a[href]")) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        let href = href.trim();
        // Skip empty and pure-fragment links (same page).
        if href.is_empty() || href.starts_with('#') {
            continue;
        }
        let Ok(resolved) = base_url.join(href) else {
            continue;
        };
        // Keep only navigable web links (drop mailto:/tel:/javascript:/data:).
        if !matches!(resolved.scheme(), "http" | "https") {
            continue;
        }
        let resolved = resolved.to_string();
        if seen.insert(resolved.clone()) {
            links.push(resolved);
        }
    }

    PageMetadata {
        title,
        lang,
        description,
        canonical,
        next_page,
        open_graph,
        links,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/blog/post").unwrap()
    }

    const DOC: &str = r##"
        <!DOCTYPE html>
        <html lang="en-US">
        <head>
            <title>  Amber HTML — Capture  </title>
            <meta name="description" content="A faithful web-page capture engine.">
            <link rel="canonical" href="/blog/post/">
            <meta property="og:title" content="Amber OG Title">
            <meta property="og:type" content="article">
            <meta property="og:image" content="https://cdn.example.com/a.png">
            <meta property="og:image" content="https://cdn.example.com/b.png">
            <meta name="viewport" content="width=device-width">
        </head>
        <body>
            <a href="/about">About</a>
            <a href="https://other.example.org/x">External</a>
            <a href="/about">About dup</a>
            <a href="#section">Anchor</a>
            <a href="mailto:hi@example.com">Mail</a>
            <a href="/about">About again</a>
        </body>
        </html>
    "##;

    #[test]
    fn extracts_title_lang_description() {
        let m = extract(DOC, &base());
        assert_eq!(m.title.as_deref(), Some("Amber HTML — Capture"));
        assert_eq!(m.lang.as_deref(), Some("en-US"));
        assert_eq!(
            m.description.as_deref(),
            Some("A faithful web-page capture engine.")
        );
    }

    #[test]
    fn canonical_resolves_to_absolute() {
        let m = extract(DOC, &base());
        assert_eq!(
            m.canonical.as_deref(),
            Some("https://example.com/blog/post/")
        );
    }

    #[test]
    fn next_page_from_link_rel_next_resolves_absolute() {
        let html = r#"<html><head><link rel="next" href="?page=2"></head><body>p1</body></html>"#;
        let m = extract(html, &base());
        assert_eq!(
            m.next_page.as_deref(),
            Some("https://example.com/blog/post?page=2")
        );
    }

    #[test]
    fn next_page_from_anchor_rel_next_and_absent_when_none() {
        // <a rel="prev next"> — rel is a token list, so `next` still matches.
        let with = r#"<html><body><a rel="prev next" href="/page/3">older</a></body></html>"#;
        assert_eq!(
            extract(with, &base()).next_page.as_deref(),
            Some("https://example.com/page/3")
        );
        // No rel=next anywhere → None.
        assert!(extract("<html><body>only</body></html>", &base())
            .next_page
            .is_none());
    }

    #[test]
    fn open_graph_collected_first_wins() {
        let m = extract(DOC, &base());
        assert_eq!(
            m.open_graph.get("og:title").map(String::as_str),
            Some("Amber OG Title")
        );
        assert_eq!(
            m.open_graph.get("og:type").map(String::as_str),
            Some("article")
        );
        // First og:image wins when duplicated.
        assert_eq!(
            m.open_graph.get("og:image").map(String::as_str),
            Some("https://cdn.example.com/a.png")
        );
        // Non-og meta is not included.
        assert!(!m.open_graph.contains_key("viewport"));
    }

    #[test]
    fn links_resolved_deduped_and_filtered() {
        let m = extract(DOC, &base());
        assert_eq!(
            m.links,
            vec![
                "https://example.com/about".to_string(),
                "https://other.example.org/x".to_string(),
            ]
        );
        // Fragment-only and mailto: links are dropped; /about appears once.
        assert!(!m.links.iter().any(|l| l.contains("#section")));
        assert!(!m.links.iter().any(|l| l.starts_with("mailto:")));
    }

    #[test]
    fn empty_and_garbage_input_yield_empty_metadata() {
        let empty = extract("", &base());
        assert_eq!(empty, PageMetadata::default());

        let garbage = extract("<<<not really html>>> &&&", &base());
        assert!(garbage.title.is_none());
        assert!(garbage.links.is_empty());
    }

    #[test]
    fn missing_pieces_are_none() {
        let html = "<html><body><p>no head</p></body></html>";
        let m = extract(html, &base());
        assert!(m.title.is_none());
        assert!(m.lang.is_none());
        assert!(m.description.is_none());
        assert!(m.canonical.is_none());
        assert!(m.open_graph.is_empty());
        assert!(m.links.is_empty());
    }
}
