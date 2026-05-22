//! Resource blocking for faster, leaner captures (and token savings). See
//! `Plans.md` (task 2.4).
//!
//! [`BlockPolicy`] decides which requests to drop — by resource type
//! (images/media/fonts) and by URL substring (ad/tracker hosts). This is the
//! pure policy + CDP command-construction layer; the render path enforces it
//! (`Network.setBlockedURLs` for URL patterns, request interception for types).

use serde_json::{json, Value};

/// The request resource type, a subset of CDP's `Network.ResourceType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Document,
    Stylesheet,
    Image,
    Media,
    Font,
    Script,
    Xhr,
    Other,
}

/// Which requests to block during a capture.
#[derive(Debug, Clone, Default)]
pub struct BlockPolicy {
    pub block_images: bool,
    pub block_media: bool,
    pub block_fonts: bool,
    /// URL substrings to block (e.g. ad/tracker hosts).
    pub blocked_url_substrings: Vec<String>,
}

impl BlockPolicy {
    /// Whether a request for `url` of `kind` should be blocked.
    pub fn should_block(&self, url: &str, kind: ResourceType) -> bool {
        let type_blocked = match kind {
            ResourceType::Image => self.block_images,
            ResourceType::Media => self.block_media,
            ResourceType::Font => self.block_fonts,
            _ => false,
        };
        type_blocked
            || self
                .blocked_url_substrings
                .iter()
                .any(|s| url.contains(s.as_str()))
    }

    /// Wildcard URL patterns for `Network.setBlockedURLs` from the configured
    /// substrings. (Type-based blocking is enforced via request interception.)
    pub fn blocked_url_patterns(&self) -> Vec<String> {
        self.blocked_url_substrings
            .iter()
            .map(|s| format!("*{s}*"))
            .collect()
    }

    /// The `Network.setBlockedURLs` CDP command, or `None` when no URL patterns
    /// are configured.
    pub fn set_blocked_urls_command(&self) -> Option<(&'static str, Value)> {
        let urls = self.blocked_url_patterns();
        if urls.is_empty() {
            None
        } else {
            Some(("Network.setBlockedURLs", json!({ "urls": urls })))
        }
    }

    /// Add a common ad/tracker host preset to the blocked substrings.
    pub fn with_ad_trackers(mut self) -> Self {
        const HOSTS: [&str; 6] = [
            "doubleclick.net",
            "googlesyndication.com",
            "google-analytics.com",
            "googletagmanager.com",
            "scorecardresearch.com",
            "adservice.google.com",
        ];
        self.blocked_url_substrings
            .extend(HOSTS.iter().map(|h| h.to_string()));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_by_resource_type() {
        let policy = BlockPolicy {
            block_images: true,
            ..Default::default()
        };
        assert!(policy.should_block("https://x.com/a.png", ResourceType::Image));
        assert!(!policy.should_block("https://x.com/a.png", ResourceType::Document));
        assert!(!policy.should_block("https://x.com/v.mp4", ResourceType::Media));
    }

    #[test]
    fn blocks_by_url_substring() {
        let policy = BlockPolicy {
            blocked_url_substrings: vec!["tracker.io".to_string()],
            ..Default::default()
        };
        assert!(policy.should_block("https://tracker.io/p.js", ResourceType::Script));
        assert!(!policy.should_block("https://example.com/p.js", ResourceType::Script));
    }

    #[test]
    fn url_patterns_wrap_substrings() {
        let policy = BlockPolicy {
            blocked_url_substrings: vec!["ads.example".to_string()],
            ..Default::default()
        };
        assert_eq!(
            policy.blocked_url_patterns(),
            vec!["*ads.example*".to_string()]
        );
    }

    #[test]
    fn set_blocked_urls_command_present_only_with_patterns() {
        assert!(BlockPolicy::default().set_blocked_urls_command().is_none());
        let policy = BlockPolicy {
            blocked_url_substrings: vec!["ads".to_string()],
            ..Default::default()
        };
        let (method, params) = policy.set_blocked_urls_command().unwrap();
        assert_eq!(method, "Network.setBlockedURLs");
        assert_eq!(params["urls"][0], "*ads*");
    }

    #[test]
    fn ad_tracker_preset_blocks_known_hosts() {
        let policy = BlockPolicy::default().with_ad_trackers();
        assert!(policy.should_block("https://stats.g.doubleclick.net/x", ResourceType::Script));
        assert!(!policy.should_block("https://example.com/app.js", ResourceType::Script));
    }
}
