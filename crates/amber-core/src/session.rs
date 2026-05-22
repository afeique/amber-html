//! Session state for capturing behind-auth pages. See `Plans.md` (task 3.3).
//!
//! A [`SessionState`] carries caller-supplied cookies and extra request headers
//! (e.g. `Authorization: Bearer …`). The same state is applied to both capture
//! tiers: folded into the request headers on the cheap static fetch, and sent
//! via `Network.setExtraHTTPHeaders` on the browser render. Secret *values* are
//! never logged (task 3.10) — only counts/names ever surface in traces.
//!
//! Storage-state (localStorage / sessionStorage) injection is a browser-path
//! follow-up; cookie- and header-based auth (the common case) is covered here.

use serde_json::{json, Map, Value};

/// Caller-supplied session state: cookies + extra HTTP request headers, applied
/// on both the static fetch and the browser navigation.
///
/// `Debug` is implemented by hand to redact secret values — it prints header
/// and cookie *names* only, so an auth secret can never leak through a debug
/// log of this struct (or of [`crate::CaptureOptions`], which contains it).
#[derive(Clone, Default)]
pub struct SessionState {
    /// Extra request headers as `(name, value)` pairs (e.g. an auth bearer).
    pub headers: Vec<(String, String)>,
    /// Cookies as `(name, value)` pairs, sent as a single `Cookie` header.
    pub cookies: Vec<(String, String)>,
}

impl std::fmt::Debug for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names =
            |pairs: &[(String, String)]| pairs.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        f.debug_struct("SessionState")
            .field("headers", &names(&self.headers))
            .field("cookies", &names(&self.cookies))
            .finish()
    }
}

impl SessionState {
    /// Whether no session state is set (the default capture sends nothing extra).
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty() && self.cookies.is_empty()
    }

    /// The `Cookie` header value (`a=1; b=2`), or `None` when no cookies are set.
    pub fn cookie_header(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }
        Some(
            self.cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    /// All request headers to send: the explicit headers, then the folded
    /// `Cookie` header. `(name, value)` pairs; empty when the session is empty.
    pub fn request_headers(&self) -> Vec<(String, String)> {
        let mut out = self.headers.clone();
        if let Some(cookie) = self.cookie_header() {
            out.push(("Cookie".to_string(), cookie));
        }
        out
    }

    /// The `Network.setExtraHTTPHeaders` CDP command applying these headers
    /// (including the folded `Cookie`) on the browser path, or `None` when the
    /// session is empty.
    pub fn extra_http_headers_command(&self) -> Option<(&'static str, Value)> {
        let headers = self.request_headers();
        if headers.is_empty() {
            return None;
        }
        let map: Map<String, Value> = headers
            .into_iter()
            .map(|(name, value)| (name, Value::String(value)))
            .collect();
        Some(("Network.setExtraHTTPHeaders", json!({ "headers": map })))
    }

    /// Header and cookie *names* (never values) — safe to log (task 3.10).
    pub fn loggable_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.headers.iter().map(|(n, _)| n.clone()).collect();
        names.extend(self.cookies.iter().map(|(n, _)| format!("cookie:{n}")));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_by_default() {
        let s = SessionState::default();
        assert!(s.is_empty());
        assert!(s.cookie_header().is_none());
        assert!(s.request_headers().is_empty());
        assert!(s.extra_http_headers_command().is_none());
    }

    #[test]
    fn cookies_fold_into_a_single_header() {
        let s = SessionState {
            cookies: vec![
                ("sid".to_string(), "abc".to_string()),
                ("theme".to_string(), "dark".to_string()),
            ],
            ..Default::default()
        };
        assert_eq!(s.cookie_header().as_deref(), Some("sid=abc; theme=dark"));
    }

    #[test]
    fn request_headers_appends_cookie_after_explicit_headers() {
        let s = SessionState {
            headers: vec![("Authorization".to_string(), "Bearer t0ken".to_string())],
            cookies: vec![("sid".to_string(), "abc".to_string())],
        };
        let headers = s.request_headers();
        assert_eq!(
            headers[0],
            ("Authorization".to_string(), "Bearer t0ken".to_string())
        );
        assert_eq!(headers[1], ("Cookie".to_string(), "sid=abc".to_string()));
    }

    #[test]
    fn extra_http_headers_command_carries_headers_and_cookie() {
        let s = SessionState {
            headers: vec![("Authorization".to_string(), "Bearer t0ken".to_string())],
            cookies: vec![("sid".to_string(), "abc".to_string())],
        };
        let (method, params) = s.extra_http_headers_command().unwrap();
        assert_eq!(method, "Network.setExtraHTTPHeaders");
        assert_eq!(params["headers"]["Authorization"], "Bearer t0ken");
        assert_eq!(params["headers"]["Cookie"], "sid=abc");
    }

    #[test]
    fn debug_redacts_secret_values() {
        let s = SessionState {
            headers: vec![("Authorization".to_string(), "Bearer SECRET".to_string())],
            cookies: vec![("sid".to_string(), "SECRET".to_string())],
        };
        let rendered = format!("{s:?}");
        assert!(rendered.contains("Authorization") && rendered.contains("sid"));
        assert!(
            !rendered.contains("SECRET"),
            "Debug must not print secret values: {rendered}"
        );
    }

    #[test]
    fn loggable_names_never_include_values() {
        let s = SessionState {
            headers: vec![("Authorization".to_string(), "Bearer SECRET".to_string())],
            cookies: vec![("sid".to_string(), "SECRET".to_string())],
        };
        let names = s.loggable_names();
        assert!(names.contains(&"Authorization".to_string()));
        assert!(names.contains(&"cookie:sid".to_string()));
        assert!(!names.iter().any(|n| n.contains("SECRET")));
    }
}
