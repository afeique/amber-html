//! The cheap "HTTP-first" static fetch tier (PLAN.md §7, step 2).
//!
//! A plain blocking HTTP GET that follows redirects, records the *final* URL,
//! and reads the body as text. This is the fast path of the tiered-fetch
//! pipeline: when the static HTML already contains the content (server-rendered
//! or content embedded as JSON), no browser is needed.
//!
//! The HTTP client is [`ureq`](https://docs.rs/ureq) (3.x): a lean, blocking,
//! `unsafe`-free client with `rustls` TLS by default. It pulls far fewer
//! dependencies than `reqwest` (no tokio / hyper), which matters for a crate
//! whose public API is intentionally blocking.
//!
//! [`FetchError`] is deliberately **local** to this module; the orchestrator
//! (`capture::run`) maps it into the crate-wide `error::Error`.

use std::time::Duration;

use url::Url;

/// A realistic desktop-Chrome User-Agent. Many sites vary their server-rendered
/// markup (or block) by UA, so the cheap tier presents itself as a normal
/// browser. Kept in sync periodically with a current stable Chrome release.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Default end-to-end timeout for the cheap fetch (PLAN.md: ~30s).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of redirects to follow before giving up.
const MAX_REDIRECTS: u32 = 10;

/// The result of a successful static HTTP fetch.
#[derive(Debug, Clone)]
pub struct FetchedPage {
    /// The final URL after following any redirects.
    pub final_url: Url,
    /// The HTTP status code of the final response.
    pub status: u16,
    /// The `Content-Type` response header, if present (verbatim, incl. params).
    pub content_type: Option<String>,
    /// The response body decoded as text (UTF-8, best-effort).
    pub html: String,
}

/// Errors from the cheap HTTP-first fetch tier. Local to this module.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// Transport / request failure (DNS, TCP, TLS, malformed request, etc.).
    #[error("HTTP request failed: {0}")]
    Request(String),

    /// The server responded, but with a non-success (non-2xx) status code.
    #[error("HTTP request returned non-success status {0}")]
    Status(u16),

    /// The request exceeded the configured timeout.
    #[error("HTTP request timed out")]
    Timeout,
}

