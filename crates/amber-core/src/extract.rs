//! HTML → Markdown and HTML → readable-text extraction.
//!
//! Two best-effort, infallible entry points back the `--markdown` and
//! `--readable` outputs (Plans.md). Both take a full
//! HTML document and return a `String`; neither panics and both degrade
//! gracefully on garbage input.
//!
//! - [`to_markdown`] converts the *whole* document to clean, LLM-friendly
//!   Markdown via [`htmd`] (a Rust port of turndown). It does not run a
//!   main-content extractor first — Markdown captures the page faithfully and
//!   the caller decides how to budget it.
//! - [`to_readable`] runs the Readability algorithm via [`dom_smoothie`] to
//!   isolate the main article (dropping nav / footer / aside / ads / cookie
//!   banners) and returns its **plain text**.
//!
//! ## Encoding
//! Both functions take `&str`, i.e. the HTML must already be valid UTF-8. The
//! capture pipeline (`fetch`/`capture`) is responsible for transcoding from the
//! page's declared charset to UTF-8 before calling here.
//!
//! ## Links / base URL
//! `htmd` emits whatever `href`/`src` values are present in the HTML verbatim;
//! it does not resolve relative URLs. If absolute links are wanted in the
//! Markdown, the caller should rewrite relative URLs against the page's final
//! URL *before* calling [`to_markdown`] (or inject a `<base href>` into the
//! document). `to_readable` returns plain text, so links there collapse to
//! their anchor text regardless.

use dom_smoothie::{Config, Readability};
use htmd::options::{HeadingStyle, LinkStyle, Options};
use htmd::HtmlToMarkdown;
use scraper::{Html, Selector};

/// Convert a full HTML document to clean, LLM-friendly Markdown.
///
/// Best-effort and infallible: on a converter error (or empty/garbage input)
/// this returns an empty `String` rather than panicking.
///
/// `<script>`, `<style>`, `<head>`, and `<noscript>` content is dropped so the
/// output is prose, not machinery. ATX headings (`#`) and inline links are used
/// for predictable, token-frugal output.
pub fn to_markdown(html: &str) -> String {
    let converter = HtmlToMarkdown::builder()
        // Drop non-content machinery. `head` removes <title>/<meta>/<link>
        // boilerplate; the others strip scripts, styling, and JS-off fallbacks.
        .skip_tags(vec!["script", "style", "head", "noscript", "template", "iframe"])
        .options(Options {
            // ATX (`# Heading`) is more robust than Setext for arbitrary depth
            // and friendlier to downstream Markdown parsers / LLMs.
            heading_style: HeadingStyle::Atx,
            // Inline links keep anchor text and URL together, which reads better
            // for an LLM than reference-style footnotes scattered at the end.
            link_style: LinkStyle::Inlined,
            ..Default::default()
        })
        .build();

    // Strip site chrome (nav/footer/aside, cookie/consent and ad containers)
    // before conversion so the Markdown is content, not boilerplate.
    let cleaned = clean_boilerplate(html);
    match converter.convert(&cleaned) {
        Ok(md) => md.trim().to_string(),
        // htmd only errors on internal write failures, which effectively never
        // happen for an in-memory buffer; degrade to empty rather than panic.
        Err(_) => String::new(),
    }
}

/// Common non-content "chrome" stripped before Markdown conversion: site
/// navigation, footers, asides, ARIA landmark regions, and high-confidence
/// cookie/consent and ad containers. Deliberately conservative — only
/// structural tags, landmark roles, and distinctive class/id patterns are
/// dropped, to avoid removing real content. (`to_readable` relies on the
/// Readability algorithm for the same job.)
const BOILERPLATE_SELECTORS: &[&str] = &[
    "nav",
    "footer",
    "aside",
    "[role=\"navigation\"]",
    "[role=\"banner\"]",
    "[role=\"contentinfo\"]",
    "[role=\"complementary\"]",
    "[class*=\"cookie-banner\" i]",
    "[class*=\"cookie-consent\" i]",
    "[id*=\"cookie-banner\" i]",
    "[id*=\"cookie-consent\" i]",
    "[aria-label*=\"cookie\" i]",
    "[id*=\"onetrust\" i]",
    "[class*=\"advertisement\" i]",
    "ins.adsbygoogle",
];

