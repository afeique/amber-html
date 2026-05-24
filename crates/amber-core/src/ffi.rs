//! UniFFI bindings facade. See `Plans.md` (task 6.1).
//!
//! AmberHTML's full Rust API uses generics (e.g. the `LlmClient` trait) that
//! don't map across an FFI boundary. This module exposes the idiomatic capture
//! surface that UniFFI *can* project into Python/Ruby/Swift/Kotlin: pick any
//! [`OutputFormat`], get back bytes, UTF-8 text, or a written file. The crate is
//! built as a `cdylib`; run `uniffi-bindgen` against it to generate the
//! foreign-language module.

use std::path::Path;

use crate::{snapshot, CaptureOptions, OutputFormat};

/// Errors surfaced across the FFI boundary (a flat message keeps it portable).
///
/// The single variant carries a *named* field (`reason`) rather than a tuple —
/// UniFFI's Ruby backend only generates valid code for named fields.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CaptureError {
    #[error("capture failed: {reason}")]
    Failed { reason: String },
}

impl CaptureError {
    fn of(e: impl std::fmt::Display) -> Self {
        CaptureError::Failed {
            reason: e.to_string(),
        }
    }
}

/// Capture `url` and return `format` as raw bytes.
///
/// Works for every format — text ones (Markdown/Readable/Html) come back as
/// UTF-8 bytes, binary ones (Screenshot/Pdf/Mhtml/Warc/Wacz) as their encoded
/// payload. Bindings see this as `bytes` (Python), `String`/`ByteArray` (Ruby/
/// Kotlin), or `Data` (Swift).
#[uniffi::export]
pub fn capture(url: String, format: OutputFormat) -> Result<Vec<u8>, CaptureError> {
    let snap = snapshot(&url, &[format], CaptureOptions::default()).map_err(CaptureError::of)?;
    snap.render(format).map_err(CaptureError::of)
}

/// Capture `url` and return `format` as UTF-8 text.
///
/// Intended for the text formats; binary formats (e.g. Screenshot/Pdf) error
/// rather than return mojibake — use [`capture`] for those.
#[uniffi::export]
pub fn capture_text(url: String, format: OutputFormat) -> Result<String, CaptureError> {
    let bytes = capture(url, format)?;
    String::from_utf8(bytes).map_err(CaptureError::of)
}

/// Capture `url`, write `format` into `dir`, and return the written path.
///
/// `name` is the file stem (the extension is chosen by the format); when
/// `None`, a `<safe-url> <date> <time>` name is used. `dir` is created if
/// missing. This is the convenient path for binary formats.
#[uniffi::export]
pub fn save(
    url: String,
    format: OutputFormat,
    dir: String,
    name: Option<String>,
) -> Result<String, CaptureError> {
    let snap = snapshot(&url, &[format], CaptureOptions::default()).map_err(CaptureError::of)?;
    let path = snap
        .save(format, Path::new(&dir), name.as_deref())
        .map_err(CaptureError::of)?;
    Ok(path.display().to_string())
}

/// Capture `url` and return its clean Markdown (convenience over [`capture_text`]).
#[uniffi::export]
pub fn capture_markdown(url: String) -> Result<String, CaptureError> {
    capture_text(url, OutputFormat::Markdown)
}

/// Capture `url` and return its readable plain text (convenience over [`capture_text`]).
#[uniffi::export]
pub fn capture_readable(url: String) -> Result<String, CaptureError> {
    capture_text(url, OutputFormat::Readable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_error_displays_message() {
        let e = CaptureError::Failed {
            reason: "boom".to_string(),
        };
        assert!(e.to_string().contains("boom"));
    }

    #[test]
    fn capture_markdown_rejects_bad_url() {
        // The FFI facade surfaces core errors as CaptureError, never panics.
        let err = capture_markdown("not a url".to_string()).unwrap_err();
        assert!(matches!(err, CaptureError::Failed { .. }));
    }

    #[test]
    fn capture_bytes_rejects_bad_url() {
        let err = capture("not a url".to_string(), OutputFormat::Screenshot).unwrap_err();
        assert!(matches!(err, CaptureError::Failed { .. }));
    }

    #[test]
    fn save_rejects_bad_url() {
        let err = save(
            "not a url".to_string(),
            OutputFormat::Pdf,
            "/tmp".to_string(),
            Some("amber_ffi_test".to_string()),
        )
        .unwrap_err();
        assert!(matches!(err, CaptureError::Failed { .. }));
    }
}
