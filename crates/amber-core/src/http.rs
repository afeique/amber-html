//! The cheap "HTTP-first" static fetch tier (Plans.md).
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

use encoding_rs::Encoding;
use url::Url;

/// A realistic desktop-Chrome User-Agent. Many sites vary their server-rendered
/// markup (or block) by UA, so the cheap tier presents itself as a normal
/// browser. Kept in sync periodically with a current stable Chrome release.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Default end-to-end timeout for the cheap fetch (~30s).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of redirects to follow before giving up.
const MAX_REDIRECTS: u32 = 10;

/// Total fetch attempts (1 initial try + retries) for transient failures.
const MAX_ATTEMPTS: u32 = 3;

/// Base backoff before the first retry; doubles each subsequent attempt.
const BACKOFF_BASE: Duration = Duration::from_millis(200);

/// The result of a successful static HTTP fetch.
#[derive(Debug, Clone)]
pub struct FetchedPage {
    /// The final URL after following any redirects.
    pub final_url: Url,
    /// The HTTP status code of the final response.
    pub status: u16,
    /// The `Content-Type` response header, if present (verbatim, incl. params).
    pub content_type: Option<String>,
    /// The response body decoded as text using the declared charset
    /// (header → BOM → `<meta charset>` → UTF-8). See [`decode_html`].
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

/// Perform a blocking HTTP GET for `url` (the cheap static tier, Plans.md),
/// retrying transient failures with exponential backoff.
///
/// Retries (up to `MAX_ATTEMPTS` total attempts) on transient errors — request
/// timeouts, transport failures, and `429`/`5xx` responses — backing off
/// `BACKOFF_BASE`, doubling each attempt. Permanent failures (e.g. `404`) and
/// successes return immediately. See `fetch_once` for the single-attempt
/// semantics (redirects, User-Agent, charset decoding, timeout).
pub fn fetch(url: &Url) -> Result<FetchedPage, FetchError> {
    fetch_with_ua(url, USER_AGENT)
}

/// Like [`fetch`] but with a caller-supplied `User-Agent` — e.g. an identifiable
/// crawler UA for polite multi-page crawling. Same retry / redirect / charset
/// semantics as [`fetch`].
pub fn fetch_with_ua(url: &Url, user_agent: &str) -> Result<FetchedPage, FetchError> {
    fetch_with_retry(url, MAX_ATTEMPTS, backoff_delay, |u| {
        fetch_once(u, user_agent)
    })
}

/// Retry driver around a single-attempt fetcher. Generic over the fetcher and
/// the backoff schedule so the policy is unit-testable without real I/O or
/// real sleeps.
fn fetch_with_retry<F, B>(
    url: &Url,
    max_attempts: u32,
    backoff: B,
    mut do_fetch: F,
) -> Result<FetchedPage, FetchError>
where
    F: FnMut(&Url) -> Result<FetchedPage, FetchError>,
    B: Fn(u32) -> Duration,
{
    let mut attempt = 1;
    loop {
        match do_fetch(url) {
            Ok(page) => return Ok(page),
            Err(err) => {
                if attempt >= max_attempts || !is_transient(&err) {
                    return Err(err);
                }
                let delay = backoff(attempt);
                tracing::warn!(
                    attempt,
                    max_attempts,
                    backoff_ms = delay.as_millis() as u64,
                    error = %err,
                    "transient fetch failure; retrying"
                );
                std::thread::sleep(delay);
                attempt += 1;
            }
        }
    }
}

/// Whether a [`FetchError`] is worth retrying. Timeouts and transport failures
/// are usually transient; among status codes only `429` (rate limit) and `5xx`
/// (server) are. A `4xx` (other than `429`) is a permanent client error.
fn is_transient(err: &FetchError) -> bool {
    match err {
        FetchError::Timeout => true,
        FetchError::Request(_) => true,
        FetchError::Status(code) => *code == 429 || (500..=599).contains(code),
    }
}

/// Exponential backoff for the `attempt`-th failure (1-based):
/// `BACKOFF_BASE` × 2^(attempt-1) — 200 ms, 400 ms, 800 ms, …
fn backoff_delay(attempt: u32) -> Duration {
    BACKOFF_BASE * 2u32.pow(attempt.saturating_sub(1))
}

/// Perform a single blocking HTTP GET for `url` with the given `user_agent`.
///
/// Follows redirects (up to [`MAX_REDIRECTS`]), records the final URL after
/// redirects, decodes the body using the declared charset, and captures the
/// status code and `Content-Type`. Uses a ~30s end-to-end timeout
/// ([`DEFAULT_TIMEOUT`]).
///
/// A non-2xx status is reported as [`FetchError::Status`] rather than a
/// successful [`FetchedPage`] — the caller decides whether to escalate.
#[tracing::instrument(level = "debug", name = "http.fetch", skip_all, fields(url = %url))]
fn fetch_once(url: &Url, user_agent: &str) -> Result<FetchedPage, FetchError> {
    // `http_status_as_error(true)` is ureq's default; we keep it so 4xx/5xx
    // surface as `Error::StatusCode(code)`, which we translate below.
    let config = ureq::Agent::config_builder()
        .user_agent(user_agent)
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

    // Read the raw bytes (raising the default 10MB cap for large
    // server-rendered pages), then decode to text using the declared charset
    // rather than assuming UTF-8 — many real pages are ISO-8859-1, Windows-1252,
    // Shift_JIS, etc.
    let (_essence, header_charset) = content_type
        .as_deref()
        .map(parse_content_type)
        .unwrap_or_default();

    let mut response = response;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()
        .map_err(map_ureq_error)?;
    let html = decode_html(&bytes, header_charset.as_deref());

    tracing::debug!(
        status,
        final_url = %final_url,
        bytes = bytes.len(),
        charset = header_charset.as_deref().unwrap_or("(sniffed/utf-8)"),
        "static fetch ok"
    );

    Ok(FetchedPage {
        final_url,
        status,
        content_type,
        html,
    })
}

/// Decode raw response bytes into a `String`, choosing the encoding by, in
/// priority order: the HTTP `Content-Type` charset, a leading byte-order mark,
/// a `<meta charset>` declaration sniffed from the document head, then UTF-8.
///
/// Best-effort and infallible: an unrecognized charset label is ignored (we try
/// the next signal), and undecodable bytes become the replacement character.
pub fn decode_html(bytes: &[u8], header_charset: Option<&str>) -> String {
    // 1. The HTTP header charset is authoritative when recognized.
    if let Some(enc) = header_charset.and_then(|c| Encoding::for_label(c.as_bytes())) {
        return enc.decode(bytes).0.into_owned();
    }
    // 2. A byte-order mark, if present, is decisive.
    if let Some((enc, _bom_len)) = Encoding::for_bom(bytes) {
        return enc.decode(bytes).0.into_owned();
    }
    // 3. A `<meta charset>` declaration in the document head.
    if let Some(enc) = sniff_meta_charset(bytes).and_then(|c| Encoding::for_label(c.as_bytes())) {
        return enc.decode(bytes).0.into_owned();
    }
    // 4. Default to UTF-8 (lossy).
    encoding_rs::UTF_8.decode(bytes).0.into_owned()
}

/// Scan the first portion of `bytes` for a `charset=…` declaration, matching
/// both `<meta charset="…">` and `<meta http-equiv content="…; charset=…">`.
/// Returns the raw charset label (lowercased) if found.
fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    // Per the HTML spec, the encoding declaration must appear early; 2 KiB is a
    // generous prescan window.
    let window = &bytes[..bytes.len().min(2048)];
    let head = String::from_utf8_lossy(window).to_ascii_lowercase();
    let idx = head.find("charset")?;
    let after = head[idx + "charset".len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let label: String = after
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| !matches!(c, '"' | '\'' | ' ' | ';' | '>' | '/' | '\t' | '\r' | '\n'))
        .collect();
    (!label.is_empty()).then_some(label)
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
    fn decode_html_uses_header_charset() {
        // "café" in ISO-8859-1: é is 0xE9.
        let bytes = [b'c', b'a', b'f', b'\xE9'];
        assert_eq!(decode_html(&bytes, Some("iso-8859-1")), "café");
    }

