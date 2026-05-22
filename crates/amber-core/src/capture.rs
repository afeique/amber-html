//! The capture pipeline: turn a URL into a format-agnostic [`RawCapture`].
//! See `Plans.md`.

use crate::browser::SettlePolicy;
use crate::error::{Error, Result};
use crate::fetch::{self, RenderMode};
use crate::output::OutputFormat;
use crate::{detect, http};

/// Options controlling a capture pass.
#[derive(Debug, Clone, Default)]
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
    // Step 1 — output gate: some outputs (or `--render always`) require a
    // browser up front, so we don't bother with a cheap fetch.
    if fetch::browser_required_upfront(formats, opts.render) {
        tracing::debug!("output gate: browser required up front");
        return browser_capture(url, formats, opts);
    }

    // Step 2 — cheap HTTP-first fetch (carrying any auth session state).
    let page = match http::fetch_with_session(url, &opts.session) {
        Ok(page) => page,
        Err(err) => {
            return match opts.render {
                // User forbade the browser: the cheap tier failing is terminal.
                RenderMode::Never => Err(map_fetch_error(err, url)),
                // Otherwise escalate to the browser.
                _ => browser_capture(url, formats, opts),
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
            _ => browser_capture(url, formats, opts),
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
            browser_capture(url, formats, opts)
        }
    }
}

/// Render via a real browser: ensure a pinned Chromium, then drive the CDP pipe
/// transport to produce a fully-rendered capture (see [`crate::render`]).
fn browser_capture(
    url: &url::Url,
    formats: &[OutputFormat],
    opts: &CaptureOptions,
) -> Result<RawCapture> {
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
    }
}
