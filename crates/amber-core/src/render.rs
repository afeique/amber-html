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
/// How many times to scroll to the bottom when auto-scroll is enabled.
const AUTO_SCROLL_STEPS: u32 = 4;
/// Pause between auto-scroll steps so lazy content can load.
const AUTO_SCROLL_INTERVAL: Duration = Duration::from_millis(150);

/// Capture `url` by rendering it in a real browser, producing the requested
/// representations from a single pass.
#[tracing::instrument(level = "debug", name = "render", skip_all, fields(url = %url))]
pub(crate) fn capture(
    chromium: &Path,
    url: &Url,
    formats: &[OutputFormat],
    opts: &CaptureOptions,
) -> Result<RawCapture> {
    let cdp = PipeCdp::spawn(chromium, &browser_args(opts.headed, opts.proxy.as_deref()))
        .map_err(browser_err)?;

    // Over the debug pipe the connection is browser-level; attach to a fresh
    // page target (CDP "flatten" mode) so Page.*/Network.*/Runtime.* commands
    // are available, routed via the returned sessionId.
    let target = cmd(&cdp, "Target.createTarget", json!({ "url": "about:blank" }))?;
    let target_id = target
        .get("targetId")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Browser("Target.createTarget returned no targetId".into()))?;
    let attached = cmd(
        &cdp,
        "Target.attachToTarget",
        json!({ "targetId": target_id, "flatten": true }),
    )?;
    let session = attached
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Browser("Target.attachToTarget returned no sessionId".into()))?
        .to_string();
    let sid = Some(session.as_str());

    scmd(&cdp, sid, "Page.enable", json!({}))?;
    scmd(&cdp, sid, "Network.enable", json!({}))?;
    scmd(
        &cdp,
        sid,
        "Page.setLifecycleEventsEnabled",
        json!({ "enabled": true }),
    )?;

    // Apply device/locale/timezone/dark-mode emulation before navigating.
    for (method, params) in crate::emulation::commands(&opts.emulation) {
        scmd(&cdp, sid, method, params)?;
    }

    let events = cdp.events();

    scmd(&cdp, sid, "Page.navigate", json!({ "url": url.as_str() }))?;
    settle(&cdp, sid, events.as_ref(), &opts.settle);

    if let Some(condition) = opts.wait_for.as_deref() {
        wait_for_ready(&cdp, sid, condition);
    }

    let mut raw = RawCapture {
        final_url: url.to_string(),
        used_browser: true,
        ..Default::default()
    };

    // Rendered DOM backs --markdown / --readable (and --html via the MHTML
    // transform). Always captured; it's a single cheap evaluate.
    raw.rendered_html = Some(eval_string(
        &cdp,
        sid,
        "document.documentElement.outerHTML",
    )?);

    let want = |f: OutputFormat| formats.contains(&f);

    // MHTML is also the source for the single-file --html transform.
    if want(OutputFormat::Mhtml) || want(OutputFormat::Html) {
        let r = scmd(
            &cdp,
            sid,
            "Page.captureSnapshot",
            json!({ "format": "mhtml" }),
        )?;
        raw.mhtml = r.get("data").and_then(Value::as_str).map(str::to_owned);
    }
    if want(OutputFormat::Screenshot) {
        let r = scmd(
            &cdp,
            sid,
            "Page.captureScreenshot",
            json!({ "format": "png", "captureBeyondViewport": true }),
        )?;
        if let Some(b64) = r.get("data").and_then(Value::as_str) {
            raw.screenshot_png = Some(decode_b64(b64)?);
        }
    }
    if want(OutputFormat::Pdf) {
        let r = scmd(
            &cdp,
            sid,
            "Page.printToPDF",
            json!({ "printBackground": true }),
        )?;
        if let Some(b64) = r.get("data").and_then(Value::as_str) {
            raw.pdf = Some(decode_b64(b64)?);
        }
    }
    if opts.accessibility {
        scmd(&cdp, sid, "Accessibility.enable", json!({}))?;
        let r = scmd(&cdp, sid, "Accessibility.getFullAXTree", json!({}))?;
        raw.accessibility_tree = r.get("nodes").cloned();
    }

    Ok(raw)
    // `cdp` is dropped here → the Chromium child is killed.
}

/// Chromium flags for capture. Headless by default (the right mode for
/// servers/CI); `headed` opts into a visible window (a stealthier escalation
/// that needs a display).
fn browser_args(headed: bool, proxy: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if !headed {
        args.push("--headless=new".to_string());
    }
    args.push("--hide-scrollbars".to_string());
    args.push("--disable-gpu".to_string());
    // Bring-your-own proxy (8.4): route all browser traffic through it.
    if let Some(proxy) = proxy {
        args.push(format!("--proxy-server={proxy}"));
    }
    // Linux/CI environments typically require --no-sandbox; macOS does not.
    if cfg!(target_os = "linux") {
        args.push("--no-sandbox".to_string());
    }
    args
}

