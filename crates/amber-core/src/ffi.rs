//! UniFFI bindings facade. See `Plans.md` (task 6.1).
//!
//! AmberHTML's full Rust API uses generics (e.g. the `LlmClient` trait) and
//! `Vec<u8>` returns that don't map cleanly across an FFI boundary. This module
//! exposes a small, idiomatic capture surface that UniFFI can project into
//! Python (and Swift/Kotlin/Ruby): given a URL, return Markdown or readable
//! text. The crate is built as a `cdylib`; run `uniffi-bindgen` against it to
//! generate the foreign-language module.

use crate::{snapshot, CaptureOptions, OutputFormat};

/// Errors surfaced across the FFI boundary (a flat message keeps it portable).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CaptureError {
    #[error("capture failed: {0}")]
    Failed(String),
}

/// Capture `url` and return its clean Markdown.
#[uniffi::export]
pub fn capture_markdown(url: String) -> Result<String, CaptureError> {
    capture_text(&url, OutputFormat::Markdown)
}

/// Capture `url` and return its readable plain text (main content).
#[uniffi::export]
pub fn capture_readable(url: String) -> Result<String, CaptureError> {
    capture_text(&url, OutputFormat::Readable)
}

fn capture_text(url: &str, format: OutputFormat) -> Result<String, CaptureError> {
    let fail = |e: crate::Error| CaptureError::Failed(e.to_string());
    let snap = snapshot(url, &[format], CaptureOptions::default()).map_err(fail)?;
    let bytes = snap.render(format).map_err(fail)?;
    String::from_utf8(bytes).map_err(|e| CaptureError::Failed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_error_displays_message() {
        let e = CaptureError::Failed("boom".to_string());
        assert!(e.to_string().contains("boom"));
    }

    #[test]
    fn capture_markdown_rejects_bad_url() {
        // The FFI facade surfaces core errors as CaptureError, never panics.
        let err = capture_markdown("not a url".to_string()).unwrap_err();
        assert!(matches!(err, CaptureError::Failed(_)));
    }
}
