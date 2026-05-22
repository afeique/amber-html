//! Change detection: a line-level diff between two captures of the same URL.
//! See `Plans.md` (task 3.7).
//!
//! [`diff_lines`] computes the added and removed lines between an old and a new
//! text rendering (e.g. two Markdown or readable captures) using a longest-
//! common-subsequence alignment, so reordered or duplicated lines are handled
//! precisely rather than as a naive set difference. Pure and I/O-free.

/// The line-level difference between two texts: lines present only in the new
/// text (`added`) and only in the old text (`removed`), in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl LineDiff {
    /// Whether anything changed between the two texts.
    pub fn is_changed(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }
}

/// Compute the [`LineDiff`] between `old` and `new` at line granularity.
///
/// Runs in O(m·n) time and memory in the line counts, which is fine for
/// page-sized captures.
pub fn diff_lines(old: &str, new: &str) -> LineDiff {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let (m, n) = (old_lines.len(), new_lines.len());

    // dp[i][j] = LCS length of old_lines[i..] and new_lines[j..].
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            dp[i][j] = if old_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    // Backtrack: matched lines are unchanged; the rest are removed/added.
    let mut diff = LineDiff::default();
    let (mut i, mut j) = (0, 0);
    while i < m && j < n {
        if old_lines[i] == new_lines[j] {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            diff.removed.push(old_lines[i].to_string());
            i += 1;
        } else {
            diff.added.push(new_lines[j].to_string());
            j += 1;
        }
    }
    diff.removed.extend(old_lines[i..].iter().map(|s| s.to_string()));
    diff.added.extend(new_lines[j..].iter().map(|s| s.to_string()));
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_changes() {
        let d = diff_lines("a\nb\nc", "a\nb\nc");
        assert!(!d.is_changed());
        assert!(d.added.is_empty() && d.removed.is_empty());
    }

    #[test]
    fn detects_added_lines() {
        let d = diff_lines("a\nc", "a\nb\nc");
        assert_eq!(d.added, vec!["b".to_string()]);
        assert!(d.removed.is_empty());
        assert!(d.is_changed());
    }

    #[test]
    fn detects_removed_lines() {
        let d = diff_lines("a\nb\nc", "a\nc");
        assert_eq!(d.removed, vec!["b".to_string()]);
        assert!(d.added.is_empty());
    }

    #[test]
    fn detects_replacements_via_lcs() {
        // The common subsequence is [a, c]; b is removed and x is added.
        let d = diff_lines("a\nb\nc", "a\nx\nc");
        assert_eq!(d.removed, vec!["b".to_string()]);
        assert_eq!(d.added, vec!["x".to_string()]);
    }

    #[test]
    fn handles_empty_sides() {
        let d = diff_lines("", "a\nb");
        assert_eq!(d.added, vec!["a".to_string(), "b".to_string()]);
        assert!(d.removed.is_empty());

        let d2 = diff_lines("a\nb", "");
        assert_eq!(d2.removed, vec!["a".to_string(), "b".to_string()]);
        assert!(d2.added.is_empty());
    }

    #[test]
    fn reordered_lines_are_not_spurious_matches() {
        // a,b -> b,a : LCS is one line; one add + one remove.
        let d = diff_lines("a\nb", "b\na");
        assert_eq!(d.added.len() + d.removed.len(), 2);
        assert!(d.is_changed());
    }
}
