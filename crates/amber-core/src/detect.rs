//! Tiered-fetch sufficiency analysis: decide whether a static HTML document
//! already contains usable content, or a real browser is required.
//! See `Plans.md`.
//!
//! Structural signals (`<noscript>` JS demands, meta-refresh redirects) are
//! parsed with html5ever via [`scraper`]; the visible-text estimate uses a fast
//! byte scan that drops `<script>`/`<style>` blocks.

use scraper::{Html, Selector};

/// Verdict from analyzing a static HTML document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sufficiency {
    /// Static HTML clearly has usable content — no browser needed.
    Static,
    /// Static HTML is clearly an empty JS shell / insufficient — render it.
    NeedsBrowser,
    /// Ambiguous. Callers should escalate to a browser (correctness bias).
    Uncertain,
}

/// Default minimum visible-text length to treat static HTML as sufficient.
pub const CONTENT_FLOOR: usize = 500;

/// Known empty-app-shell root markers (only meaningful combined with low text).
const SHELL_MARKERS: [&str; 5] = [
    "id=\"root\"",
    "id=\"app\"",
    "id=\"__next\"",
    "<app-root",
    "id=\"__nuxt\"",
];

/// Markers that the page embeds its content as structured data, so an empty
/// shell may actually be statically complete (a future fast-path could harvest
/// it). Their presence downgrades a hard "needs browser" to "uncertain".
const EMBEDDED_DATA_MARKERS: [&str; 5] = [
    "__next_data__",
    "__nuxt__",
    "application/ld+json",
    "window.__initial_state__",
    "__apollo_state__",
];

/// Phrases (lowercased) inside a `<noscript>` block that signal the page is
/// non-functional without JavaScript.
const JS_REQUIRED_PHRASES: [&str; 6] = [
    "enable javascript",
    "enable js",
    "requires javascript",
    "javascript is required",
    "javascript is disabled",
    "turn on javascript",
];

/// Parse a selector known at authoring time to be valid (a bad literal is a
/// programmer error and surfaces immediately in tests).
fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector must be valid")
}

/// Assess a static HTML document against `content_floor`.
///
/// Correctness-biased: anything ambiguous returns [`Sufficiency::Uncertain`] so
/// the caller escalates to a browser (a wrong "static is fine" silently loses
/// data; a wrong "needs browser" only costs time).
pub fn assess(html: &str, content_floor: usize) -> Sufficiency {
    let verdict = assess_inner(html, content_floor);
    tracing::debug!(?verdict, content_floor, "sufficiency verdict");
    verdict
}

fn assess_inner(html: &str, content_floor: usize) -> Sufficiency {
    let doc = Html::parse_document(html);

    // Hard signal: a <noscript> block telling the user to enable JavaScript.
    if noscript_demands_js(&doc) {
        return Sufficiency::NeedsBrowser;
    }

    // Hard signal: a meta-refresh redirect to another URL (the real content
    // lives at the target, reachable only by following it in a browser).
    if has_meta_refresh_redirect(&doc) {
        return Sufficiency::NeedsBrowser;
    }

    let lower = html.to_ascii_lowercase();
    let text_len = visible_text_len(&lower);

    // Enough rendered text already present: static is fine.
    if text_len >= content_floor {
        return Sufficiency::Static;
    }

    // Below the content floor. An empty app-shell needs a browser — unless it
    // embeds structured data, in which case the content *may* be static, so we
    // stay uncertain (still escalate today, but not a hard "needs browser").
    let is_shell = SHELL_MARKERS.iter().any(|m| lower.contains(m));
    if is_shell {
        let has_embedded_data = EMBEDDED_DATA_MARKERS.iter().any(|m| lower.contains(m));
        return if has_embedded_data {
            Sufficiency::Uncertain
        } else {
            Sufficiency::NeedsBrowser
        };
    }

    Sufficiency::Uncertain
}

/// True if any `<noscript>` block contains a "please enable JavaScript" notice.
fn noscript_demands_js(doc: &Html) -> bool {
    doc.select(&sel("noscript")).any(|el| {
        let text = el.text().collect::<String>().to_ascii_lowercase();
        JS_REQUIRED_PHRASES.iter().any(|p| text.contains(p))
    })
}

/// True if the document has a `<meta http-equiv="refresh">` redirect (a
/// `content` with a `url=` target), case-insensitively.
fn has_meta_refresh_redirect(doc: &Html) -> bool {
    doc.select(&sel("meta")).any(|el| {
        let is_refresh = el
            .value()
            .attr("http-equiv")
            .is_some_and(|v| v.eq_ignore_ascii_case("refresh"));
        is_refresh
            && el
                .value()
                .attr("content")
                .is_some_and(|c| c.to_ascii_lowercase().contains("url="))
    })
}

