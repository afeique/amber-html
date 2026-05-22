//! Browser-render capture: drive a real Chromium over the CDP pipe to produce a
//! fully-rendered [`RawCapture`]. See `Plans.md`. Built on
//! [`crate::cdp::PipeCdp`].

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use url::Url;

use crate::browser::SettlePolicy;
use crate::capture::{CaptureOptions, RawCapture};
use crate::cdp::{CdpError, PipeCdp};
use crate::error::{Error, Result};
use crate::output::OutputFormat;

/// Hard cap on how long we wait for a page to settle.
const SETTLE_OVERALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Poll interval while waiting on lifecycle/network events.
const SETTLE_POLL: Duration = Duration::from_millis(250);
/// Max time to wait for a `--wait-for` selector to appear.
const WAIT_FOR_TIMEOUT: Duration = Duration::from_secs(15);

/// Capture `url` by rendering it in a real browser, producing the requested
/// representations from a single pass.
#[tracing::instrument(level = "debug", name = "render", skip_all, fields(url = %url))]
pub(crate) fn capture(
    chromium: &Path,
    url: &Url,
    formats: &[OutputFormat],
    opts: &CaptureOptions,
) -> Result<RawCapture> {
    let cdp = PipeCdp::spawn(chromium, &browser_args()).map_err(browser_err)?;

    cmd(&cdp, "Page.enable", json!({}))?;
    cmd(&cdp, "Network.enable", json!({}))?;
    cmd(&cdp, "Page.setLifecycleEventsEnabled", json!({ "enabled": true }))?;

    let events = cdp.events();

    cmd(&cdp, "Page.navigate", json!({ "url": url.as_str() }))?;
    settle(&cdp, events.as_ref(), &opts.settle);

    if let Some(condition) = opts.wait_for.as_deref() {
        wait_for_ready(&cdp, condition);
    }

    let mut raw = RawCapture {
        final_url: url.to_string(),
        used_browser: true,
        ..Default::default()
    };

    // Rendered DOM backs --markdown / --readable (and --html via the MHTML
    // transform). Always captured; it's a single cheap evaluate.
    raw.rendered_html = Some(eval_string(&cdp, "document.documentElement.outerHTML")?);

    let want = |f: OutputFormat| formats.contains(&f);

    // MHTML is also the source for the single-file --html transform.
    if want(OutputFormat::Mhtml) || want(OutputFormat::Html) {
        let r = cmd(&cdp, "Page.captureSnapshot", json!({ "format": "mhtml" }))?;
        raw.mhtml = r.get("data").and_then(Value::as_str).map(str::to_owned);
    }
    if want(OutputFormat::Screenshot) {
        let r = cmd(
            &cdp,
            "Page.captureScreenshot",
            json!({ "format": "png", "captureBeyondViewport": true }),
        )?;
        if let Some(b64) = r.get("data").and_then(Value::as_str) {
            raw.screenshot_png = Some(decode_b64(b64)?);
        }
    }
    if want(OutputFormat::Pdf) {
        let r = cmd(&cdp, "Page.printToPDF", json!({ "printBackground": true }))?;
        if let Some(b64) = r.get("data").and_then(Value::as_str) {
            raw.pdf = Some(decode_b64(b64)?);
        }
    }

    Ok(raw)
    // `cdp` is dropped here → the Chromium child is killed.
}

/// Default Chromium flags for headless capture.
fn browser_args() -> Vec<String> {
    let mut args = vec![
        "--hide-scrollbars".to_string(),
        "--disable-gpu".to_string(),
    ];
    // Linux/CI environments typically require --no-sandbox; macOS does not.
    if cfg!(target_os = "linux") {
        args.push("--no-sandbox".to_string());
    }
    args
}

/// Send a CDP command, mapping transport errors into the crate error type.
fn cmd(cdp: &PipeCdp, method: &str, params: Value) -> Result<Value> {
    cdp.send(method, params).map_err(browser_err)
}

