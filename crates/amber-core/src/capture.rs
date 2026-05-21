//! The capture pipeline: turn a URL into a format-agnostic [`RawCapture`].
//! See `docs/PLAN.md` §7.

use crate::browser::SettlePolicy;
use crate::error::{Error, Result};
use crate::fetch::RenderMode;
use crate::output::OutputFormat;

/// Options controlling a capture pass.
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    /// Browser rendering policy (auto / always / never).
    pub render: RenderMode,
    /// When/how to consider the page "settled" before capture.
    pub settle: SettlePolicy,
    /// Optional CSS selector / JS predicate to wait for (forces a browser).
    pub wait_for: Option<String>,
    /// Override the minimum static content length treated as sufficient.
    pub min_content: Option<usize>,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            render: RenderMode::default(),
            settle: SettlePolicy::default(),
            wait_for: None,
            min_content: None,
        }
    }
}

/// The raw, format-agnostic product of a single capture pass. Output emitters
/// (markdown, mhtml, screenshot, ...) derive concrete results from this.
/// Fields grow as the pipeline is implemented.
#[derive(Debug, Default)]
pub struct RawCapture {
    /// The final URL after redirects.
    pub final_url: String,
    /// Raw static HTML from the cheap HTTP fetch, if taken.
    pub static_html: Option<String>,
    /// Rendered HTML / serialized DOM from the browser, if rendered.
    pub rendered_html: Option<String>,
    /// Whether a browser was used.
    pub used_browser: bool,
    // TODO(phase1+): mhtml bytes, screenshot bytes, network log (for WARC),
    //                accessibility tree, response metadata, ...
}

/// Run the capture pipeline (PLAN.md §7):
/// output gate → HTTP fetch → sufficiency → escalate → settle → capture.
///
/// *(Skeleton — the static fast-path and browser path are implemented next.)*
pub(crate) fn run(
    url: &url::Url,
    formats: &[OutputFormat],
    opts: &CaptureOptions,
) -> Result<RawCapture> {
    let _ = (url, formats, opts);
    Err(Error::NotImplemented(
        "capture::run (HTTP fetch + browser render pipeline)",
    ))
}
