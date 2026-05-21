//! Browser management and the (only) CDP transport. See `docs/PLAN.md` §13.
//!
//! AmberHTML drives a real, pinned Chromium over a hand-rolled WebSocket CDP
//! client. chromiumoxide is intentionally NOT used.

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
/// This exists ONLY as a seam for test mocking — there is exactly one real
/// implementation, [`NativeCdp`]. We do not support swapping transports.
pub trait CdpTransport {
    /// Send a CDP command and await its result. Blocking facade; the real
    /// implementation runs async internally.
    fn send(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value>;
}

/// Hand-rolled CDP client over a WebSocket — the only transport. *(Skeleton.)*
pub struct NativeCdp {
    // TODO: ws stream, command-id counter, id->oneshot map, event dispatcher.
}

impl NativeCdp {
    /// Spawn (or attach to) Chromium and connect to its CDP WebSocket endpoint.
    ///
    /// Plan: launch with `--remote-debugging-port`, `GET /json/version` for the
    /// `webSocketDebuggerUrl`, connect, then drive commands with id↔response
    /// correlation and an event dispatcher.
    pub fn connect(_chromium: &Path) -> Result<Self> {
        Err(Error::NotImplemented(
            "NativeCdp::connect (hand-rolled CDP WebSocket client)",
        ))
    }
}

impl CdpTransport for NativeCdp {
    fn send(&mut self, _method: &str, _params: serde_json::Value) -> Result<serde_json::Value> {
        Err(Error::NotImplemented("NativeCdp::send"))
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