/// `Runtime.evaluate` an expression and return its string value (or "").
fn eval_string(cdp: &PipeCdp, expression: &str) -> Result<String> {
    let r = cmd(
        cdp,
        "Runtime.evaluate",
        json!({ "expression": expression, "returnByValue": true }),
    )?;
    Ok(r.get("result")
        .and_then(|x| x.get("value"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

/// Wait until the page is "settled" before capture (Plans.md): drain lifecycle
/// and network events until load + network-idle (or a quiet period), then honor
/// `fonts.ready` and a settle delay. Best-effort; never fails the capture.
fn settle(cdp: &PipeCdp, events: Option<&Receiver<Value>>, policy: &SettlePolicy) {
    if let Some(rx) = events {
        let start = Instant::now();
        let mut tracker = SettleTracker::new(policy);
        loop {
            if start.elapsed() > SETTLE_OVERALL_TIMEOUT {
                break;
            }
            match rx.recv_timeout(SETTLE_POLL) {
                // An event arrived: feed it to the tracker.
                Ok(ev) => {
                    if tracker.observe(&ev) {
                        break;
                    }
                }
                // A quiet interval: settled if loaded with nothing in flight.
                Err(RecvTimeoutError::Timeout) => {
                    if tracker.settled_on_quiet() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    if policy.fonts_ready {
        let _ = cmd(
            cdp,
            "Runtime.evaluate",
            json!({
                "expression": "document.fonts ? document.fonts.ready.then(() => true) : true",
                "awaitPromise": true,
                "returnByValue": true
            }),
        );
    }
    if policy.settle_delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(policy.settle_delay_ms));
    }
}

/// Event-driven tracker that decides when a page has settled, per a
/// [`SettlePolicy`].
///
/// Pure (no I/O): it folds CDP lifecycle and network events into a verdict, so
/// the settle decision logic can be unit-tested without a live browser. The
/// driving loop in [`settle`] supplies events and quiet-interval ticks.
struct SettleTracker<'a> {
    policy: &'a SettlePolicy,
    /// Whether the `load` lifecycle event has fired (or wasn't required).
    loaded: bool,
    /// Outstanding network requests (sent minus finished/failed).
    inflight: i64,
}

impl<'a> SettleTracker<'a> {
    fn new(policy: &'a SettlePolicy) -> Self {
        Self {
            policy,
            // If the policy doesn't require `load`, treat the page as loaded.
            loaded: !policy.wait_load,
            inflight: 0,
        }
    }

    /// Fold one CDP event into the state; returns `true` once settled (load seen
    /// and, if `network_idle`, a network-idle lifecycle event observed).
    fn observe(&mut self, ev: &Value) -> bool {
        match ev.get("method").and_then(Value::as_str).unwrap_or("") {
            "Page.lifecycleEvent" => {
                let name = ev
                    .get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if name == "load" {
                    self.loaded = true;
                }
                if self.loaded
                    && self.policy.network_idle
                    && (name == "networkIdle" || name == "networkAlmostIdle")
                {
                    return true;
                }
            }
            "Network.requestWillBeSent" => self.inflight += 1,
            "Network.loadingFinished" | "Network.loadingFailed" => self.inflight -= 1,
            _ => {}
        }
        false
    }

    /// Verdict for a quiet interval (no event within the poll window): settled
    /// once the page has loaded and no requests are in flight.
    fn settled_on_quiet(&self) -> bool {
        self.loaded && self.inflight <= 0
    }
}

/// Build the boolean JS expression polled for a `--wait-for` condition.
///
/// A `js:` prefix marks an arbitrary boolean predicate, coerced to bool with
/// `!!(…)`; anything else is treated as a CSS selector and tested via
/// `document.querySelector`. The value is JSON-encoded in the selector case so
/// it can't break out of the expression.
fn wait_for_expression(condition: &str) -> String {
    match condition.strip_prefix("js:") {
        Some(predicate) => format!("!!({predicate})"),
        None => {
            let sel = Value::String(condition.to_string()).to_string();
            format!("!!document.querySelector({sel})")
        }
    }
}

/// Poll until the `--wait-for` condition (CSS selector or `js:` predicate)
/// becomes true (or a timeout). Best-effort.
fn wait_for_ready(cdp: &PipeCdp, condition: &str) {
    let expr = wait_for_expression(condition);
    let start = Instant::now();
    while start.elapsed() < WAIT_FOR_TIMEOUT {
        let present = cmd(
            cdp,
            "Runtime.evaluate",
            json!({ "expression": expr, "returnByValue": true }),
        )
        .ok()
        .and_then(|r| {
            r.get("result")
                .and_then(|x| x.get("value"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
        if present {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Decode a base64 payload (CDP returns screenshots/PDFs base64-encoded).
fn decode_b64(s: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| Error::Browser(format!("base64 decode failed: {e}")))
}

/// Map a transport [`CdpError`] into the crate-wide [`Error`].
fn browser_err(e: CdpError) -> Error {
    Error::Browser(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lifecycle(name: &str) -> Value {
        json!({ "method": "Page.lifecycleEvent", "params": { "name": name } })
    }
    fn net(method: &str) -> Value {
        json!({ "method": method })
    }

    #[test]
    fn settles_on_network_idle_after_load() {
        let policy = SettlePolicy::default();
        let mut t = SettleTracker::new(&policy);
        // Idle before load must not settle (content may still be arriving).
        assert!(!t.observe(&lifecycle("DOMContentLoaded")));
        assert!(!t.observe(&lifecycle("networkIdle")));
        // Once loaded, a network-idle lifecycle event settles it.
        assert!(!t.observe(&lifecycle("load")));
        assert!(t.observe(&lifecycle("networkAlmostIdle")));
    }

    #[test]
    fn quiet_period_settles_when_loaded_and_no_inflight() {
        let policy = SettlePolicy::default();
        let mut t = SettleTracker::new(&policy);
        t.observe(&lifecycle("load"));
        t.observe(&net("Network.requestWillBeSent"));
        assert!(!t.settled_on_quiet(), "a request is in flight");
        t.observe(&net("Network.loadingFinished"));
        assert!(t.settled_on_quiet(), "loaded and nothing in flight");
    }

    #[test]
    fn failed_requests_also_decrement_inflight() {
        let policy = SettlePolicy::default();
        let mut t = SettleTracker::new(&policy);
        t.observe(&lifecycle("load"));
        t.observe(&net("Network.requestWillBeSent"));
        t.observe(&net("Network.loadingFailed"));
        assert!(t.settled_on_quiet());
    }

    #[test]
    fn not_settled_before_load() {
        let policy = SettlePolicy::default();
        let t = SettleTracker::new(&policy);
        // Default policy requires `load`, which hasn't fired yet.
        assert!(!t.settled_on_quiet());
    }

    #[test]
    fn wait_load_disabled_starts_loaded() {
        let policy = SettlePolicy {
            wait_load: false,
            ..Default::default()
        };
        let t = SettleTracker::new(&policy);
        assert!(t.settled_on_quiet());
    }

    #[test]
    fn network_idle_disabled_relies_on_quiet_period() {
        let policy = SettlePolicy {
            network_idle: false,
            ..Default::default()
        };
        let mut t = SettleTracker::new(&policy);
        t.observe(&lifecycle("load"));
        // With network_idle off, an idle lifecycle event does not settle...
        assert!(!t.observe(&lifecycle("networkIdle")));
        // ...but a quiet interval (loaded, no inflight) does.
        assert!(t.settled_on_quiet());
    }

    #[test]
    fn wait_for_expression_treats_plain_value_as_selector() {
        assert_eq!(
            wait_for_expression(".main-content"),
            r#"!!document.querySelector(".main-content")"#
        );
    }

    #[test]
    fn wait_for_expression_escapes_selector_quotes() {
        // Inner quotes are JSON-escaped so they can't break out of the JS string.
        assert_eq!(
            wait_for_expression(r#"a[href="x"]"#),
            r#"!!document.querySelector("a[href=\"x\"]")"#
        );
    }

    #[test]
    fn wait_for_expression_passes_through_js_predicate() {
        assert_eq!(
            wait_for_expression("js:window.__ready === true"),
            "!!(window.__ready === true)"
        );
    }

    #[test]
    fn browser_args_are_headless_safe() {
        let args = browser_args();
        assert!(args.iter().any(|a| a == "--disable-gpu"));
        assert!(args.iter().any(|a| a == "--hide-scrollbars"));
        // --no-sandbox is added only where required (Linux/CI).
        assert_eq!(
            args.iter().any(|a| a == "--no-sandbox"),
            cfg!(target_os = "linux")
        );
    }
}