/// Remove [`BOILERPLATE_SELECTORS`] elements from `html`, returning the cleaned
/// document re-serialized as a string.
///
/// Best-effort and infallible: unparseable selectors are skipped and the
/// document is always returned (html5ever normalizes even malformed input).
fn clean_boilerplate(html: &str) -> String {
    let mut doc = Html::parse_document(html);

    // Collect matching node ids first (an immutable borrow), then detach them.
    let mut ids = Vec::new();
    for s in BOILERPLATE_SELECTORS {
        if let Ok(selector) = Selector::parse(s) {
            ids.extend(doc.select(&selector).map(|el| el.id()));
        }
    }
    for id in ids {
        if let Some(mut node) = doc.tree.get_mut(id) {
            node.detach();
        }
    }

    doc.root_element().html()
}

/// Extract the main content of an HTML page as clean, readable **plain text**.
///
/// Runs the Readability algorithm (via [`dom_smoothie`]) to strip navigation,
/// footers, asides, ads, and other boilerplate, then returns the article's
/// text content with surrounding whitespace trimmed.
///
/// Best-effort and infallible: if Readability cannot find a confident main
/// article (e.g. the page is not article-shaped, or the input is empty/garbage)
/// this falls back to a crude tag-stripped rendering of the whole document, and
/// finally to an empty `String`. Never panics.
pub fn to_readable(html: &str) -> String {
    // `document_url = None` is fine: we want text, not resolved links, so an
    // absolute base URL is unnecessary. Default config mirrors Mozilla's
    // Readability defaults.
    match Readability::new(html, None, Some(Config::default())) {
        Ok(mut readability) => match readability.parse() {
            Ok(article) => {
                let text = article.text_content.trim();
                if text.is_empty() {
                    fallback_text(html)
                } else {
                    text.to_string()
                }
            }
            // No confident main article — fall back to a whole-page strip.
            Err(_) => fallback_text(html),
        },
        // Parsing/setup failed (e.g. malformed input) — fall back.
        Err(_) => fallback_text(html),
    }
}

/// Detect the natural language of `text`, returning its ISO 639-3 code
/// (e.g. `"eng"`, `"fra"`). Returns `None` for empty input or text too short to
/// classify with any confidence.
///
/// Uses [`whatlang`], a script- and trigram-based detector that needs no model
/// download. Run it on already-extracted text (e.g. [`to_readable`] output) so
/// markup and boilerplate do not skew the result.
pub fn detect_language(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    whatlang::detect(text).map(|info| info.lang().code().to_string())
}

/// Crude last-resort plain-text extraction when Readability declines to extract
/// a main article. Removes `<script>`/`<style>` blocks, strips all remaining
/// tags, decodes a handful of common HTML entities, and collapses whitespace.
///
/// This is intentionally simple — it is the degraded path, not the happy path.
fn fallback_text(html: &str) -> String {
    let without_blocks = strip_block("style", &strip_block("script", html));

    // Strip tags: copy through everything outside of `<...>` spans. Treat a
    // stray `<` with no closing `>` as literal text so we never lose the tail.
    let mut out = String::with_capacity(without_blocks.len());
    let mut in_tag = false;
    for ch in without_blocks.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }

    let decoded = decode_basic_entities(&out);
    collapse_whitespace(&decoded)
}

/// Remove `<tag>...</tag>` regions (case-insensitive) wholesale, including the
/// tags themselves. Used to drop `<script>`/`<style>` bodies before tag
/// stripping so their contents don't leak into the text.
fn strip_block(tag: &str, html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0usize;
    while cursor < html.len() {
        match lower[cursor..].find(&open) {
            Some(rel_start) => {
                let start = cursor + rel_start;
                out.push_str(&html[cursor..start]);
                // Find the matching close tag after the open; if none, drop the
                // remainder (an unterminated script/style block).
                match lower[start..].find(&close) {
                    Some(rel_end) => {
                        cursor = start + rel_end + close.len();
                    }
                    None => break,
                }
            }
            None => {
                out.push_str(&html[cursor..]);
                break;
            }
        }
    }
    out
}

/// Decode the small set of HTML entities common in body text. Enough for the
/// degraded fallback path; the Readability happy path handles entities itself.
fn decode_basic_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

