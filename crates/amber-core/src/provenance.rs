//! Provenance: anchor an extracted value back to where it came from in the
//! page. See `Plans.md` (task 4.4).
//!
//! [`anchor_for`] computes the **DOM-node** part of provenance — a CSS-path
//! selector to the smallest element whose text contains a value — plus the
//! source URL. The screenshot-region anchor needs the browser's layout box for
//! the element and is wired with the render path later.

use scraper::{ElementRef, Html, Selector};

/// Where an extracted value came from: a DOM anchor + source URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// CSS-path selector to the element containing the value
    /// (e.g. `div#main > article > p.lead`).
    pub css_path: String,
    /// The anchoring element's trimmed text.
    pub text: String,
    /// The capture's source URL.
    pub url: String,
}

/// Anchor `needle` to the smallest element in `html` whose text contains it,
/// tagged with `url`. Returns `None` if `needle` is empty or not found.
pub fn anchor_for(html: &str, needle: &str, url: &str) -> Option<Provenance> {
    if needle.is_empty() {
        return None;
    }
    let doc = Html::parse_document(html);
    let all = Selector::parse("*").ok()?;

    // The smallest containing element is the deepest one whose text matches.
    let mut best: Option<(usize, ElementRef)> = None;
    for el in doc.select(&all) {
        if el.text().collect::<String>().contains(needle) {
            let depth = el.ancestors().count();
            if best.as_ref().is_none_or(|(d, _)| depth > *d) {
                best = Some((depth, el));
            }
        }
    }
    let (_, element) = best?;

    Some(Provenance {
        css_path: css_path(element),
        text: element.text().collect::<String>().trim().to_string(),
        url: url.to_string(),
    })
}

/// Build a CSS path to `element`: each segment is `tag`, plus `#id` (which stops
/// the climb, being specific) or `.class…`. Climbing stops at the first id or at
/// `<body>`, so paths read top-down like `div#main > article > p.lead`.
fn css_path(element: ElementRef) -> String {
    let mut parts = Vec::new();
    let chain = std::iter::once(element).chain(element.ancestors().filter_map(ElementRef::wrap));
    for el in chain {
        let (segment, stop) = segment(el);
        parts.push(segment);
        if stop {
            break;
        }
    }
    parts.reverse();
    parts.join(" > ")
}

/// A single path segment and whether it ends the climb (has an id, or is body).
fn segment(el: ElementRef) -> (String, bool) {
    let v = el.value();
    let name = v.name();
    if let Some(id) = v.id() {
        return (format!("{name}#{id}"), true);
    }
    let mut seg = name.to_string();
    for class in v.classes() {
        seg.push('.');
        seg.push_str(class);
    }
    (seg, name == "body")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<html><body>
        <div id="main">
            <article>
                <p class="lead">The price is <span class="amount">42 USD</span> today.</p>
            </article>
        </div>
    </body></html>"#;

    #[test]
    fn anchors_to_smallest_containing_element() {
        let p = anchor_for(DOC, "42 USD", "https://ex.com/").unwrap();
        assert_eq!(p.css_path, "div#main > article > p.lead > span.amount");
        assert_eq!(p.text, "42 USD");
        assert_eq!(p.url, "https://ex.com/");
    }

    #[test]
    fn anchors_to_paragraph_when_value_spans_only_it() {
        // "The price is" is only in the <p>, not the inner <span>.
        let p = anchor_for(DOC, "The price is", "https://ex.com/").unwrap();
        assert_eq!(p.css_path, "div#main > article > p.lead");
        assert!(p.text.starts_with("The price is"));
    }

    #[test]
    fn returns_none_when_not_found_or_empty() {
        assert!(anchor_for(DOC, "nonexistent value", "u").is_none());
        assert!(anchor_for(DOC, "", "u").is_none());
    }

    #[test]
    fn path_stops_at_body_without_an_id() {
        let html = "<html><body><section><h2>Title Here</h2></section></body></html>";
        let p = anchor_for(html, "Title Here", "u").unwrap();
        assert_eq!(p.css_path, "body > section > h2");
    }
}
