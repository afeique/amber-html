//! Content-addressed caching and conditional requests for re-capture and
//! incremental crawling. See `Plans.md` (task 3.4).
//!
//! Two complementary mechanisms let a re-capture skip unchanged pages:
//! - **Content addressing** — [`content_hash`] fingerprints a response body, so
//!   a re-fetch whose body hashes to the stored value is known to be unchanged
//!   even when the server sends no validators.
//! - **Conditional requests** — when a prior response carried an `ETag` or
//!   `Last-Modified`, [`Cache::conditional_headers`] produces the matching
//!   `If-None-Match` / `If-Modified-Since` request headers so the server can
//!   answer `304 Not Modified` and save the transfer entirely.
//!
//! The store here is in-memory; a durable on-disk backing is a later step.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

/// Hex-encoded SHA-256 of `bytes`. Stable across runs, so it is suitable as a
/// content-addressed cache key / change fingerprint.
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// What we remember about a previously fetched URL for revalidation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheEntry {
    /// Content fingerprint of the last response body.
    pub content_hash: String,
    /// The `ETag` response header from the last fetch, if any.
    pub etag: Option<String>,
    /// The `Last-Modified` response header from the last fetch, if any.
    pub last_modified: Option<String>,
}

/// An in-memory content-addressed cache keyed by URL.
#[derive(Debug, Default)]
pub struct Cache {
    entries: HashMap<String, CacheEntry>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (or replace) the cache entry for `url` from a fetched response.
    pub fn record(
        &mut self,
        url: &str,
        body: &[u8],
        etag: Option<String>,
        last_modified: Option<String>,
    ) {
        self.entries.insert(
            url.to_string(),
            CacheEntry {
                content_hash: content_hash(body),
                etag,
                last_modified,
            },
        );
    }

    /// The stored entry for `url`, if any.
    pub fn get(&self, url: &str) -> Option<&CacheEntry> {
        self.entries.get(url)
    }

    /// Whether a freshly fetched `body` for `url` is byte-identical to what we
    /// cached (by content hash). `false` if we have no entry for `url`.
    pub fn is_unchanged(&self, url: &str, body: &[u8]) -> bool {
        self.get(url)
            .is_some_and(|e| e.content_hash == content_hash(body))
    }

    /// Conditional-request headers to revalidate `url`: `If-None-Match` from the
    /// stored `ETag` and `If-Modified-Since` from the stored `Last-Modified`.
    /// Empty when there is no entry or no validators.
    pub fn conditional_headers(&self, url: &str) -> Vec<(&'static str, String)> {
        let mut headers = Vec::new();
        if let Some(entry) = self.get(url) {
            if let Some(etag) = &entry.etag {
                headers.push(("If-None-Match", etag.clone()));
            }
            if let Some(lm) = &entry.last_modified {
                headers.push(("If-Modified-Since", lm.clone()));
            }
        }
        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_stable_and_known() {
        // SHA-256("") — a fixed, well-known value.
        assert_eq!(
            content_hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // Deterministic and distinct for distinct input.
        assert_eq!(content_hash(b"hello"), content_hash(b"hello"));
        assert_ne!(content_hash(b"hello"), content_hash(b"world"));
    }

    #[test]
    fn is_unchanged_detects_identical_body() {
        let mut cache = Cache::new();
        cache.record("https://ex.com/a", b"<html>v1</html>", None, None);
        assert!(cache.is_unchanged("https://ex.com/a", b"<html>v1</html>"));
        assert!(!cache.is_unchanged("https://ex.com/a", b"<html>v2</html>"));
        // Unknown URL is never "unchanged".
        assert!(!cache.is_unchanged("https://ex.com/other", b"<html>v1</html>"));
    }

    #[test]
    fn conditional_headers_from_validators() {
        let mut cache = Cache::new();
        cache.record(
            "https://ex.com/a",
            b"body",
            Some("\"abc123\"".to_string()),
            Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
        );
        let headers = cache.conditional_headers("https://ex.com/a");
        assert_eq!(
            headers,
            vec![
                ("If-None-Match", "\"abc123\"".to_string()),
                (
                    "If-Modified-Since",
                    "Wed, 21 Oct 2026 07:28:00 GMT".to_string()
                ),
            ]
        );
    }

    #[test]
    fn conditional_headers_empty_without_entry_or_validators() {
        let mut cache = Cache::new();
        assert!(cache
            .conditional_headers("https://ex.com/missing")
            .is_empty());
        cache.record("https://ex.com/a", b"body", None, None);
        assert!(cache.conditional_headers("https://ex.com/a").is_empty());
    }

    #[test]
    fn record_replaces_prior_entry() {
        let mut cache = Cache::new();
        cache.record("https://ex.com/a", b"v1", Some("e1".into()), None);
        cache.record("https://ex.com/a", b"v2", Some("e2".into()), None);
        let entry = cache.get("https://ex.com/a").unwrap();
        assert_eq!(entry.etag.as_deref(), Some("e2"));
        assert_eq!(entry.content_hash, content_hash(b"v2"));
    }
}
