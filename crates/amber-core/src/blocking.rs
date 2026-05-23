//! Resource blocking for faster, leaner captures (and token savings). See
//! `Plans.md` (task 2.4).
//!
//! [`BlockPolicy`] decides which requests to drop — by resource type
//! (images/media/fonts) and by URL substring (ad/tracker hosts). This is the
//! pure policy + CDP command-construction layer; the render path enforces it by
//! sending [`BlockPolicy::set_blocked_urls_command`] (`Network.setBlockedURLs`),
//! which drops both the substring patterns and per-type file-extension patterns.

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

    /// Wildcard URL patterns for `Network.setBlockedURLs`: the configured
    /// substrings, plus file-extension patterns for each blocked resource type.
    ///
    /// Type blocking is enforced by extension here (e.g. `*.png*`) rather than
    /// by CDP request interception — `setBlockedURLs` simply drops matches, with
    /// no per-request pausing to manage (and so no risk of stalling a render).
    /// The trade-off is a heuristic: extensionless resource URLs (e.g. an image
    /// served from `/img?id=5`) slip through; byte-perfect type blocking would
    /// need `Fetch` interception.
    pub fn blocked_url_patterns(&self) -> Vec<String> {
        const IMAGE_EXTS: &[&str] = &[
            "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "avif",
        ];
        const MEDIA_EXTS: &[&str] = &["mp4", "webm", "ogg", "mp3", "wav", "m4a", "mov", "avi"];
        const FONT_EXTS: &[&str] = &["woff", "woff2", "ttf", "otf", "eot"];

        let mut patterns: Vec<String> = self
            .blocked_url_substrings
            .iter()
            .map(|s| format!("*{s}*"))
            .collect();
        let mut add_exts = |exts: &[&str]| {
            patterns.extend(exts.iter().map(|ext| format!("*.{ext}*")));
        };
        if self.block_images {
            add_exts(IMAGE_EXTS);
        }
        if self.block_media {
            add_exts(MEDIA_EXTS);
        }
        if self.block_fonts {
            add_exts(FONT_EXTS);
        }
        patterns
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
    fn blocked_types_add_extension_patterns() {
        let policy = BlockPolicy {
            block_images: true,
            block_fonts: true,
            ..Default::default()
        };
        let patterns = policy.blocked_url_patterns();
        assert!(patterns.contains(&"*.png*".to_string()));
        assert!(patterns.contains(&"*.webp*".to_string()));
        assert!(patterns.contains(&"*.woff2*".to_string()));
        // Media wasn't requested, so its extensions aren't added.
        assert!(!patterns.iter().any(|p| p == "*.mp4*"));
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