/// Rough visible-text length: drop `<script>`/`<style>` blocks and tag markup,
/// then count non-whitespace bytes. Operates on already-lowercased input.
fn visible_text_len(lower: &str) -> usize {
    let b = lower.as_bytes();
    let mut len = 0usize;
    let mut i = 0usize;
    let mut in_tag = false;
    let mut skip_close: Option<&'static [u8]> = None;

    while i < b.len() {
        if let Some(close) = skip_close {
            if at(b, i, close) {
                i += close.len();
                skip_close = None;
            } else {
                i += 1;
            }
            continue;
        }
        if at(b, i, b"<script") {
            skip_close = Some(b"</script>");
            i += 7;
            continue;
        }
        if at(b, i, b"<style") {
            skip_close = Some(b"</style>");
            i += 6;
            continue;
        }
        match b[i] {
            b'<' => in_tag = true,
            b'>' => in_tag = false,
            c if !in_tag && !c.is_ascii_whitespace() => len += 1,
            _ => {}
        }
        i += 1;
    }
    len
}

/// True if `needle` occurs in `hay` starting at byte index `i`.
fn at(hay: &[u8], i: usize, needle: &[u8]) -> bool {
    hay.len() >= i + needle.len() && &hay[i..i + needle.len()] == needle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noscript_enable_js_needs_browser() {
        let html = "<html><body><noscript>Please enable JavaScript to continue.</noscript>\
                    <div id=\"root\"></div></body></html>";
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::NeedsBrowser);
    }

    #[test]
    fn enable_js_phrase_in_body_does_not_force_browser() {
        // A content-rich page that merely mentions enabling JavaScript in prose
        // (not inside <noscript>) is still static — no false escalation.
        let body = format!("To use the live demo, enable JavaScript. {}", "lorem ".repeat(200));
        let html = format!("<html><body><article>{body}</article></body></html>");
        assert_eq!(assess(&html, CONTENT_FLOOR), Sufficiency::Static);
    }

    #[test]
    fn meta_refresh_redirect_needs_browser() {
        let html = r#"<html><head>
            <meta http-equiv="Refresh" content="0; url=https://example.com/app">
            </head><body></body></html>"#;
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::NeedsBrowser);
    }

    #[test]
    fn meta_refresh_without_url_is_not_a_redirect() {
        // A timed self-reload (no url=) is not a redirect signal on its own;
        // with no content it falls through to Uncertain.
        let html = r#"<html><head>
            <meta http-equiv="refresh" content="30">
            </head><body><p>thin</p></body></html>"#;
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::Uncertain);
    }

    #[test]
    fn empty_shell_needs_browser() {
        let html = r#"<html><body><div id="root"></div><script>var x = 1;</script></body></html>"#;
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::NeedsBrowser);
    }

    #[test]
    fn shell_with_embedded_data_is_uncertain_not_hard_fail() {
        // Next.js app shell that embeds its content as JSON: the content *may*
        // be statically present, so we stay uncertain rather than hard-failing.
        let html = r#"<html><body>
            <div id="__next"></div>
            <script id="__NEXT_DATA__" type="application/json">{"props":{"x":1}}</script>
            </body></html>"#;
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::Uncertain);
    }

    #[test]
    fn rich_content_is_static() {
        let body = "lorem ".repeat(200); // ~1000 non-whitespace chars
        let html = format!("<html><body><article>{body}</article></body></html>");
        assert_eq!(assess(&html, CONTENT_FLOOR), Sufficiency::Static);
    }

    #[test]
    fn thin_content_is_uncertain() {
        let html = "<html><body><p>hi</p></body></html>";
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::Uncertain);
    }

    #[test]
    fn scripts_do_not_count_as_content() {
        let js = "var data = 'x';".repeat(100); // lots of script text
        let html = format!("<html><body><div id=\"app\"></div><script>{js}</script></body></html>");
        // Script content is stripped, so this empty shell still needs a browser.
        assert_eq!(assess(&html, CONTENT_FLOOR), Sufficiency::NeedsBrowser);
    }

    #[tracing_test::traced_test]
    #[test]
    fn assess_emits_a_verdict_event() {
        let _ = assess("<html><body><div id=\"root\"></div></body></html>", CONTENT_FLOOR);
        // The single verdict event is emitted with the chosen Sufficiency.
        assert!(logs_contain("sufficiency verdict"));
        assert!(logs_contain("NeedsBrowser"));
    }
}
