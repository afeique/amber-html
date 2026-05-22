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

    if let Some(selector) = opts.wait_for.as_deref() {
        wait_for_selector(&cdp, selector);
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
        let mut inflight: i64 = 0;
        let mut loaded = !policy.wait_load;
        loop {
            if start.elapsed() > SETTLE_OVERALL_TIMEOUT {
                break;
            }
            match rx.recv_timeout(SETTLE_POLL) {
                Ok(ev) => match ev.get("method").and_then(Value::as_str).unwrap_or("") {
                    "Page.lifecycleEvent" => {
                        let name = ev
                            .get("params")
                            .and_then(|p| p.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if name == "load" {
                            loaded = true;
                        }
                        if loaded
                            && policy.network_idle
                            && (name == "networkIdle" || name == "networkAlmostIdle")
                        {
                            break;
                        }
                    }
                    "Network.requestWillBeSent" => inflight += 1,
                    "Network.loadingFinished" | "Network.loadingFailed" => inflight -= 1,
                    _ => {}
                },
                Err(RecvTimeoutError::Timeout) => {
                    // Quiet period: if we've loaded with nothing in flight, done.
                    if loaded && inflight <= 0 {
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

/// Poll until `selector` appears in the DOM (or a timeout). Best-effort.
fn wait_for_selector(cdp: &PipeCdp, selector: &str) {
    // JSON-encode the selector so it can't break out of the JS expression.
    let sel = Value::String(selector.to_string()).to_string();
    let expr = format!("!!document.querySelector({sel})");
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