/// Collapse runs of whitespace, preserving paragraph breaks: blank-line-
/// separated chunks become single `\n\n`-joined paragraphs with internal
/// whitespace squeezed to single spaces.
fn collapse_whitespace(s: &str) -> String {
    s.split("\n\n")
        .map(|para| para.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|para| !para.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>Rustaceans Guide</title>
  <meta charset="utf-8">
  <style>.x { color: red; }</style>
</head>
<body>
  <article>
    <h1>Getting Started with Rust</h1>
    <p>Rust is a <strong>systems</strong> language focused on
       safety and speed. Visit <a href="https://rust-lang.org">the site</a>.</p>
    <h2>Why Rust</h2>
    <ul>
      <li>Memory safety</li>
      <li>Fearless concurrency</li>
      <li>Zero-cost abstractions</li>
    </ul>
    <p>Read the <a href="/book">official book</a> next.</p>
  </article>
  <script>console.log("tracking");</script>
</body>
</html>"#;

    // ---- to_markdown -----------------------------------------------------

    #[test]
    fn markdown_renders_headings() {
        let md = to_markdown(ARTICLE);
        assert!(md.contains("# Getting Started with Rust"), "h1 missing:\n{md}");
        assert!(md.contains("## Why Rust"), "h2 missing:\n{md}");
    }

    #[test]
    fn markdown_renders_paragraph_text_and_emphasis() {
        let md = to_markdown(ARTICLE);
        assert!(md.contains("systems"), "paragraph text missing:\n{md}");
        // bold rendered with ** by default
        assert!(md.contains("**systems**"), "strong not bolded:\n{md}");
    }

    #[test]
    fn markdown_renders_inline_links() {
        let md = to_markdown(ARTICLE);
        assert!(
            md.contains("[the site](https://rust-lang.org)"),
            "absolute inline link missing:\n{md}"
        );
        // Relative hrefs are emitted verbatim (no base-URL resolution here).
        assert!(md.contains("[official book](/book)"), "relative link missing:\n{md}");
    }

    const CHROME_DOC: &str = r##"<!DOCTYPE html><html><body>
        <nav><a href="/">Home</a> <a href="/about">About</a></nav>
        <div class="cookie-banner">We use cookies on this site. Accept?</div>
        <div class="advertisement">Buy our product now!</div>
        <article>
          <h1>Genuine Headline</h1>
          <p>This is the authentic article body that should survive cleaning.</p>
        </article>
        <aside>Related sidebar links here</aside>
        <footer>Copyright 2026 Example Incorporated.</footer>
        </body></html>"##;

    #[test]
    fn markdown_strips_nav_footer_aside() {
        let md = to_markdown(CHROME_DOC);
        assert!(md.contains("Genuine Headline"), "article heading lost:\n{md}");
        assert!(md.contains("authentic article body"), "article body lost:\n{md}");
        assert!(!md.contains("About"), "nav not stripped:\n{md}");
        assert!(!md.contains("Copyright"), "footer not stripped:\n{md}");
        assert!(!md.contains("Related sidebar"), "aside not stripped:\n{md}");
    }

    #[test]
    fn markdown_strips_cookie_and_ad_banners() {
        let md = to_markdown(CHROME_DOC);
        assert!(!md.contains("We use cookies"), "cookie banner not stripped:\n{md}");
        assert!(!md.contains("Buy our product"), "ad not stripped:\n{md}");
    }

    #[test]
    fn markdown_renders_list_items() {
        let md = to_markdown(ARTICLE);
        assert!(md.contains("Memory safety"), "list item missing:\n{md}");
        assert!(md.contains("Fearless concurrency"), "list item missing:\n{md}");
        assert!(md.contains("Zero-cost abstractions"), "list item missing:\n{md}");
        // bullet list markers present
        assert!(
            md.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with("- ") || t.starts_with("* ")
            }),
            "no bullet markers found:\n{md}"
        );
    }

    #[test]
    fn markdown_drops_script_and_style() {
        let md = to_markdown(ARTICLE);
        assert!(!md.contains("tracking"), "script body leaked:\n{md}");
        assert!(!md.contains("color: red"), "style body leaked:\n{md}");
    }

    #[test]
    fn markdown_empty_input_is_empty_not_panic() {
        assert_eq!(to_markdown(""), "");
    }

    #[test]
    fn markdown_garbage_input_does_not_panic() {
        // Unbalanced / nonsense markup must not panic.
        let _ = to_markdown("<<<not >really< html &&& <p>hi");
        let _ = to_markdown("just some plain text with no tags at all");
    }

    // ---- to_readable -----------------------------------------------------

    const PAGE_WITH_BOILERPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head><title>News Site</title></head>
<body>
  <header>
    <nav>
      <a href="/">Home</a>
      <a href="/world">World</a>
      <a href="/sports">Sports</a>
      <a href="/login">Sign in to your account</a>
    </nav>
  </header>
  <aside class="ad">
    <p>SPECIAL OFFER! Buy now and save 50% on your subscription today only!</p>
  </aside>
  <main>
    <article>
      <h1>Local River Cleanup Draws Hundreds of Volunteers</h1>
      <p>On a bright Saturday morning, more than three hundred residents gathered
         along the banks of the Willow River to take part in the largest community
         cleanup the town has ever organized. Armed with gloves, rakes, and
         reusable bags, they fanned out across nearly four miles of shoreline.</p>
      <p>Organizers said the event removed an estimated two tons of debris, ranging
         from discarded tires to tangled fishing line. The mayor praised the turnout
         and promised that the city would match the volunteer hours with new funding
         for riverbank restoration over the coming year.</p>
      <p>By early afternoon, the once-littered stretch of riverbank looked
         transformed, and several families stayed behind to plant native grasses
         intended to stabilize the soil and filter runoff before it reaches the water.</p>
    </article>
  </main>
  <footer>
    <p>Copyright 2026 News Site. All rights reserved. Contact us | Privacy policy | Terms of service</p>
  </footer>
  <script>analytics.track("pageview");</script>
