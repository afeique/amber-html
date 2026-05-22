//! Provenance: anchor an extracted value back to where it came from in the
//! page. See `Plans.md` (task 4.4).
//!
//! [`anchor_for`] computes the **DOM-node** part of provenance — a CSS-path
//! selector to the smallest element whose text contains a value — plus the
//! source URL. The screenshot-region anchor needs the browser's layout box for
//! the element and is wired with the render path later.

use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

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

/// An extracted scalar field paired with where it was found on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProvenance {
    /// JSON Pointer path to the field within the extracted value (RFC 6901,
    /// e.g. `/price` or `/authors/0`); empty for a top-level scalar.
    pub path: String,
    /// The field's value rendered as the text searched for in the page.
    pub value: String,
    /// DOM-node + URL anchor, present when the value's text was located in the
    /// page. `None` for values not found verbatim (e.g. inferred/derived facts).
    pub anchor: Option<Provenance>,
}

/// Anchor every scalar leaf (string or number) of `value` back to `html`,
/// returning one [`FieldProvenance`] per leaf keyed by its JSON Pointer path and
/// tagged with `url`. Objects and arrays are traversed; booleans and nulls are
/// skipped (they rarely correspond to verbatim page text). This is how an
/// extracted record "carries a verifiable anchor" per `Plans.md` task 4.4.
pub fn anchor_fields(html: &str, value: &Value, url: &str) -> Vec<FieldProvenance> {
    let mut out = Vec::new();
    collect_fields(html, value, url, &mut String::new(), &mut out);
    out
}

fn collect_fields(
    html: &str,
    value: &Value,
    url: &str,
    path: &mut String,
    out: &mut Vec<FieldProvenance>,
) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                let len = path.len();
                path.push('/');
                path.push_str(&escape_pointer(key));
                collect_fields(html, v, url, path, out);
                path.truncate(len);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                let len = path.len();
                path.push('/');
                path.push_str(&i.to_string());
                collect_fields(html, v, url, path, out);
                path.truncate(len);
            }
        }
        Value::String(s) => push_leaf(html, url, path, s.clone(), out),
        Value::Number(n) => push_leaf(html, url, path, n.to_string(), out),
        // Booleans and nulls aren't verbatim page facts; skip them.
        Value::Bool(_) | Value::Null => {}
    }
}

fn push_leaf(html: &str, url: &str, path: &str, needle: String, out: &mut Vec<FieldProvenance>) {
    let anchor = anchor_for(html, &needle, url);
    out.push(FieldProvenance {
        path: path.to_string(),
        value: needle,
        anchor,
    });
}

/// Escape `~` and `/` in an object key per RFC 6901 (JSON Pointer).
fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
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

    fn field<'a>(fields: &'a [FieldProvenance], path: &str) -> &'a FieldProvenance {
        fields
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("no field at {path}"))
    }

    #[test]
    fn anchor_fields_anchors_each_scalar_leaf() {
        use serde_json::json;
        let value = json!({
            "amount": "42 USD",
            "lead": "The price is",
            "tags": ["today"],
            "missing": "nonexistent",
            "ok": true,
            "nothing": null,
        });
        let fields = anchor_fields(DOC, &value, "https://ex.com/");

        // A string leaf anchors to the smallest containing element.
        let amount = field(&fields, "/amount").anchor.as_ref().unwrap();
        assert_eq!(amount.css_path, "div#main > article > p.lead > span.amount");
        assert_eq!(amount.url, "https://ex.com/");

        // A value present only in the paragraph anchors to the paragraph.
        let lead = field(&fields, "/lead").anchor.as_ref().unwrap();
        assert_eq!(lead.css_path, "div#main > article > p.lead");

        // Array elements get an indexed JSON-Pointer path.
        let tag = field(&fields, "/tags/0").anchor.as_ref().unwrap();
        assert_eq!(tag.css_path, "div#main > article > p.lead");

        // A value absent from the page is carried but with no anchor.
        assert!(field(&fields, "/missing").anchor.is_none());

        // Booleans and nulls are skipped entirely.
        assert!(!fields
            .iter()
            .any(|f| f.path == "/ok" || f.path == "/nothing"));
    }

    #[test]
    fn anchor_fields_escapes_pointer_keys() {
        use serde_json::json;
        let value = json!({ "a/b": "Title Here" });
        let html = "<html><body><h2>Title Here</h2></body></html>";
        let fields = anchor_fields(html, &value, "u");
        // `/` in a key is escaped as `~1` per RFC 6901.
        assert_eq!(fields[0].path, "/a~1b");
        assert!(fields[0].anchor.is_some());
    }
}
