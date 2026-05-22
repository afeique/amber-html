//! Crawl store: an in-memory, JSON-persistable, queryable record of crawl
//! results keyed by URL. See `Plans.md` (task 3.5).
//!
//! The store maps each crawled URL to a [`StoredPage`] (its content hash and
//! fetch time). It is queryable in memory ([`CrawlStore::get`],
//! [`CrawlStore::pages`]) and persists to / loads from a JSON file
//! ([`CrawlStore::save`] / [`CrawlStore::load`]).

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::error::Result;

/// A stored crawl result for one URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPage {
    /// The (final) URL of the page.
    pub url: String,
    /// Content fingerprint (see [`crate::cache::content_hash`]).
    pub content_hash: String,
    /// When the page was fetched (RFC 3339).
    pub fetched_at: String,
}

/// An in-memory, JSON-persistable store of crawl results keyed by URL. Entries
/// are kept in sorted URL order for deterministic output.
#[derive(Debug, Default, Clone)]
pub struct CrawlStore {
    pages: BTreeMap<String, StoredPage>,
}

impl CrawlStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace the entry for `page.url`.
    pub fn upsert(&mut self, page: StoredPage) {
        self.pages.insert(page.url.clone(), page);
    }

    /// Record a crawled page from its raw body, hashing it and stamping the
    /// current time.
    pub fn record(&mut self, url: &str, body: &[u8]) {
        self.upsert(StoredPage {
            url: url.to_string(),
            content_hash: crate::cache::content_hash(body),
            fetched_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// The stored entry for `url`, if any.
    pub fn get(&self, url: &str) -> Option<&StoredPage> {
        self.pages.get(url)
    }

    /// Iterate all stored pages in sorted URL order.
    pub fn pages(&self) -> impl Iterator<Item = &StoredPage> {
        self.pages.values()
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Serialize the store to a pretty JSON array.
    pub fn to_json(&self) -> String {
        let arr: Vec<Value> = self
            .pages
            .values()
            .map(|p| {
                json!({
                    "url": p.url,
                    "content_hash": p.content_hash,
                    "fetched_at": p.fetched_at,
                })
            })
            .collect();
        serde_json::to_string_pretty(&Value::Array(arr)).unwrap_or_else(|_| "[]".to_string())
    }

    /// Parse a store from a JSON array. Best-effort: malformed entries (or a
    /// non-array document) are skipped, yielding whatever parsed cleanly.
    pub fn from_json(s: &str) -> CrawlStore {
        let mut store = CrawlStore::new();
        if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(s) {
            for item in items {
                let (Some(url), Some(hash), Some(at)) = (
                    item.get("url").and_then(Value::as_str),
                    item.get("content_hash").and_then(Value::as_str),
                    item.get("fetched_at").and_then(Value::as_str),
                ) else {
                    continue;
                };
                store.upsert(StoredPage {
                    url: url.to_string(),
                    content_hash: hash.to_string(),
                    fetched_at: at.to_string(),
                });
            }
        }
        store
    }

    /// Persist the store to `path` as JSON.
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_json())?;
        Ok(())
    }

    /// Load a store from a JSON file written by [`save`](Self::save).
    pub fn load(path: &Path) -> Result<CrawlStore> {
        let contents = std::fs::read_to_string(path)?;
        Ok(CrawlStore::from_json(&contents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, hash: &str) -> StoredPage {
        StoredPage {
            url: url.to_string(),
            content_hash: hash.to_string(),
            fetched_at: "2026-01-01T00:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn upsert_get_and_query() {
        let mut store = CrawlStore::new();
        assert!(store.is_empty());
        store.upsert(page("https://ex.com/b", "h2"));
        store.upsert(page("https://ex.com/a", "h1"));
        assert_eq!(store.len(), 2);
        assert_eq!(store.get("https://ex.com/a").unwrap().content_hash, "h1");
        // Sorted URL order.
        let urls: Vec<&str> = store.pages().map(|p| p.url.as_str()).collect();
        assert_eq!(urls, vec!["https://ex.com/a", "https://ex.com/b"]);
    }

    #[test]
    fn upsert_replaces_existing() {
        let mut store = CrawlStore::new();
        store.upsert(page("https://ex.com/a", "old"));
        store.upsert(page("https://ex.com/a", "new"));
        assert_eq!(store.len(), 1);
        assert_eq!(store.get("https://ex.com/a").unwrap().content_hash, "new");
    }

    #[test]
    fn record_hashes_and_timestamps() {
        let mut store = CrawlStore::new();
        store.record("https://ex.com/a", b"hello");
        let p = store.get("https://ex.com/a").unwrap();
        assert_eq!(p.content_hash, crate::cache::content_hash(b"hello"));
        assert!(!p.fetched_at.is_empty());
    }

    #[test]
    fn json_round_trip() {
        let mut store = CrawlStore::new();
        store.upsert(page("https://ex.com/a", "h1"));
        store.upsert(page("https://ex.com/b", "h2"));
        let restored = CrawlStore::from_json(&store.to_json());
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.get("https://ex.com/b").unwrap().content_hash, "h2");
    }

    #[test]
    fn from_json_skips_malformed() {
        assert!(CrawlStore::from_json("not json").is_empty());
        assert!(CrawlStore::from_json("{}").is_empty());
        // Array with one good and one incomplete entry → keeps the good one.
        let mixed = r#"[{"url":"https://ex.com/a","content_hash":"h","fetched_at":"t"},
                        {"url":"https://ex.com/b"}]"#;
        assert_eq!(CrawlStore::from_json(mixed).len(), 1);
    }

    #[test]
    fn save_and_load_round_trip() {
        let mut store = CrawlStore::new();
        store.upsert(page("https://ex.com/a", "h1"));
        store.upsert(page("https://ex.com/b", "h2"));

        let path = std::env::temp_dir()
            .join(format!("amber-store-test-{}.json", std::process::id()));
        store.save(&path).unwrap();
        let loaded = CrawlStore::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get("https://ex.com/a").unwrap(), &page("https://ex.com/a", "h1"));
    }
}
