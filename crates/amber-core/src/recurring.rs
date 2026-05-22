//! Recurring-capture scheduling. See `Plans.md` (task 7.7).
//!
//! [`Cadence`] is the pure scheduling math: given a first-run time and an
//! interval, compute the next run at or after a reference instant (and the
//! delay until then). Times are Unix-epoch seconds so the logic is fully
//! deterministic and testable; a runner loop that actually re-captures on the
//! cadence is the integration layer.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A fixed-interval schedule anchored at a first-run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cadence {
    /// First run, in seconds since the Unix epoch.
    start_epoch_secs: u64,
    /// Interval between runs, in whole seconds (minimum 1).
    interval_secs: u64,
}

impl Cadence {
    /// A cadence starting at `start_epoch_secs`, repeating every `interval`
    /// (clamped to a minimum of 1 second).
    pub fn new(start_epoch_secs: u64, interval: Duration) -> Self {
        Self {
            start_epoch_secs,
            interval_secs: interval.as_secs().max(1),
        }
    }

    /// The next scheduled run (epoch secs) at or after `now_epoch_secs`.
    pub fn next_run(&self, now_epoch_secs: u64) -> u64 {
        if now_epoch_secs <= self.start_epoch_secs {
            return self.start_epoch_secs;
        }
        let elapsed = now_epoch_secs - self.start_epoch_secs;
        let periods = elapsed.div_ceil(self.interval_secs);
        self.start_epoch_secs + periods * self.interval_secs
    }

    /// Seconds to wait from `now_epoch_secs` until the next run (0 if due now).
    pub fn delay_from(&self, now_epoch_secs: u64) -> u64 {
        self.next_run(now_epoch_secs).saturating_sub(now_epoch_secs)
    }
}

/// Current time in seconds since the Unix epoch.
fn epoch_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Run `capture` on `cadence` for `max_runs` iterations, waiting for each
/// scheduled boundary in between. Returns the number of runs performed.
///
/// Generic over the clock (`now`) and `sleep` so the loop is testable without
/// real waiting; see [`run_schedule`] for the wall-clock wrapper.
pub fn run_schedule_with<F, S, N>(
    cadence: Cadence,
    max_runs: usize,
    mut now: N,
    mut sleep: S,
    mut capture: F,
) -> usize
where
    F: FnMut(),
    S: FnMut(Duration),
    N: FnMut() -> u64,
{
    let mut runs = 0;
    let mut target = cadence.next_run(now());
    while runs < max_runs {
        let t = now();
        if t < target {
            sleep(Duration::from_secs(target - t));
        }
        capture();
        runs += 1;
        // Advance to the next boundary strictly after this run.
        target = cadence.next_run(target + 1);
    }
    runs
}

/// Run `capture` on `cadence` for `max_runs` iterations against the wall clock,
/// sleeping until each scheduled run. A recurring capture loop (Plans.md 7.7).
pub fn run_schedule<F: FnMut()>(cadence: Cadence, max_runs: usize, capture: F) {
    run_schedule_with(cadence, max_runs, epoch_now, std::thread::sleep, capture);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cadence() -> Cadence {
        Cadence::new(1000, Duration::from_secs(60))
    }

    #[test]
    fn before_start_runs_at_start() {
        let c = cadence();
        assert_eq!(c.next_run(900), 1000);
        assert_eq!(c.next_run(1000), 1000);
        assert_eq!(c.delay_from(900), 100);
    }

    #[test]
    fn next_run_rounds_up_to_the_next_boundary() {
        let c = cadence();
        assert_eq!(c.next_run(1030), 1060); // mid-interval → next boundary
        assert_eq!(c.next_run(1061), 1120);
    }

    #[test]
    fn on_a_boundary_is_due_now() {
        let c = cadence();
        assert_eq!(c.next_run(1060), 1060);
        assert_eq!(c.delay_from(1060), 0);
    }

    #[test]
    fn delay_counts_down_within_an_interval() {
        let c = cadence();
        assert_eq!(c.delay_from(1030), 30);
        assert_eq!(c.delay_from(1059), 1);
    }

    #[test]
    fn zero_interval_is_clamped_to_one_second() {
        let c = Cadence::new(0, Duration::from_secs(0));
        // Must not divide by zero; advances one second at a time.
        assert_eq!(c.next_run(5), 5);
        assert_eq!(c.next_run(6), 6);
    }

    #[test]
    fn run_schedule_invokes_capture_each_tick() {
        use std::cell::Cell;

        // A mock clock the injected sleep advances, so no real waiting occurs.
        let clock = Cell::new(1000u64);
        let calls = Cell::new(0u32);
        let waited = Cell::new(0u64);

        let runs = run_schedule_with(
            cadence(),
            3,
            || clock.get(),
            |d| {
                clock.set(clock.get() + d.as_secs());
                waited.set(waited.get() + d.as_secs());
            },
            || calls.set(calls.get() + 1),
        );

        assert_eq!(runs, 3);
        assert_eq!(calls.get(), 3);
        // First run is immediate (due now); the next two each wait one interval.
        assert_eq!(waited.get(), 120);
    }
}
