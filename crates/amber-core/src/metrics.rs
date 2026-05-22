//! In-process metrics for observability. See `Plans.md` (task 7.3).
//!
//! [`Metrics`] accumulates capture-pipeline counters (static fetches, browser
//! renders, cache hits/misses, bytes, fetch latency). [`Metrics::snapshot`]
//! exposes a derived, observable view (render rate, cache-hit rate, average
//! latency) and [`Metrics::to_json`] exports it. Callers record events as the
//! pipeline runs; for shared use across threads, wrap a `Metrics` in a `Mutex`.

use std::time::Duration;

use serde_json::json;

/// Mutable accumulator of capture-pipeline metrics.
#[derive(Debug, Default, Clone)]
pub struct Metrics {
    pages_fetched: u64,
    browser_renders: u64,
    cache_hits: u64,
    cache_misses: u64,
    bytes_fetched: u64,
    total_fetch_ms: u64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed static HTTP fetch of `bytes` taking `elapsed`.
    pub fn record_fetch(&mut self, bytes: usize, elapsed: Duration) {
        self.pages_fetched += 1;
        self.bytes_fetched += bytes as u64;
        self.total_fetch_ms += elapsed.as_millis() as u64;
    }

    /// Record a browser render pass (an escalation beyond the static tier).
    pub fn record_render(&mut self) {
        self.browser_renders += 1;
    }

    pub fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
    }

    pub fn record_cache_miss(&mut self) {
        self.cache_misses += 1;
    }

    /// A derived, observable snapshot of the current metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let captures = self.pages_fetched + self.browser_renders;
        let cache_lookups = self.cache_hits + self.cache_misses;
        MetricsSnapshot {
            pages_fetched: self.pages_fetched,
            browser_renders: self.browser_renders,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            bytes_fetched: self.bytes_fetched,
            render_rate: ratio(self.browser_renders, captures),
            cache_hit_rate: ratio(self.cache_hits, cache_lookups),
            avg_fetch_ms: ratio(self.total_fetch_ms, self.pages_fetched),
        }
    }

    /// Export the snapshot as a JSON object.
    pub fn to_json(&self) -> String {
        let s = self.snapshot();
        json!({
            "pages_fetched": s.pages_fetched,
            "browser_renders": s.browser_renders,
            "cache_hits": s.cache_hits,
            "cache_misses": s.cache_misses,
            "bytes_fetched": s.bytes_fetched,
            "render_rate": s.render_rate,
            "cache_hit_rate": s.cache_hit_rate,
            "avg_fetch_ms": s.avg_fetch_ms,
        })
        .to_string()
    }
}

/// A point-in-time, observable view of [`Metrics`], including derived rates.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricsSnapshot {
    pub pages_fetched: u64,
    pub browser_renders: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub bytes_fetched: u64,
    /// Fraction of captures that required a browser render (0.0 when none).
    pub render_rate: f64,
    /// Fraction of cache lookups that hit (0.0 when none).
    pub cache_hit_rate: f64,
    /// Mean static-fetch latency in milliseconds (0.0 when no fetches).
    pub avg_fetch_ms: f64,
}

/// `num / den` as f64, or 0.0 when `den == 0`.
fn ratio(num: u64, den: u64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metrics_have_zeroed_rates() {
        let s = Metrics::new().snapshot();
        assert_eq!(s.pages_fetched, 0);
        assert_eq!(s.render_rate, 0.0);
        assert_eq!(s.cache_hit_rate, 0.0);
        assert_eq!(s.avg_fetch_ms, 0.0);
    }

    #[test]
    fn records_and_derives_rates() {
        let mut m = Metrics::new();
        m.record_fetch(1000, Duration::from_millis(100));
        m.record_fetch(3000, Duration::from_millis(300));
        m.record_render(); // 2 fetches + 1 render = 3 captures
        m.record_cache_hit();
        m.record_cache_hit();
        m.record_cache_miss(); // 2/3 hit rate

        let s = m.snapshot();
        assert_eq!(s.pages_fetched, 2);
        assert_eq!(s.browser_renders, 1);
        assert_eq!(s.bytes_fetched, 4000);
        assert!((s.render_rate - 1.0 / 3.0).abs() < 1e-9);
        assert!((s.cache_hit_rate - 2.0 / 3.0).abs() < 1e-9);
        assert!((s.avg_fetch_ms - 200.0).abs() < 1e-9); // (100+300)/2
    }

    #[test]
    fn to_json_includes_all_fields() {
        let mut m = Metrics::new();
        m.record_fetch(10, Duration::from_millis(5));
        let v: serde_json::Value = serde_json::from_str(&m.to_json()).unwrap();
        for key in [
            "pages_fetched",
            "browser_renders",
            "cache_hits",
            "cache_misses",
            "bytes_fetched",
            "render_rate",
            "cache_hit_rate",
            "avg_fetch_ms",
        ] {
            assert!(v.get(key).is_some(), "missing key {key}");
        }
    }
}
