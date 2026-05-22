//! Agent-native action primitives. See `Plans.md` (task 2.5).
//!
//! [`Action`] models the interactions an agent can drive on a page; [`to_cdp`]
//! builds the CDP `(method, params)` call for each — `Page.navigate` for
//! navigation, `Runtime.evaluate` of a small JS snippet for click/fill/scroll.
//! Selectors and values are JSON-encoded into the JS so they can't break out of
//! (or inject into) the expression. This is the pure construction layer; the
//! render/agent path issues these over the CDP pipe.

use serde_json::{json, Value};

/// An interaction to perform on the current page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Navigate to a URL.
    Navigate(String),
    /// Click the first element matching a CSS selector.
    Click(String),
    /// Set an input/textarea's value and fire `input`/`change` events.
    Fill { selector: String, value: String },
    /// Scroll the window by a relative offset.
    ScrollBy { x: i64, y: i64 },
    /// Scroll to the bottom of the document (e.g. to trigger lazy-load).
    ScrollToBottom,
}

/// Build the CDP `(method, params)` call that performs `action`.
///
/// The JS-backed actions return a boolean (whether the target element was
/// found) via `returnByValue`, so callers can detect a missing selector.
pub fn to_cdp(action: &Action) -> (&'static str, Value) {
    match action {
        Action::Navigate(url) => ("Page.navigate", json!({ "url": url })),
        Action::Click(selector) => {
            let sel = js_str(selector);
            evaluate(format!(
                "(()=>{{const e=document.querySelector({sel});if(e)e.click();return !!e;}})()"
            ))
        }
        Action::Fill { selector, value } => {
            let sel = js_str(selector);
            let val = js_str(value);
            evaluate(format!(
                "(()=>{{const e=document.querySelector({sel});if(e){{e.value={val};\
                 e.dispatchEvent(new Event('input',{{bubbles:true}}));\
                 e.dispatchEvent(new Event('change',{{bubbles:true}}));}}return !!e;}})()"
            ))
        }
        Action::ScrollBy { x, y } => evaluate(format!("window.scrollBy({x},{y})")),
        Action::ScrollToBottom => {
            evaluate("window.scrollTo(0,document.body.scrollHeight)".to_string())
        }
    }
}

/// A `Runtime.evaluate` command for `expression`, returning its value.
fn evaluate(expression: String) -> (&'static str, Value) {
    (
        "Runtime.evaluate",
        json!({ "expression": expression, "returnByValue": true }),
    )
}

/// JSON-encode `s` as a JS string literal (quotes + escaping included).
fn js_str(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expr(action: &Action) -> String {
        let (method, params) = to_cdp(action);
        assert_eq!(method, "Runtime.evaluate");
        params["expression"].as_str().unwrap().to_string()
    }

    #[test]
    fn navigate_uses_page_navigate() {
        let (method, params) = to_cdp(&Action::Navigate("https://example.com/".into()));
        assert_eq!(method, "Page.navigate");
        assert_eq!(params["url"], "https://example.com/");
    }

    #[test]
    fn click_evaluates_query_selector_click() {
        let e = expr(&Action::Click(".submit".into()));
        assert!(e.contains(r#"document.querySelector(".submit")"#));
        assert!(e.contains(".click()"));
    }

    #[test]
    fn fill_sets_value_and_dispatches_events() {
        let e = expr(&Action::Fill {
            selector: "#email".into(),
            value: "a@b.com".into(),
        });
        assert!(e.contains(r##"document.querySelector("#email")"##));
        assert!(e.contains(r#"e.value="a@b.com""#));
        assert!(e.contains("new Event('input'"));
        assert!(e.contains("new Event('change'"));
    }

    #[test]
    fn fill_escapes_values_to_prevent_injection() {
        // A value containing a quote + JS must be encoded, not break out of the
        // string literal it's embedded in.
        let injection = "a\"); alert(1); (";
        let e = expr(&Action::Fill {
            selector: "#x".into(),
            value: injection.into(),
        });
        // The raw injection (with its unescaped quote) must not appear verbatim.
        assert!(!e.contains(injection));
        // The quote is JSON-escaped (\") inside the embedded string literal.
        assert!(e.contains("\\\""));
    }

    #[test]
    fn scroll_actions() {
        assert_eq!(
            expr(&Action::ScrollBy { x: 0, y: 500 }),
            "window.scrollBy(0,500)"
        );
        assert_eq!(
            expr(&Action::ScrollToBottom),
            "window.scrollTo(0,document.body.scrollHeight)"
        );
    }
}