    #[test]
    fn decode_html_windows_1252_smart_quotes() {
        // 0x93/0x94 are “ ” curly quotes in Windows-1252 (undefined in latin1).
        let bytes = [b'\x93', b'h', b'i', b'\x94'];
        assert_eq!(decode_html(&bytes, Some("windows-1252")), "“hi”");
    }

    #[test]
    fn decode_html_defaults_to_utf8() {
        // "café" in UTF-8: é is 0xC3 0xA9.
        let bytes = [b'c', b'a', b'f', b'\xC3', b'\xA9'];
        assert_eq!(decode_html(&bytes, None), "café");
    }

    #[test]
    fn decode_html_sniffs_meta_charset_when_header_absent() {
        // No header charset; the document declares ISO-8859-1 via <meta>.
        let mut bytes = b"<html><head><meta charset=\"iso-8859-1\"></head><body>caf".to_vec();
        bytes.push(0xE9); // é in latin1
        bytes.extend_from_slice(b"</body></html>");
        let decoded = decode_html(&bytes, None);
        assert!(decoded.contains("café"), "expected decoded é, got: {decoded}");
    }

    #[test]
    fn decode_html_unknown_charset_falls_back_to_utf8() {
        let bytes = [b'c', b'a', b'f', b'\xC3', b'\xA9'];
        // A bogus label is ignored; UTF-8 decoding still yields café.
        assert_eq!(decode_html(&bytes, Some("x-not-a-charset")), "café");
    }

