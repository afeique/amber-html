//! Node.js bindings for AmberHTML via napi-rs. See `Plans.md` (task 6.3).
//!
//! Exposes a small capture surface as a Node N-API addon: given a URL, return
//! clean Markdown or readable text. napi-rs camelCases the exports, so these
//! are `captureMarkdown(url)` / `captureReadable(url)` in JavaScript. Build the
//! addon with `napi build` (or `cargo build` + rename the cdylib to `.node`).

use amber_core::{snapshot, CaptureOptions, OutputFormat};
use napi_derive::napi;

/// Capture `url` and return its clean Markdown.
#[napi]
pub fn capture_markdown(url: String) -> napi::Result<String> {
    capture(&url, OutputFormat::Markdown)
}

/// Capture `url` and return its readable plain text (main content).
#[napi]
pub fn capture_readable(url: String) -> napi::Result<String> {
    capture(&url, OutputFormat::Readable)
}

fn capture(url: &str, format: OutputFormat) -> napi::Result<String> {
    let to_err = |e: amber_core::Error| napi::Error::from_reason(e.to_string());
    let snap = snapshot(url, &[format], CaptureOptions::default()).map_err(to_err)?;
    let bytes = snap.render(format).map_err(to_err)?;
    String::from_utf8(bytes).map_err(|e| napi::Error::from_reason(e.to_string()))
}
