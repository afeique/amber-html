//! Node.js bindings for AmberHTML via napi-rs. See `Plans.md` (tasks 6.3, 10.2).
//!
//! Exposes the full capture surface as a Node N-API addon. napi-rs camelCases
//! exports, so in JavaScript these are:
//!
//! * `capture(url, format)` → `Buffer` (any format, as raw bytes)
//! * `captureText(url, format)` → `string` (text formats)
//! * `save(url, format, dir, name?)` → `string` (written path)
//! * `captureMarkdown(url)` / `captureReadable(url)` → `string` (convenience)
//! * `snapshot(url, formats)` → `Snapshot` — **capture once, emit many**: the
//!   returned object renders/saves any requested format with no re-capture.
//! * `Format` — the output-format enum (`Format.Markdown`, `Format.Pdf`, …).
//!
//! Build the addon with `napi build` (or `cargo build` + rename the cdylib to
//! `.node`).

use amber_core::{snapshot as core_snapshot, CaptureOptions, OutputFormat};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

/// Output formats, mirroring `amber_core::OutputFormat` (Plans.md).
#[napi]
pub enum Format {
    Html,
    Mhtml,
    Markdown,
    Readable,
    Warc,
    Wacz,
    Screenshot,
    Pdf,
}

impl From<Format> for OutputFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Html => OutputFormat::Html,
            Format::Mhtml => OutputFormat::Mhtml,
            Format::Markdown => OutputFormat::Markdown,
            Format::Readable => OutputFormat::Readable,
            Format::Warc => OutputFormat::Warc,
            Format::Wacz => OutputFormat::Wacz,
            Format::Screenshot => OutputFormat::Screenshot,
            Format::Pdf => OutputFormat::Pdf,
        }
    }
}

/// Map any displayable error (core or UTF-8) to a JS exception.
fn err(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// Capture `url` and return `format` as raw bytes (a Node `Buffer`). Works for
/// every format — text ones come back as UTF-8 bytes, binary ones (screenshot/
/// PDF/MHTML/WARC/WACZ) as their encoded payload.
#[napi]
pub fn capture(url: String, format: Format) -> napi::Result<Buffer> {
    let format = format.into();
    let snap = core_snapshot(&url, &[format], CaptureOptions::default()).map_err(err)?;
    Ok(Buffer::from(snap.render(format).map_err(err)?))
}

/// Capture `url` and return `format` as UTF-8 text (for the text formats; binary
/// formats error rather than return mojibake — use [`capture`] for those).
#[napi]
pub fn capture_text(url: String, format: Format) -> napi::Result<String> {
    let format = format.into();
    let snap = core_snapshot(&url, &[format], CaptureOptions::default()).map_err(err)?;
    let bytes = snap.render(format).map_err(err)?;
    String::from_utf8(bytes).map_err(err)
}

/// Capture `url`, write `format` into `dir`, and return the written path. `name`
/// is the file stem (extension chosen by the format) or `null`/`undefined` for a
/// default name; `dir` is created if missing.
#[napi]
pub fn save(
    url: String,
    format: Format,
    dir: String,
    name: Option<String>,
) -> napi::Result<String> {
    let format = format.into();
    let snap = core_snapshot(&url, &[format], CaptureOptions::default()).map_err(err)?;
    let path = snap
        .save(format, std::path::Path::new(&dir), name.as_deref())
        .map_err(err)?;
    Ok(path.display().to_string())
}

/// Capture `url` and return its clean Markdown (convenience over [`capture_text`]).
#[napi]
pub fn capture_markdown(url: String) -> napi::Result<String> {
    capture_text(url, Format::Markdown)
}

/// Capture `url` and return its readable plain text (convenience over [`capture_text`]).
#[napi]
pub fn capture_readable(url: String) -> napi::Result<String> {
    capture_text(url, Format::Readable)
}

/// A captured page, reusable across many output formats (Plans.md 10.1/10.2).
///
/// [`snapshot`] runs the capture pipeline **once**; this object then renders or
/// saves any requested format with no re-fetch and no re-render — capturing
/// three formats costs one browser pass, not three.
#[napi]
pub struct Snapshot {
    inner: amber_core::Snapshot,
}

#[napi]
impl Snapshot {
    /// Render one `format` to raw bytes (a Node `Buffer`).
    #[napi]
    pub fn render(&self, format: Format) -> napi::Result<Buffer> {
        Ok(Buffer::from(self.inner.render(format.into()).map_err(err)?))
    }

    /// Render one (text) `format` to UTF-8 text.
    #[napi]
    pub fn text(&self, format: Format) -> napi::Result<String> {
        let bytes = self.inner.render(format.into()).map_err(err)?;
        String::from_utf8(bytes).map_err(err)
    }

    /// Write one `format` into `dir`; returns the written path.
    #[napi]
    pub fn save(&self, format: Format, dir: String, name: Option<String>) -> napi::Result<String> {
        let path = self
            .inner
            .save(format.into(), std::path::Path::new(&dir), name.as_deref())
            .map_err(err)?;
        Ok(path.display().to_string())
    }

    /// Convenience: this page's clean Markdown.
    #[napi]
    pub fn markdown(&self) -> napi::Result<String> {
        self.text(Format::Markdown)
    }

    /// Convenience: this page's readable plain text.
    #[napi]
    pub fn readable(&self) -> napi::Result<String> {
        self.text(Format::Readable)
    }
}

/// Capture `url` **once** for `formats`, returning a reusable [`Snapshot`].
/// `formats` must be non-empty — it configures the pass and the browser-vs-
/// static decision (Plans.md).
#[napi]
pub fn snapshot(url: String, formats: Vec<Format>) -> napi::Result<Snapshot> {
    let formats: Vec<OutputFormat> = formats.into_iter().map(Into::into).collect();
    let inner = core_snapshot(&url, &formats, CaptureOptions::default()).map_err(err)?;
    Ok(Snapshot { inner })
}