</body>
</html>"#;

    #[test]
    fn readable_keeps_article_body() {
        let text = to_readable(PAGE_WITH_BOILERPLATE);
        assert!(
            text.contains("three hundred residents gathered"),
            "article body missing:\n{text}"
        );
        assert!(
            text.contains("estimated two tons of debris"),
            "second paragraph missing:\n{text}"
        );
        assert!(
            text.contains("plant native grasses"),
            "third paragraph missing:\n{text}"
        );
    }

    #[test]
    fn readable_drops_nav_footer_and_ads() {
        let text = to_readable(PAGE_WITH_BOILERPLATE);
        assert!(!text.contains("Sign in to your account"), "nav leaked:\n{text}");
        assert!(!text.contains("Sports"), "nav link leaked:\n{text}");
        assert!(!text.contains("SPECIAL OFFER"), "ad leaked:\n{text}");
        assert!(!text.contains("All rights reserved"), "footer leaked:\n{text}");
        assert!(!text.contains("Privacy policy"), "footer leaked:\n{text}");
    }

    #[test]
    fn readable_is_plain_text_not_markup() {
        let text = to_readable(PAGE_WITH_BOILERPLATE);
        assert!(!text.contains('<'), "stray markup in readable text:\n{text}");
        assert!(!text.contains("analytics.track"), "script leaked:\n{text}");
    }

    #[test]
    fn readable_empty_input_is_empty_not_panic() {
        assert_eq!(to_readable(""), "");
    }

    #[test]
    fn readable_garbage_input_does_not_panic() {
        let _ = to_readable("<<<broken <span>fragment &amp; more");
        let _ = to_readable("plain text, no tags");
    }

    #[test]
    fn readable_fallback_strips_tags_and_scripts() {
        // A non-article fragment Readability may decline; the fallback must still
        // yield clean text with no tags, no script bodies, decoded entities.
        // NB: avoid `&lt;`/`&gt;` in the body here — decoding them would
        // legitimately reintroduce `<`/`>`, which would defeat the no-markup
        // assertion below. Use `&amp;` to exercise entity decoding instead.
        let html = "<div><script>steal()</script><p>Tom &amp; Jerry rule the world</p></div>";
        let text = to_readable(html);
        assert!(text.contains("Tom & Jerry"), "entities not decoded:\n{text}");
        assert!(!text.contains("steal()"), "script body leaked:\n{text}");
        assert!(!text.contains('<'), "tags leaked:\n{text}");
    }

    // ---- helper units ----------------------------------------------------

    #[test]
    fn strip_block_removes_script_body_case_insensitive() {
        let out = strip_block("script", "a<SCRIPT>x=1</SCRIPT>b");
        assert_eq!(out, "ab");
    }

    #[test]
    fn strip_block_handles_unterminated_block() {
        // An unterminated script tag drops the remainder rather than panicking.
        let out = strip_block("script", "keep<script>oops never closed");
        assert_eq!(out, "keep");
    }

    #[test]
    fn collapse_whitespace_preserves_paragraphs() {
        let out = collapse_whitespace("  one   two  \n\n  three\n four ");
        assert_eq!(out, "one two\n\nthree four");
    }

    #[test]
    fn decode_basic_entities_handles_common_named_refs() {
        assert_eq!(decode_basic_entities("a &amp; b &lt;c&gt; &quot;d&quot;"), "a & b <c> \"d\"");
    }

    // ---- detect_language -------------------------------------------------

    #[test]
    fn detect_language_english() {
        let text = "The quick brown fox jumps over the lazy dog. This sentence \
                    is clearly written in the English language for testing.";
        assert_eq!(detect_language(text).as_deref(), Some("eng"));
    }

    #[test]
    fn detect_language_french() {
        let text = "Le renard brun et rapide saute par-dessus le chien paresseux. \
                    Cette phrase est clairement écrite en langue française.";
        assert_eq!(detect_language(text).as_deref(), Some("fra"));
    }

    #[test]
    fn detect_language_empty_is_none() {
        assert_eq!(detect_language(""), None);
        assert_eq!(detect_language("   \n  "), None);
    }
}
