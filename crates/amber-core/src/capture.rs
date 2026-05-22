//! The capture pipeline: turn a URL into a format-agnostic [`RawCapture`].
//! See `Plans.md`.

use crate::browser::SettlePolicy;
use crate::error::{Error, Result};
use crate::fetch::{self, RenderMode};
use crate::output::OutputFormat;
use crate::{detect, http};

/// Options controlling a capture pass.
///
/// `Debug` is hand-written so secrets never leak through a debug log (task
/// 3.10): the `proxy` URL is shown with credentials redacted, and `session`
/// delegates to its own redacting `Debug`.
#[derive(Clone, Default)]
pub struct CaptureOptions {
    /// Browser rendering policy (auto / always / never).
    pub render: RenderMode,
    /// When/how to consider the page "settled" before capture.
    pub settle: SettlePolicy,
    /// Optional CSS selector to wait for after settle (forces a browser).
    pub wait_for: Option<String>,
    /// Override the minimum static content length treated as sufficient.
    pub min_content: Option<usize>,
    /// Device/locale/timezone/dark-mode emulation applied on the browser path.
    pub emulation: crate::emulation::EmulationConfig,
    /// Also capture the accessibility tree (browser path) for grounding.
    pub accessibility: bool,
    /// Run the browser headed (visible window) instead of headless. Headless is
    /// the default; headed is an escalation (more stealthy, needs a display).
    pub headed: bool,
    /// Bring-your-own proxy for the browser render (e.g. `http://host:8080` or
    /// `socks5://host:1080`). Passed to Chromium as `--proxy-server`.
    pub proxy: Option<String>,
    /// Auth session state (cookies + extra request headers) sent on both the
    /// static fetch and the browser navigation, for behind-auth pages.
    pub session: crate::session::SessionState,
    /// Per-capture resource limits (time / byte budget). See [`crate::limits`].
    pub limits: crate::limits::ResourceLimits,
}

impl std::fmt::Debug for CaptureOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureOptions")
            .field("render", &self.render)
            .field("settle", &self.settle)
            .field("wait_for", &self.wait_for)
            .field("min_content", &self.min_content)
            .field("emulation", &self.emulation)
            .field("accessibility", &self.accessibility)
            .field("headed", &self.headed)
            .field(
                "proxy",
                &self.proxy.as_deref().map(crate::secrets::redact_proxy_url),
            )
            .field("session", &self.session)
            .field("limits", &self.limits)
            .finish()
    }
}

/// The raw, format-agnostic product of a single capture pass. Output emitters
/// derive concrete results from this.
#[derive(Debug, Default)]
pub struct RawCapture {
    /// The final URL after redirects.
    pub final_url: String,
    /// Raw static HTML from the cheap HTTP fetch, if taken.
    pub static_html: Option<String>,
    /// Rendered HTML / serialized DOM from the browser, if rendered.
    pub rendered_html: Option<String>,
    /// MHTML bundle (from `Page.captureSnapshot`), if captured.
    pub mhtml: Option<String>,
    /// Full-page PNG screenshot bytes, if captured.
    pub screenshot_png: Option<Vec<u8>>,
    /// PDF bytes, if captured.
    pub pdf: Option<Vec<u8>>,
    /// Accessibility tree nodes (`Accessibility.getFullAXTree`), if requested.
    pub accessibility_tree: Option<serde_json::Value>,
    /// Whether a browser was used.
    pub used_browser: bool,
}