/// Perform a blocking HTTP GET for `url` (the cheap static tier, PLAN.md §7).
///
/// Follows redirects (up to [`MAX_REDIRECTS`]), sends a realistic desktop-Chrome
/// `User-Agent`, records the final URL after redirects, reads the body as text
/// (UTF-8 best-effort), and captures the status code and `Content-Type`. Uses a
/// ~30s end-to-end timeout ([`DEFAULT_TIMEOUT`]).
///
/// A non-2xx status is reported as [`FetchError::Status`] rather than a
/// successful [`FetchedPage`] — the caller decides whether to escalate.
pub fn fetch(url: &Url) -> Result<FetchedPage, FetchError> {
    // `http_status_as_error(true)` is ureq's default; we keep it so 4xx/5xx
    // surface as `Error::StatusCode(code)`, which we translate below.
    let config = ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_global(Some(DEFAULT_TIMEOUT))
        .max_redirects(MAX_REDIRECTS)
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let response = agent.get(url.as_str()).call().map_err(map_ureq_error)?;

    // Final URL after redirects, via the `ResponseExt` extension trait.
    let final_url = {
        use ureq::ResponseExt;
        let uri = response.get_uri();
        // `Uri` -> `Url`: parse its string form. A successful HTTP response
        // always carries an absolute URI, so this should not fail; if it
        // somehow does, fall back to the request URL.
        Url::parse(&uri.to_string()).unwrap_or_else(|_| url.clone())
    };

    let status = response.status().as_u16();

    let content_type = response
        .headers()
        .get(ureq::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // Read the body as text. ureq replaces invalid UTF-8 with the replacement
    // character (best-effort decode). Raise the default 10MB cap generously
    // for large server-rendered pages.
    let mut response = response;
    let html = response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_string()
        .map_err(map_ureq_error)?;

    Ok(FetchedPage {
        final_url,
        status,
        content_type,
        html,
    })
}

/// Translate a [`ureq::Error`] into our local [`FetchError`].
fn map_ureq_error(err: ureq::Error) -> FetchError {
    match err {
        ureq::Error::StatusCode(code) => FetchError::Status(code),
        ureq::Error::Timeout(_) => FetchError::Timeout,
        other => FetchError::Request(other.to_string()),
    }
}

/// Parse a `Content-Type` header value into its MIME essence (lowercased,
/// trimmed, parameters stripped) and optional `charset` parameter (lowercased).
///
/// Examples:
/// - `"text/html; charset=UTF-8"` -> `("text/html", Some("utf-8"))`
/// - `"application/xhtml+xml"`     -> `("application/xhtml+xml", None)`
pub fn parse_content_type(value: &str) -> (String, Option<String>) {
    let mut parts = value.split(';');
    let essence = parts
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    let mut charset = None;
    for param in parts {
        let param = param.trim();
        if let Some((key, val)) = param.split_once('=') {
            if key.trim().eq_ignore_ascii_case("charset") {
                // Strip optional surrounding quotes.
                let val = val.trim().trim_matches('"');
                if !val.is_empty() {
                    charset = Some(val.to_ascii_lowercase());
                }
            }
        }
    }

    (essence, charset)
}

/// Whether a `Content-Type` essence string denotes HTML (or XHTML) content.
///
/// Accepts `text/html` and `application/xhtml+xml`. Used by the sufficiency /
/// escalation logic to decide whether the cheap fetch returned something the
/// static path can even reason about.
pub fn is_html_mime(essence: &str) -> bool {
    let essence = essence.trim().to_ascii_lowercase();
    essence == "text/html" || essence == "application/xhtml+xml"
}

/// Convenience: does this `Content-Type` *header value* (with optional params)
/// denote HTML? `None`/absent header is treated as not-HTML.
pub fn content_type_is_html(content_type: Option<&str>) -> bool {
    match content_type {
        Some(ct) => {
            let (essence, _charset) = parse_content_type(ct);
            is_html_mime(&essence)
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_content_type_html_with_charset() {
        let (essence, charset) = parse_content_type("text/html; charset=UTF-8");
        assert_eq!(essence, "text/html");
        assert_eq!(charset.as_deref(), Some("utf-8"));
    }

    #[test]
    fn parse_content_type_no_params() {
        let (essence, charset) = parse_content_type("application/xhtml+xml");
        assert_eq!(essence, "application/xhtml+xml");
        assert_eq!(charset, None);
    }

    #[test]
    fn parse_content_type_trims_and_lowercases_essence() {
        let (essence, _) = parse_content_type("  TEXT/HTML  ");
        assert_eq!(essence, "text/html");
    }

    #[test]
    fn parse_content_type_charset_quoted_and_spaced() {
        let (essence, charset) = parse_content_type("text/html ; charset = \"ISO-8859-1\"");
        assert_eq!(essence, "text/html");
        assert_eq!(charset.as_deref(), Some("iso-8859-1"));
    }

    #[test]
    fn parse_content_type_multiple_params_picks_charset() {
        let (essence, charset) =
            parse_content_type("text/html; boundary=xyz; charset=us-ascii");
        assert_eq!(essence, "text/html");
        assert_eq!(charset.as_deref(), Some("us-ascii"));
    }

    #[test]
    fn parse_content_type_empty_charset_is_none() {
        let (_essence, charset) = parse_content_type("text/html; charset=");
        assert_eq!(charset, None);
    }

    #[test]
    fn is_html_mime_accepts_html_and_xhtml() {
        assert!(is_html_mime("text/html"));
        assert!(is_html_mime("application/xhtml+xml"));
        assert!(is_html_mime("  TEXT/HTML "));
    }

    #[test]
    fn is_html_mime_rejects_non_html() {
        assert!(!is_html_mime("application/json"));
        assert!(!is_html_mime("text/plain"));
        assert!(!is_html_mime("image/png"));
        assert!(!is_html_mime(""));
    }

    #[test]
    fn content_type_is_html_handles_full_header() {
        assert!(content_type_is_html(Some("text/html; charset=utf-8")));
        assert!(content_type_is_html(Some("application/xhtml+xml")));
        assert!(!content_type_is_html(Some("application/json; charset=utf-8")));
        assert!(!content_type_is_html(None));
    }

    #[test]
    fn fetch_error_display_includes_status_code() {
        let e = FetchError::Status(404);
        assert!(e.to_string().contains("404"));
        assert!(FetchError::Timeout.to_string().contains("timed out"));
        assert!(FetchError::Request("boom".into())
            .to_string()
            .contains("boom"));
    }

    // --- Real-network test (opt-in) -----------------------------------------

    #[test]
    #[ignore = "performs a real network request; run with --ignored"]
    fn fetch_example_com_live() {
        let url = Url::parse("https://example.com/").unwrap();
        let page = fetch(&url).expect("fetch example.com");
        assert_eq!(page.status, 200);
        assert!(content_type_is_html(page.content_type.as_deref()));
        assert!(page.html.to_lowercase().contains("<html"));
        assert_eq!(page.final_url.host_str(), Some("example.com"));
    }
}
