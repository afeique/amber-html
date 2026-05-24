//! UniFFI bindings facade. See `Plans.md` (task 6.1).
//!
//! AmberHTML's full Rust API uses generics (e.g. the `LlmClient` trait) that
//! don't map across an FFI boundary. This module exposes the idiomatic capture
//! surface that UniFFI *can* project into Python/Ruby/Swift/Kotlin: pick any
//! [`OutputFormat`], get back bytes, UTF-8 text, or a written file. The crate is
//! built as a `cdylib`; run `uniffi-bindgen` against it to generate the
//! foreign-language module.

use std::path::Path;
use std::sync::Arc;

use crate::{snapshot as core_snapshot, CaptureOptions, OutputFormat};

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
    let snap =
        core_snapshot(&url, &[format], CaptureOptions::default()).map_err(CaptureError::of)?;
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
    let snap =
        core_snapshot(&url, &[format], CaptureOptions::default()).map_err(CaptureError::of)?;
    let path = snap
        .save(format, Path::new(&dir), name.as_deref())
        .map_err(CaptureError::of)?;
    Ok(path.display().to_string())
}

/// A captured page, reusable across many output formats (Plans.md 10.1).
///
/// [`snapshot`] runs the capture pipeline **once**; the returned object then
/// renders or saves any of the requested formats with no re-fetch and no
/// re-render. This is the engine's "render once, emit everything" promise,
/// exposed across the FFI — capturing three formats costs one browser pass, not
/// three. Bindings see it as an object: `snap.markdown()`, `snap.save(...)`, ….
#[derive(Debug, uniffi::Object)]
pub struct Snapshot {
    inner: crate::Snapshot,
}

#[uniffi::export]
impl Snapshot {
    /// Render one `format` to raw bytes — text formats as UTF-8 bytes, binary
    /// ones (Screenshot/Pdf/Mhtml/Warc/Wacz) as their encoded payload.
    pub fn render(&self, format: OutputFormat) -> Result<Vec<u8>, CaptureError> {
        self.inner.render(format).map_err(CaptureError::of)
    }

    /// Render one (text) `format` to UTF-8 text; binary formats error rather
    /// than return mojibake.
    pub fn text(&self, format: OutputFormat) -> Result<String, CaptureError> {
        let bytes = self.render(format)?;
        String::from_utf8(bytes).map_err(CaptureError::of)
    }

    /// Write one `format` into `dir` and return the written path. `name` is the
    /// file stem (extension chosen by the format), or a `<safe-url> <date>
    /// <time>` default when `None`; `dir` is created if missing.
    pub fn save(
        &self,
        format: OutputFormat,
        dir: String,
        name: Option<String>,
    ) -> Result<String, CaptureError> {
        let path = self
            .inner
            .save(format, Path::new(&dir), name.as_deref())
            .map_err(CaptureError::of)?;
        Ok(path.display().to_string())
    }

    /// Convenience: this page's clean Markdown.
    pub fn markdown(&self) -> Result<String, CaptureError> {
        self.text(OutputFormat::Markdown)
    }

    /// Convenience: this page's readable plain text.
    pub fn readable(&self) -> Result<String, CaptureError> {
        self.text(OutputFormat::Readable)
    }
}

/// Capture `url` **once** for the given `formats`, returning a reusable
/// [`Snapshot`]. `formats` must be non-empty — it configures the capture pass
/// and the browser-vs-static decision (Plans.md). Render or save any of them
/// from the returned object.
#[uniffi::export]
pub fn snapshot(url: String, formats: Vec<OutputFormat>) -> Result<Arc<Snapshot>, CaptureError> {
    let inner =
        core_snapshot(&url, &formats, CaptureOptions::default()).map_err(CaptureError::of)?;
    Ok(Arc::new(Snapshot { inner }))
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

    #[test]
    fn snapshot_object_rejects_bad_url() {
        let err = snapshot("not a url".to_string(), vec![OutputFormat::Markdown]).unwrap_err();
        assert!(matches!(err, CaptureError::Failed { .. }));
    }

    #[test]
    fn snapshot_object_renders_many_formats_from_one_capture() {
        // Build the object from a known capture (no browser/network), then prove
        // it serves multiple formats and re-renders deterministically — the
        // "capture once, emit many" contract (Plans.md 10.1).
        let url = url::Url::parse("https://example.com/").unwrap();
        let raw = crate::RawCapture {
            rendered_html: Some(
                "<html><head><title>T</title></head><body><h1>Hi</h1><p>Body text here.</p></body></html>"
                    .to_string(),
            ),
            ..Default::default()
        };
        let snap = Snapshot {
            inner: crate::Snapshot::from_parts(url, raw),
        };

        let md = snap.markdown().unwrap();
        assert!(
            md.contains("Hi"),
            "markdown should contain the heading: {md:?}"
        );
        let txt = snap.readable().unwrap();
        assert!(
            txt.contains("Body text"),
            "readable should contain the body: {txt:?}"
        );
        // Re-rendering the same format from the same capture is stable.
        assert_eq!(
            snap.render(OutputFormat::Markdown).unwrap(),
            md.into_bytes()
        );
    }
}