    #[test]
    fn decode_html_honors_utf8_bom() {
        // UTF-8 BOM (EF BB BF) + "hi"; the BOM is stripped from the output.
        let bytes = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        assert_eq!(decode_html(&bytes, None), "hi");
    }

    // --- retry policy -------------------------------------------------------

    fn ok_page(url: &Url) -> FetchedPage {
        FetchedPage {
            final_url: url.clone(),
            status: 200,
            content_type: Some("text/html".into()),
            html: "<html></html>".into(),
        }
    }

    #[test]
    fn is_transient_classifies_errors() {
        assert!(is_transient(&FetchError::Timeout));
        assert!(is_transient(&FetchError::Request("connection reset".into())));
        assert!(is_transient(&FetchError::Status(429)));
        assert!(is_transient(&FetchError::Status(500)));
        assert!(is_transient(&FetchError::Status(503)));
        assert!(!is_transient(&FetchError::Status(404)));
        assert!(!is_transient(&FetchError::Status(400)));
    }

    #[test]
    fn backoff_doubles_each_attempt() {
        assert_eq!(backoff_delay(1), Duration::from_millis(200));
        assert_eq!(backoff_delay(2), Duration::from_millis(400));
        assert_eq!(backoff_delay(3), Duration::from_millis(800));
    }

    #[test]
    fn retry_returns_first_success_without_retrying() {
        let url = Url::parse("https://ex.com/").unwrap();
        let mut calls = 0;
        let page = fetch_with_retry(&url, 3, |_| Duration::ZERO, |u| {
            calls += 1;
            Ok(ok_page(u))
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(page.status, 200);
    }

    #[test]
    fn retry_recovers_after_transient_failures() {
        let url = Url::parse("https://ex.com/").unwrap();
        let mut calls = 0;
        let page = fetch_with_retry(&url, 3, |_| Duration::ZERO, |u| {
            calls += 1;
            if calls < 3 {
                Err(FetchError::Status(503))
            } else {
                Ok(ok_page(u))
            }
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(page.status, 200);
    }

    #[test]
    fn retry_gives_up_after_max_attempts() {
        let url = Url::parse("https://ex.com/").unwrap();
        let mut calls = 0;
        let err = fetch_with_retry(&url, 3, |_| Duration::ZERO, |_| {
            calls += 1;
            Err(FetchError::Timeout)
        })
        .unwrap_err();
        assert_eq!(calls, 3);
        assert!(matches!(err, FetchError::Timeout));
    }

    #[test]
    fn retry_does_not_retry_permanent_errors() {
        let url = Url::parse("https://ex.com/").unwrap();
        let mut calls = 0;
        let err = fetch_with_retry(&url, 3, |_| Duration::ZERO, |_| {
            calls += 1;
            Err(FetchError::Status(404))
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(matches!(err, FetchError::Status(404)));
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
