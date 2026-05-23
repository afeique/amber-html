//! Visual / content change detection for page monitoring. See `Plans.md`
//! (task 9.3).
//!
//! [`compare`] diffs two captures of the same URL — e.g. consecutive scheduled
//! captures ([`crate::recurring`], 7.7) — flagging **content** changes (the line
//! diff of 3.7) and **visual** changes (the screenshot bytes differ). Composing
//! this over a schedule turns recurring captures into a change monitor.

use crate::cache::content_hash;
use crate::diff::{diff_lines, LineDiff};

/// What changed between a previous and current capture.
#[derive(Debug, Clone)]
pub struct ChangeReport {
    /// The readable text changed (the line diff is non-empty).
    pub content_changed: bool,
    /// The screenshot changed (both present and their bytes differ).
    pub visual_changed: bool,
    /// The line-level content diff (added/removed lines).
    pub content_diff: LineDiff,
}

impl ChangeReport {
    /// Whether anything (content or visual) changed.
    pub fn any_change(&self) -> bool {
        self.content_changed || self.visual_changed
    }
}

/// Compare a previous vs. current capture: `*_text` are the readable texts,
/// `*_png` the optional screenshots.
///
/// Content change = a non-empty line diff. Visual change = both screenshots are
/// present and their SHA-256 differs (a coarse "changed/unchanged" signal — a
/// pixel-level diff would need image decoding); a missing screenshot on either
/// side means no visual comparison (`visual_changed = false`).
pub fn compare(
    prev_text: &str,
    cur_text: &str,
    prev_png: Option<&[u8]>,
    cur_png: Option<&[u8]>,
) -> ChangeReport {
    let content_diff = diff_lines(prev_text, cur_text);
    let visual_changed = match (prev_png, cur_png) {
        (Some(a), Some(b)) => content_hash(a) != content_hash(b),
        _ => false,
    };
    ChangeReport {
        content_changed: content_diff.is_changed(),
        visual_changed,
        content_diff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_change_for_identical_text_and_no_screenshots() {
        let r = compare("line one\nline two", "line one\nline two", None, None);
        assert!(!r.any_change());
        assert!(!r.content_changed && !r.visual_changed);
    }

    #[test]
    fn flags_content_change() {
        let r = compare("the old text", "the new text", None, None);
        assert!(r.content_changed);
        assert!(r.any_change());
        assert!(!r.content_diff.added.is_empty() || !r.content_diff.removed.is_empty());
    }

    #[test]
    fn flags_visual_change_when_screenshots_differ() {
        let r = compare("same", "same", Some(b"PNG-A"), Some(b"PNG-B"));
        assert!(r.visual_changed);
        assert!(!r.content_changed);
        assert!(r.any_change());
    }

    #[test]
    fn no_visual_change_for_identical_screenshots() {
        let r = compare("same", "same", Some(b"PNG"), Some(b"PNG"));
        assert!(!r.visual_changed);
        assert!(!r.any_change());
    }

    #[test]
    fn missing_screenshot_means_no_visual_comparison() {
        let r = compare("same", "same", Some(b"PNG"), None);
        assert!(!r.visual_changed);
    }
}
