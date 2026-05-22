//! Sitemap ingestion: extract URLs from a `sitemap.xml` (or sitemap index) to
//! seed a crawl. See `Plans.md` (task 3.8).
//!
//! [`parse_sitemap`] is pure: it pulls every `<loc>` value out of the document
//! (works for both `<urlset>` page sitemaps and `<sitemapindex>` documents,
//! which both use `<loc>`), decodes the common XML entities, and returns the
//! valid absolute URLs. [`fetch_sitemap`] fetches one over HTTP and parses it.

use url::Url;

/// Extract the URLs from a sitemap or sitemap-index document by reading every
/// `<loc>…</loc>` value. Best-effort and infallible: malformed entries are
/// skipped and a non-sitemap input simply yields no URLs.
pub fn parse_sitemap(xml: &str) -> Vec<Url> {
    let lower = xml.to_ascii_lowercase();
    let mut urls = Vec::new();
    let mut pos = 0usize;
    while let Some(rel) = lower[pos..].find("<loc>") {
        let start = pos + rel + "<loc>".len();
        let Some(end_rel) = lower[start..].find("</loc>") else {
            break;
        };
        let raw = xml[start..start + end_rel].trim();
        if let Ok(url) = Url::parse(&decode_xml_entities(raw)) {
            urls.push(url);
        }
        pos = start + end_rel + "</loc>".len();
    }
    urls
}

/// Fetch a sitemap over HTTP (identifying as the crawler) and parse its URLs.
/// Returns an empty list on any fetch failure.
pub fn fetch_sitemap(url: &Url) -> Vec<Url> {
    match crate::http::fetch_with_ua(url, crate::crawl::CRAWL_USER_AGENT) {
        Ok(page) => parse_sitemap(&page.html),
        Err(_) => Vec::new(),
    }
}

/// Decode the XML entities that appear in sitemap `<loc>` values. `&amp;` is
/// decoded last so an already-decoded `&` isn't reinterpreted.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawl::{CrawlLimits, CrawlScope, Frontier};

    const URLSET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/</loc><lastmod>2026-01-01</lastmod></url>
  <url><loc>https://example.com/about</loc></url>
  <url><loc>https://example.com/search?q=a&amp;p=2</loc></url>
</urlset>"#;

    #[test]
    fn parses_urlset_locs() {
        let urls = parse_sitemap(URLSET);
        let strs: Vec<String> = urls.iter().map(|u| u.to_string()).collect();
        assert_eq!(
            strs,
            vec![
                "https://example.com/".to_string(),
                "https://example.com/about".to_string(),
                // &amp; decoded back to & in the query.
                "https://example.com/search?q=a&p=2".to_string(),
            ]
        );
    }

    #[test]
    fn parses_sitemap_index_locs() {
        let index = r#"<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://example.com/sitemap-1.xml</loc></sitemap>
  <sitemap><loc>https://example.com/sitemap-2.xml</loc></sitemap>
</sitemapindex>"#;
        let urls = parse_sitemap(index);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].path().ends_with("sitemap-1.xml"));
    }

    #[test]
    fn skips_malformed_and_handles_empty() {
        assert!(parse_sitemap("").is_empty());
        assert!(parse_sitemap("<urlset></urlset>").is_empty());
        // A non-URL <loc> is skipped, valid ones kept.
        let mixed = "<urlset><url><loc>not a url</loc></url>\
                     <url><loc>https://example.com/ok</loc></url></urlset>";
        let urls = parse_sitemap(mixed);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].as_str(), "https://example.com/ok");
    }

    #[test]
    fn case_insensitive_loc_tags() {
        let urls = parse_sitemap("<URLSET><URL><LOC>https://example.com/x</LOC></URL></URLSET>");
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].path(), "/x");
    }

    #[test]
    fn sitemap_urls_can_seed_a_crawl_frontier() {
        // The whole point of 3.8: feed parsed sitemap URLs into the frontier.
        let urls = parse_sitemap(URLSET);
        let seed = urls[0].clone();
        let scope = CrawlScope::same_host(&seed);
        let mut frontier = Frontier::new(seed, scope, CrawlLimits::default());
        let queued: usize = frontier.add_links(0, urls.into_iter().skip(1));
        // /about and /search?... are in scope and not yet seen → both queued.
        assert_eq!(queued, 2);
    }
}