/// Run the capture pipeline (Plans.md):
/// output gate → HTTP fetch → sufficiency → escalate → (browser render).
#[tracing::instrument(level = "debug", name = "capture", skip_all, fields(url = %url))]
pub(crate) fn run(
    url: &url::Url,
    formats: &[OutputFormat],
    opts: &CaptureOptions,
) -> Result<RawCapture> {
    // Per-capture wall-clock budget (7.4); checked before the expensive browser
    // step so a capture that has blown its budget doesn't start a render.
    let deadline = opts.limits.deadline();

    // Step 1 — output gate: some outputs (or `--render always`) require a
    // browser up front, so we don't bother with a cheap fetch.
    if fetch::browser_required_upfront(formats, opts.render) {
        tracing::debug!("output gate: browser required up front");
        return browser_capture(url, formats, opts, &deadline);
    }

    // Step 2 — cheap HTTP-first fetch (carrying any auth session state).
    let page = match http::fetch_with_session(
        url,
        &opts.session,
        opts.proxy.as_deref(),
        opts.limits.max_bytes,
    ) {
        Ok(page) => page,
        Err(err) => {
            return match opts.render {
                // User forbade the browser: the cheap tier failing is terminal.
                RenderMode::Never => Err(map_fetch_error(err, url)),
                // Otherwise escalate to the browser.
                _ => browser_capture(url, formats, opts, &deadline),
            };
        }
    };

    // The static path can only reason about HTML responses.
    if !http::content_type_is_html(page.content_type.as_deref()) {
        return match opts.render {
            RenderMode::Never => Err(Error::Fetch(format!(
                "non-HTML response ({}) and --render never",
                page.content_type
                    .as_deref()
                    .unwrap_or("unknown content-type")
            ))),
            _ => browser_capture(url, formats, opts, &deadline),
        };
    }

    // Step 3 — sufficiency: is the static HTML enough, or must we render?
    let floor = opts.min_content.unwrap_or(detect::CONTENT_FLOOR);
    match detect::assess(&page.html, floor) {
        // Clearly enough content: use what we fetched, no browser.
        detect::Sufficiency::Static => {
            tracing::debug!("static HTML sufficient; no browser");
            Ok(static_capture(page))
        }
        // `--render never`: best-effort with whatever static HTML we have.
        _ if opts.render == RenderMode::Never => {
            tracing::debug!("insufficient static HTML but --render never; using static");
            Ok(static_capture(page))
        }
        // Insufficient/ambiguous in auto mode → escalate (correctness bias).
        detect::Sufficiency::NeedsBrowser | detect::Sufficiency::Uncertain => {
            tracing::debug!("escalating to browser render");
            browser_capture(url, formats, opts, &deadline)
        }
    }
}

/// Render via a real browser: ensure a pinned Chromium, then drive the CDP pipe
/// transport to produce a fully-rendered capture (see [`crate::render`]).
fn browser_capture(
    url: &url::Url,
    formats: &[OutputFormat],
    opts: &CaptureOptions,
    deadline: &crate::limits::Deadline,
) -> Result<RawCapture> {
    if deadline.exceeded() {
        return Err(Error::Fetch(
            "capture exceeded its time budget before the browser render".to_string(),
        ));
    }
    let chromium = crate::browser::ensure_chromium()?;
    crate::render::capture(&chromium, url, formats, opts)
}

/// Build a [`RawCapture`] from a static (non-browser) fetch.
fn static_capture(page: http::FetchedPage) -> RawCapture {
    RawCapture {
        final_url: page.final_url.to_string(),
        static_html: Some(page.html),
        used_browser: false,
        ..Default::default()
    }
}

/// Map the module-local [`http::FetchError`] into the crate-wide [`Error`].
fn map_fetch_error(err: http::FetchError, url: &url::Url) -> Error {
    match err {
        http::FetchError::Status(code) => Error::HttpStatus(code, url.to_string()),
        http::FetchError::Timeout => Error::Fetch(format!("request timed out: {url}")),
        http::FetchError::Request(msg) => Error::Fetch(msg),
        http::FetchError::TooLarge(limit) => {
            Error::Fetch(format!("response exceeded the {limit}-byte budget: {url}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Debug` of `CaptureOptions` must not leak proxy credentials or session
    /// secrets — only redacted/`name`-level information (task 3.10).
    #[test]
    fn debug_redacts_proxy_and_session_secrets() {
        let opts = CaptureOptions {
            proxy: Some("http://user:hunter2@gw.local:3128".to_string()),
            session: crate::session::SessionState {
                headers: vec![("Authorization".to_string(), "Bearer SEKRET".to_string())],
                cookies: vec![("sid".to_string(), "SEKRET".to_string())],
            },
            ..Default::default()
        };
        let rendered = format!("{opts:?}");
        assert!(
            !rendered.contains("hunter2") && !rendered.contains("SEKRET"),
            "secrets leaked through Debug:\n{rendered}"
        );
        // The non-secret parts still surface for debugging.
        assert!(rendered.contains("gw.local:3128"));
        assert!(rendered.contains("Authorization") && rendered.contains("sid"));
    }
}
