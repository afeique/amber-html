//! Secret hygiene for logs and traces. See `Plans.md` (task 3.10).
//!
//! Auth material reaches the engine in two shapes: the session headers/cookies
//! (redacted by [`crate::session::SessionState`]'s own `Debug`) and credentials
//! a caller may embed in a proxy URL (`http://user:pass@host`). [`redact_proxy_url`]
//! strips the latter so a proxy can be logged for observability without leaking
//! its password.

/// Redact userinfo (credentials) from a proxy URL for safe logging:
/// `http://user:pass@host:8080` → `http://***@host:8080`. A URL with no
/// credentials is returned unchanged; an unparseable string that contains an
/// `@` is replaced wholesale with `***` (never echo something we can't
/// sanitize), while a scheme-less, credential-free value passes through.
pub fn redact_proxy_url(proxy: &str) -> String {
    match url::Url::parse(proxy) {
        Ok(u) if !u.username().is_empty() || u.password().is_some() => {
            let mut out = format!("{}://***@", u.scheme());
            if let Some(host) = u.host_str() {
                out.push_str(host);
            }
            if let Some(port) = u.port() {
                out.push(':');
                out.push_str(&port.to_string());
            }
            // Proxy URLs rarely carry a path; include it only when non-trivial.
            if u.path() != "/" {
                out.push_str(u.path());
            }
            out
        }
        Ok(_) => proxy.to_string(),
        Err(_) if proxy.contains('@') => "***".to_string(),
        Err(_) => proxy.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_userinfo_keeping_scheme_host_port() {
        assert_eq!(
            redact_proxy_url("http://user:s3cret@proxy.local:8080"),
            "http://***@proxy.local:8080"
        );
        assert_eq!(
            redact_proxy_url("socks5://bob:pw@10.0.0.1:1080"),
            "socks5://***@10.0.0.1:1080"
        );
    }

    #[test]
    fn passes_through_credential_free_urls() {
        assert_eq!(
            redact_proxy_url("http://proxy.local:8080"),
            "http://proxy.local:8080"
        );
        // Scheme-less but credential-free (e.g. a bare host:port) is unchanged.
        assert_eq!(redact_proxy_url("proxy.local:8080"), "proxy.local:8080");
    }

    #[test]
    fn unparseable_with_at_sign_is_fully_redacted() {
        let r = redact_proxy_url("@@not a url@@ user:pass@");
        assert_eq!(r, "***");
        assert!(!r.contains("pass"));
    }

    #[test]
    fn never_leaks_the_password() {
        let r = redact_proxy_url("https://admin:hunter2@gw:3128");
        assert!(!r.contains("hunter2"));
        assert!(r.contains("gw:3128"));
    }
}