/// Send a browser-level CDP command (no session), mapping transport errors.
fn cmd(cdp: &PipeCdp, method: &str, params: Value) -> Result<Value> {
    cdp.send(method, params).map_err(browser_err)
}

/// Send a session-scoped CDP command (Page.*/Network.*/Runtime.*).
fn scmd(cdp: &PipeCdp, session: Option<&str>, method: &str, params: Value) -> Result<Value> {
    cdp.send_to(session, method, params).map_err(browser_err)
}

/// `Runtime.evaluate` an expression in the page session and return its string
/// value (or "").
fn eval_string(cdp: &PipeCdp, session: Option<&str>, expression: &str) -> Result<String> {
    let r = scmd(
        cdp,
        session,
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
fn settle(
    cdp: &PipeCdp,
    session: Option<&str>,
    events: Option<&Receiver<Value>>,
    policy: &SettlePolicy,
) {
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

    // Auto-scroll to the bottom in steps to trigger lazy-loaded content.
    if policy.auto_scroll {
        for _ in 0..AUTO_SCROLL_STEPS {
            let _ = scmd(
                cdp,
                session,
                "Runtime.evaluate",
                json!({ "expression": "window.scrollTo(0, document.body.scrollHeight)" }),
            );
            std::thread::sleep(AUTO_SCROLL_INTERVAL);
        }
    }

    if policy.fonts_ready {
        let _ = scmd(
            cdp,
            session,
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
fn wait_for_ready(cdp: &PipeCdp, session: Option<&str>, condition: &str) {
    let expr = wait_for_expression(condition);
    let start = Instant::now();
    while start.elapsed() < WAIT_FOR_TIMEOUT {
        let present = scmd(
            cdp,
            session,
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
    fn browser_args_headless_by_default_headed_opts_out() {
        let headless = browser_args(false, None);
        assert!(
            headless.iter().any(|a| a == "--headless=new"),
            "default must be headless"
        );
        assert!(headless.iter().any(|a| a == "--disable-gpu"));
        assert!(headless.iter().any(|a| a == "--hide-scrollbars"));
        // Headed mode drops the headless flag (8.3).
        let headed = browser_args(true, None);
        assert!(!headed.iter().any(|a| a == "--headless=new"));
        // --no-sandbox is added only where required (Linux/CI).
        assert_eq!(
            headless.iter().any(|a| a == "--no-sandbox"),
            cfg!(target_os = "linux")
        );
    }

    #[test]
    fn browser_args_includes_proxy_when_set() {
        let with = browser_args(false, Some("http://proxy.local:8080"));
        assert!(with
            .iter()
            .any(|a| a == "--proxy-server=http://proxy.local:8080"));
        let without = browser_args(false, None);
        assert!(!without.iter().any(|a| a.starts_with("--proxy-server")));
    }

    #[test]
    fn capture_with_missing_browser_surfaces_clean_error() {
        // A failed browser spawn is detected and surfaced as Error::Browser,
        // not a panic or a hang. (The PipeCdp child is supervised and killed on
        // Drop; a mid-capture disconnect surfaces ConnectionClosed similarly.)
        let url = Url::parse("https://example.com/").unwrap();
        let err = capture(
            Path::new("/nonexistent/amber-no-such-chromium-binary"),
            &url,
            &[OutputFormat::Markdown],
            &CaptureOptions::default(),
        )
        .unwrap_err();
        assert!(
            matches!(err, Error::Browser(_)),
            "expected Browser error, got {err:?}"
        );
    }

    #[test]
    #[ignore = "downloads ~150MB Chrome-for-Testing and drives a real browser; run with --ignored"]
    fn live_browser_capture_renders_and_screenshots() {
        let chromium = crate::browser::ensure_chromium().expect("ensure chromium");
        let url = Url::parse(
            "data:text/html,<html><body><h1>Hello Amber</h1><p>Rendered content.</p></body></html>",
        )
        .unwrap();
        let opts = CaptureOptions {
            render: crate::fetch::RenderMode::Always,
            ..Default::default()
        };
        let raw = capture(
            &chromium,
            &url,
            &[
                OutputFormat::Markdown,
                OutputFormat::Screenshot,
                OutputFormat::Mhtml,
                OutputFormat::Pdf,
            ],
            &opts,
        )
        .expect("browser capture");

        assert!(raw.used_browser, "should have used the browser");
        let html = raw.rendered_html.as_deref().unwrap_or_default();
        assert!(
            html.contains("Hello Amber"),
            "rendered HTML missing heading:\n{html}"
        );

        let png = raw.screenshot_png.as_deref().unwrap_or_default();
        assert!(
            png.starts_with(&[0x89, b'P', b'N', b'G']),
            "screenshot is not a PNG ({} bytes)",
            png.len()
        );

        assert!(raw.mhtml.is_some(), "MHTML was not captured");

        // PDF (5.5): a real PDF starts with the "%PDF" magic.
        let pdf = raw.pdf.as_deref().unwrap_or_default();
        assert!(
            pdf.starts_with(b"%PDF"),
            "PDF magic missing ({} bytes)",
            pdf.len()
        );

        // Single-file HTML (5.2): the captured MHTML flattens to a self-contained
        // document that still carries the page content.
        let single_file = crate::inline::mhtml_to_single_file_html(raw.mhtml.as_deref().unwrap());
        assert!(
            single_file.contains("Hello Amber"),
            "single-file HTML missing content"
        );
    }

    #[test]
    #[ignore = "drives a real browser; run with --ignored (Chromium cached after first run)"]
    fn live_emulation_applies_viewport() {
        use crate::emulation::{EmulationConfig, Viewport};

        let chromium = crate::browser::ensure_chromium().expect("ensure chromium");
        // The page writes its own innerWidth into the body, so the rendered DOM
        // reflects the emulated viewport.
        let url = Url::parse(
            "data:text/html,<html><body><script>document.body.textContent='W='+window.innerWidth</script></body></html>",
        )
        .unwrap();
        let opts = CaptureOptions {
            render: crate::fetch::RenderMode::Always,
            emulation: EmulationConfig {
                viewport: Some(Viewport::desktop(800, 600)),
                ..Default::default()
            },
            ..Default::default()
        };
        let raw = capture(&chromium, &url, &[OutputFormat::Markdown], &opts).expect("capture");
        let html = raw.rendered_html.as_deref().unwrap_or_default();
        assert!(
            html.contains("W=800"),
            "viewport emulation did not take effect:\n{html}"
        );
    }

    #[test]
    #[ignore = "drives a real browser; run with --ignored (Chromium cached after first run)"]
    fn live_auto_scroll_triggers_scroll_handler() {
        let chromium = crate::browser::ensure_chromium().expect("ensure chromium");
        // A tall page whose scroll handler flips a marker once scrolled.
        let url = Url::parse(
            "data:text/html,<body style='height:5000px'><div id=m>no</div>\
             <script>addEventListener('scroll',()=>{document.getElementById('m').textContent='yes'})</script></body>",
        )
        .unwrap();
        let opts = CaptureOptions {
            render: crate::fetch::RenderMode::Always,
            settle: SettlePolicy {
                auto_scroll: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let raw = capture(&chromium, &url, &[OutputFormat::Markdown], &opts).expect("capture");
        let html = raw.rendered_html.as_deref().unwrap_or_default();
        assert!(
            html.contains(">yes<"),
            "auto-scroll did not fire a scroll event:\n{html}"
        );
    }

    #[test]
    #[ignore = "drives a real browser; run with --ignored (Chromium cached after first run)"]
    fn live_accessibility_tree_capture() {
        let chromium = crate::browser::ensure_chromium().expect("ensure chromium");
        let url = Url::parse(
            "data:text/html,<html><body><h1>Hello Amber</h1><button>Go</button></body></html>",
        )
        .unwrap();
        let opts = CaptureOptions {
            render: crate::fetch::RenderMode::Always,
            accessibility: true,
            ..Default::default()
        };
        let raw = capture(&chromium, &url, &[OutputFormat::Markdown], &opts).expect("capture");
        let tree = raw.accessibility_tree.as_ref().expect("a11y tree captured");
        assert!(
            tree.as_array().is_some_and(|nodes| !nodes.is_empty()),
            "accessibility tree has no nodes: {tree}"
        );
    }

    #[test]
    #[ignore = "drives a real browser; run with --ignored (Chromium cached after first run)"]
    fn live_capture_is_reproducible() {
        let chromium = crate::browser::ensure_chromium().expect("ensure chromium");
        let url = Url::parse(
            "data:text/html,<html><body><h1>Reproducible</h1><p>Same every time.</p></body></html>",
        )
        .unwrap();
        let opts = CaptureOptions {
            render: crate::fetch::RenderMode::Always,
            ..Default::default()
        };
        // Two captures of the same input with the same pinned browser must agree.
        let a = capture(&chromium, &url, &[OutputFormat::Markdown], &opts).expect("capture a");
        let b = capture(&chromium, &url, &[OutputFormat::Markdown], &opts).expect("capture b");
        let md_a = crate::extract::to_markdown(a.rendered_html.as_deref().unwrap_or_default());
        let md_b = crate::extract::to_markdown(b.rendered_html.as_deref().unwrap_or_default());
        assert_eq!(md_a, md_b, "repeated captures differ");
        assert!(md_a.contains("Reproducible"));
    }
}
