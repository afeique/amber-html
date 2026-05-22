//! Per-capture resource limits. See `Plans.md` (task 7.4).
//!
//! Time and byte budgets are enforced cooperatively: the pipeline constructs a
//! [`Deadline`] and checks it at safe points (and checks
//! [`ResourceLimits::within_byte_budget`] as bytes accumulate), aborting when a
//! cap is hit. OS-level **memory/CPU** caps require platform integration
//! (rlimit / cgroups / job objects around the browser process) and are
//! configured here but enforced by that integration — not yet wired.

use std::time::{Duration, Instant};

/// Caps applied to a single capture.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceLimits {
    /// Wall-clock budget for the whole capture (`None` = unlimited).
    pub max_duration: Option<Duration>,
    /// Maximum bytes to read for a capture (`None` = unlimited).
    pub max_bytes: Option<u64>,
}

impl ResourceLimits {
    /// Whether `bytes` is within the byte budget (true when unlimited).
    pub fn within_byte_budget(&self, bytes: u64) -> bool {
        self.max_bytes.is_none_or(|max| bytes <= max)
    }

    /// Whether `elapsed` has reached the time budget (false when unlimited).
    pub fn time_exceeded(&self, elapsed: Duration) -> bool {
        self.max_duration.is_some_and(|max| elapsed >= max)
    }

    /// Start a [`Deadline`] for the configured time budget.
    pub fn deadline(&self) -> Deadline {
        Deadline::new(self.max_duration)
    }
}

/// A wall-clock deadline started "now", checked cooperatively during a capture.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    start: Instant,
    limit: Option<Duration>,
}

impl Deadline {
    /// Start a deadline with an optional time `limit` (`None` = no deadline).
    pub fn new(limit: Option<Duration>) -> Self {
        Self {
            start: Instant::now(),
            limit,
        }
    }

    /// Whether the deadline has passed (always false when there is no limit).
    pub fn exceeded(&self) -> bool {
        self.limit.is_some_and(|l| self.start.elapsed() >= l)
    }

    /// Time left before the deadline, or `None` when there is no limit.
    pub fn remaining(&self) -> Option<Duration> {
        self.limit.map(|l| l.saturating_sub(self.start.elapsed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_by_default() {
        let limits = ResourceLimits::default();
        assert!(limits.within_byte_budget(u64::MAX));
        assert!(!limits.time_exceeded(Duration::from_secs(86_400)));
    }

    #[test]
    fn byte_budget_boundary() {
        let limits = ResourceLimits {
            max_bytes: Some(1000),
            ..Default::default()
        };
        assert!(limits.within_byte_budget(1000)); // at the cap is OK
        assert!(!limits.within_byte_budget(1001)); // over is not
    }

    #[test]
    fn time_budget_boundary() {
        let limits = ResourceLimits {
            max_duration: Some(Duration::from_secs(5)),
            ..Default::default()
        };
        assert!(!limits.time_exceeded(Duration::from_secs(4)));
        assert!(limits.time_exceeded(Duration::from_secs(5)));
        assert!(limits.time_exceeded(Duration::from_secs(6)));
    }

    #[test]
    fn deadline_without_limit_never_exceeds() {
        let d = Deadline::new(None);
        assert!(!d.exceeded());
        assert_eq!(d.remaining(), None);
    }

    #[test]
    fn fresh_deadline_with_generous_limit_has_time_left() {
        let d = ResourceLimits {
            max_duration: Some(Duration::from_secs(3600)),
            ..Default::default()
        }
        .deadline();
        assert!(!d.exceeded()); // just started
        assert!(d.remaining().unwrap() > Duration::from_secs(3590));
    }
}
