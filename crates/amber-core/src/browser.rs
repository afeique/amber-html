//! Browser management and the (only) CDP transport. See `docs/PLAN.md` §13.
//!
//! AmberHTML drives a real, pinned Chromium over Chromium's CDP **debug pipe**
//! (`--remote-debugging-pipe`): commands and events are NUL-delimited JSON
//! exchanged over file descriptors inherited by the spawned browser (fd 3 for
//! input to the browser, fd 4 for output). There is **no open debugging port
//! and no WebSocket** — the pipe is reachable only by the parent process that
//! launched the browser. chromiumoxide is intentionally NOT used.
//!
//! - The pinned-browser fetcher lives in [`crate::chromium`]; [`ensure_chromium`]
//!   here is the crate-level entry point that maps its errors into [`Error`].
//! - The pipe transport implementation lives in [`crate::cdp`]
//!   ([`crate::cdp::PipeCdp`], which implements [`CdpTransport`]).

use crate::error::{Error, Result};
use std::path::PathBuf;

/// Policy for deciding a page is "settled" before capture (PLAN.md §7).
#[derive(Debug, Clone)]
pub struct SettlePolicy {
    /// Wait for the `load` lifecycle event.
    pub wait_load: bool,
    /// Wait for network to go (nearly) idle.
    pub network_idle: bool,
    /// Wait for `document.fonts.ready`.
    pub fonts_ready: bool,
    /// Extra grace period after idle, in milliseconds.
    pub settle_delay_ms: u64,
    /// Auto-scroll the page to trigger lazy-loaded content.
    pub auto_scroll: bool,
}

impl Default for SettlePolicy {
    fn default() -> Self {
        Self {
            wait_load: true,
            network_idle: true,
            fonts_ready: true,
            settle_delay_ms: 200,
            auto_scroll: false,
        }
    }
}

/// Abstraction over a CDP connection.
///
/// This exists ONLY as a seam for test mocking — the sole real implementation
/// is [`crate::cdp::PipeCdp`] (CDP over the debug pipe). It is also the natural
/// slot where a WebSocket transport could be added later *if* attaching to a
/// remote/existing browser is ever supported; out of scope today.
pub trait CdpTransport {
    /// Send a CDP command and await its result. Blocking facade; the real
    /// implementation runs a background reader internally.
    fn send(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value>;
}

/// Ensure a pinned Chrome for Testing build is present locally, returning the
/// path to its executable (downloading + caching on first use).
///
/// Delegates to [`crate::chromium`] and maps its module-local error into the
/// crate-wide [`Error`].
pub fn ensure_chromium() -> Result<PathBuf> {
    crate::chromium::ensure_chromium().map_err(|e| Error::Browser(e.to_string()))
}
