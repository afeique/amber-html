//! Tiered-fetch sufficiency analysis: decide whether a static HTML document
//! already contains usable content, or a real browser is required.
//! See `docs/PLAN.md` §7.
//!
//! NOTE: these are scaffold heuristics over raw HTML text; the real
//! implementation will parse with `html5ever` for accuracy.

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

/// Assess a static HTML document against `content_floor`.
///
/// Correctness-biased: anything ambiguous returns [`Sufficiency::Uncertain`] so
/// the caller escalates to a browser (a wrong "static is fine" silently loses
/// data; a wrong "needs browser" only costs time).
pub fn assess(html: &str, content_floor: usize) -> Sufficiency {
    let lower = html.to_ascii_lowercase();

    // Hard signal: the page explicitly demands JavaScript.
    if lower.contains("enable javascript") || lower.contains("requires javascript") {
        return Sufficiency::NeedsBrowser;
    }

    let text_len = visible_text_len(&lower);

    // Empty app-shell: a known framework root with almost no rendered text.
    let is_shell = SHELL_MARKERS.iter().any(|m| lower.contains(m));
    if is_shell && text_len < content_floor {
        return Sufficiency::NeedsBrowser;
    }

    if text_len >= content_floor {
        Sufficiency::Static
    } else {
        Sufficiency::Uncertain
    }
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
    fn enable_js_needs_browser() {
        let html = "<html><body>Please enable JavaScript to continue.</body></html>";
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::NeedsBrowser);
    }

    #[test]
    fn empty_shell_needs_browser() {
        let html = r#"<html><body><div id="root"></div><script>var x = 1;</script></body></html>"#;
        assert_eq!(assess(html, CONTENT_FLOOR), Sufficiency::NeedsBrowser);
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
}
