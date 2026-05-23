//! Per-capture resource limits. See `Plans.md` (task 7.4).
//!
//! Time and byte budgets are enforced cooperatively: the pipeline constructs a
//! [`Deadline`] and checks it at safe points (and caps the static-fetch read at
//! the byte budget), aborting when a cap is hit. OS-level **memory/CPU** caps
//! (`max_memory_bytes` / `max_cpu_seconds`) are enforced on Unix by `setrlimit`
//! (`RLIMIT_AS` / `RLIMIT_CPU`) on the spawned browser child — see
//! [`crate::cdp::PipeCdp::spawn_with_caps`].

use std::time::{Duration, Instant};

/// Caps applied to a single capture.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceLimits {
    /// Wall-clock budget for the whole capture (`None` = unlimited).
    pub max_duration: Option<Duration>,
    /// Maximum bytes to read for a capture (`None` = unlimited).
    pub max_bytes: Option<u64>,
    /// Address-space cap (bytes) for the spawned browser, enforced via
    /// `RLIMIT_AS` (Unix). `None` = unlimited. **Caveat:** Chromium reserves a
    /// large virtual address space, so a low cap can prevent it from starting;
    /// also note macOS does not enforce `RLIMIT_AS`. Prefer `max_cpu_seconds`.
    pub max_memory_bytes: Option<u64>,
    /// CPU-time cap (seconds) for the spawned browser, enforced via
    /// `RLIMIT_CPU` (Unix); the process is killed (`SIGXCPU`) past it.
    /// `None` = unlimited.
    pub max_cpu_seconds: Option<u64>,
}

impl ResourceLimits {
    /// Whether any OS-level (memory/CPU) cap is configured for the browser.
    pub fn has_os_caps(&self) -> bool {
        self.max_memory_bytes.is_some() || self.max_cpu_seconds.is_some()
    }
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
        assert!(!limits.has_os_caps());
    }

    #[test]
    fn has_os_caps_reflects_memory_or_cpu() {
        assert!(ResourceLimits {
            max_memory_bytes: Some(1 << 30),
            ..Default::default()
        }
        .has_os_caps());
        assert!(ResourceLimits {
            max_cpu_seconds: Some(10),
            ..Default::default()
        }
        .has_os_caps());
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
