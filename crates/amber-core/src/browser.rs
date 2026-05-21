//! Browser management and the (only) CDP transport. See `docs/PLAN.md` §13.
//!
//! AmberHTML drives a real, pinned Chromium over Chromium's CDP **debug pipe**
//! (`--remote-debugging-pipe`): commands and events are NUL-delimited JSON
//! exchanged over file descriptors inherited by the spawned browser (fd 3 for
//! input to the browser, fd 4 for output from it). There is **no open debugging
//! port and no WebSocket** — the pipe is reachable only by the parent process
//! that launched the browser, which is the security property we want. (A debug
//! *port* would let any local process hijack the browser.) chromiumoxide is
//! intentionally NOT used.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

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
/// is [`PipeCdp`] (CDP over the debug pipe). It is also the natural slot where a
/// WebSocket transport could be added later *if* attaching to a remote/existing
/// browser (enterprise browser pool, cloud browser) is ever supported; that is
/// out of scope today, since we always spawn our own local browser.
pub trait CdpTransport {
    /// Send a CDP command and await its result. Blocking facade; the real
    /// implementation runs async internally.
    fn send(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value>;
}

/// Hand-rolled CDP client over Chromium's debug pipe — the only transport.
/// *(Skeleton.)*
pub struct PipeCdp {
    // TODO: child process handle; the two pipe ends (we WRITE commands to the
    // browser's fd 3, and READ responses/events from the browser's fd 4); a
    // command-id counter; an id->oneshot map; and an event dispatcher.
    // Framing: each CDP message is JSON terminated by a single `\0` byte.
}

impl PipeCdp {
    /// Spawn a pinned Chromium with `--remote-debugging-pipe` and connect over
    /// the inherited pipe file descriptors.
    ///
    /// Plan: create two OS pipes and launch Chromium
    /// (`--headless=new --remote-debugging-pipe ...`) with their ends mapped to
    /// the child's **fd 3** (browser reads our commands) and **fd 4** (browser
    /// writes responses/events). Then exchange NUL-delimited JSON, correlating
    /// command `id`s to responses and routing events. No TCP port is opened.
    ///
    /// Note: fd remapping is platform-specific (Unix `dup2` via `pre_exec` or a
    /// helper crate; Windows inherited handles) — handled in the implementation.
    pub fn connect(_chromium: &Path) -> Result<Self> {
        Err(Error::NotImplemented(
            "PipeCdp::connect (CDP over --remote-debugging-pipe)",
        ))
    }
}

impl CdpTransport for PipeCdp {
    fn send(&mut self, _method: &str, _params: serde_json::Value) -> Result<serde_json::Value> {
        Err(Error::NotImplemented("PipeCdp::send"))
    }
}

/// Ensure a pinned Chrome for Testing build is present locally, downloading and
/// checksum-verifying it into the cache if needed (PLAN.md §6, §13).
///
/// Plan: resolve the pinned CfT version via the known-good-versions JSON,
/// download the platform build, verify its checksum, and cache it under
/// `~/.cache/amber-html/chromium/<rev>/`. Honors `AMBER_CHROMIUM_PATH`.
pub fn ensure_chromium() -> Result<PathBuf> {
    Err(Error::NotImplemented(
        "ensure_chromium (Chrome for Testing fetcher)",
    ))
}
