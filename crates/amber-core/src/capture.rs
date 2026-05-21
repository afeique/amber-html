//! The capture pipeline: turn a URL into a format-agnostic [`RawCapture`].
//! See `docs/PLAN.md` §7.

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
    /// Optional CSS selector / JS predicate to wait for (forces a browser).
    pub wait_for: Option<String>,
    /// Override the minimum static content length treated as sufficient.
    pub min_content: Option<usize>,
}

/// The raw, format-agnostic product of a single capture pass. Output emitters
/// (markdown, readable, ...) derive concrete results from this. Fields grow as
/// the pipeline is implemented.
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
    // TODO(next): mhtml bytes, screenshot bytes, network log (for WARC),
    //             accessibility tree, response metadata, ...
}

/// Run the capture pipeline (PLAN.md §7):
/// output gate → HTTP fetch → sufficiency → escalate → (settle → render).
///
/// The static (HTTP-only) tier is implemented. The browser path — Chrome for
/// Testing fetcher + the hand-rolled CDP client — is the next milestone; any
/// branch that would require it returns [`Error::NotImplemented`] for now.
pub(crate) fn run(
    url: &url::Url,
    formats: &[OutputFormat],
    opts: &CaptureOptions,
) -> Result<RawCapture> {
    // Step 1 — output gate: some outputs (or `--render always`) require a
    // browser up front, so we don't bother with a cheap fetch.
    if fetch::browser_required_upfront(formats, opts.render) {
        return Err(Error::NotImplemented(
            "browser render path (Chrome for Testing fetcher + CDP client)",
        ));
    }

    // Step 2 — cheap HTTP-first fetch.
    let page = match http::fetch(url) {
        Ok(page) => page,
        Err(err) => {
            return match opts.render {
                // User forbade the browser: the cheap tier failing is terminal.
                RenderMode::Never => Err(map_fetch_error(err, url)),
                // Otherwise we'd escalate to the browser (not built yet).
                _ => Err(Error::NotImplemented(
                    "browser escalation after HTTP fetch failure",
                )),
            };
        }
    };

    // The static path can only reason about HTML responses.
    if !http::content_type_is_html(page.content_type.as_deref()) {
        return match opts.render {
            RenderMode::Never => Err(Error::Fetch(format!(
                "non-HTML response ({}) and --render never",
                page.content_type.as_deref().unwrap_or("unknown content-type")
            ))),
            _ => Err(Error::NotImplemented(
                "browser escalation for non-HTML static response",
            )),
        };
    }

    // Step 3 — sufficiency: is the static HTML enough, or must we render?
    let floor = opts.min_content.unwrap_or(detect::CONTENT_FLOOR);
    match detect::assess(&page.html, floor) {
        // Clearly enough content: use what we fetched, no browser.
        detect::Sufficiency::Static => Ok(static_capture(page)),
        // `--render never`: best-effort with whatever static HTML we have.
        _ if opts.render == RenderMode::Never => Ok(static_capture(page)),
        // Insufficient/ambiguous in auto mode → escalate (correctness bias).
        detect::Sufficiency::NeedsBrowser | detect::Sufficiency::Uncertain => Err(
            Error::NotImplemented("browser escalation (static HTML insufficient)"),
        ),
    }
}

/// Build a [`RawCapture`] from a static (non-browser) fetch.
fn static_capture(page: http::FetchedPage) -> RawCapture {
    RawCapture {
        final_url: page.final_url.to_string(),
        static_html: Some(page.html),
        rendered_html: None,
        used_browser: false,
    }
}

/// Map the module-local [`http::FetchError`] into the crate-wide [`Error`],
/// preserving the URL for context.
fn map_fetch_error(err: http::FetchError, url: &url::Url) -> Error {
    match err {
        http::FetchError::Status(code) => Error::HttpStatus(code, url.to_string()),
        http::FetchError::Timeout => Error::Fetch(format!("request timed out: {url}")),
        http::FetchError::Request(msg) => Error::Fetch(msg),
    }
}
