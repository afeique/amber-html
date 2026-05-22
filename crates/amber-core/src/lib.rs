//! AmberHTML core — the local-first web-capture engine.
//!
//! Renders a page in a real local browser *only when needed*, then emits the
//! requested representations from a single pass. The public API is intentionally
//! blocking (async lives inside). See `Plans.md` for the full design.

// Scaffold: several items are defined ahead of their implementations.
#![allow(dead_code)]

pub mod browser;
pub mod capture;
pub mod cdp;
pub mod chromium;
pub mod detect;
pub mod error;
pub mod extract;
pub mod fetch;
pub mod http;
pub mod inline;
pub mod naming;
pub mod output;
pub mod render;

pub use capture::{CaptureOptions, RawCapture};
pub use error::{Error, Result};
pub use fetch::RenderMode;
pub use output::OutputFormat;

use std::path::{Path, PathBuf};
use url::Url;

/// Capture `url`, returning a [`Snapshot`] that can emit the requested formats.
///
/// `formats` must be non-empty — there is no default output (Plans.md). The
/// requested set also configures the capture pass and whether a browser is used.
pub fn snapshot(url: &str, formats: &[OutputFormat], opts: CaptureOptions) -> Result<Snapshot> {
    if formats.is_empty() {
        return Err(Error::NoOutputSelected);
    }
    let parsed = Url::parse(url).map_err(|_| Error::InvalidUrl(url.to_string()))?;
    let raw = capture::run(&parsed, formats, &opts)?;
    Ok(Snapshot { url: parsed, raw })
}

/// The product of a capture pass; emits concrete formats on demand.
#[derive(Debug)]
pub struct Snapshot {
    url: Url,
    raw: RawCapture,
}

impl Snapshot {
    /// Render a single format to bytes.
    pub fn render(&self, format: OutputFormat) -> Result<Vec<u8>> {
        // Prefer browser-rendered HTML; fall back to the static fetch.
        let html = self
            .raw
            .rendered_html
            .as_deref()
            .or(self.raw.static_html.as_deref());

        match format {
            OutputFormat::Markdown => {
                let html = html.ok_or(Error::NotImplemented("no captured HTML for Markdown"))?;
                Ok(extract::to_markdown(html).into_bytes())
            }
            OutputFormat::Readable => {
                let html =
                    html.ok_or(Error::NotImplemented("no captured HTML for readable text"))?;
                Ok(extract::to_readable(html).into_bytes())
            }
            OutputFormat::Mhtml => self
                .raw
                .mhtml
                .clone()
                .map(String::into_bytes)
                .ok_or_else(|| Error::Browser("MHTML was not captured".into())),
            OutputFormat::Screenshot => self
                .raw
                .screenshot_png
                .clone()
                .ok_or_else(|| Error::Browser("screenshot was not captured".into())),
            OutputFormat::Pdf => self
                .raw
                .pdf
                .clone()
                .ok_or_else(|| Error::Browser("PDF was not captured".into())),
            OutputFormat::Html => {
                // Single-file HTML is the MHTML bundle flattened into one
                // self-contained document (subresources → `data:` URIs).
                let mhtml = self.raw.mhtml.as_deref().ok_or_else(|| {
                    Error::Browser("single-file HTML requires an MHTML capture".into())
                })?;
                Ok(inline::mhtml_to_single_file_html(mhtml).into_bytes())
            }
            OutputFormat::Warc => Err(Error::NotImplemented("WARC emitter")),
            OutputFormat::Wacz => Err(Error::NotImplemented("WACZ emitter")),
        }
    }

    /// Write `format` into `dir` using `name` (or the default URL+datetime name).
    /// Creates `dir` if missing and returns the written path.
    pub fn save(&self, format: OutputFormat, dir: &Path, name: Option<&str>) -> Result<PathBuf> {
        let base = match name {
            Some(n) => n.to_string(),
            None => naming::default_name(&self.url, chrono::Local::now()),
        };
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{base}.{}", format.extension()));
        std::fs::write(&path, self.render(format)?)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_formats_is_rejected() {
        let err = snapshot("https://example.com", &[], CaptureOptions::default()).unwrap_err();
        assert!(matches!(err, Error::NoOutputSelected));
    }

    #[test]
    fn bad_url_is_rejected() {
        let err = snapshot("not a url", &[OutputFormat::Markdown], CaptureOptions::default())
            .unwrap_err();
        assert!(matches!(err, Error::InvalidUrl(_)));
    }

    /// Build a `Snapshot` directly from a `RawCapture` for emitter unit tests
    /// (bypasses the live capture pipeline).
    fn snapshot_from(raw: RawCapture) -> Snapshot {
        Snapshot {
            url: Url::parse("https://ex.com/").unwrap(),
            raw,
        }
    }

    /// `render(Html)` flattens the captured MHTML into single-file HTML by
    /// delegating to `inline::mhtml_to_single_file_html`.
    #[test]
    fn html_emitter_inlines_captured_mhtml() {
        let mhtml = concat!(
            "Content-Type: multipart/related; boundary=\"B\"; type=\"text/html\"\r\n",
            "Content-Location: https://ex.com/\r\n",
            "\r\n",
            "--B\r\n",
            "Content-Type: text/html\r\n",
            "Content-Transfer-Encoding: quoted-printable\r\n",
            "Content-Location: https://ex.com/\r\n",
            "\r\n",
            "<html><body><p>Hello Amber</p></body></html>\r\n",
            "--B--\r\n",
        );
        let snap = snapshot_from(RawCapture {
            mhtml: Some(mhtml.to_string()),
            ..Default::default()
        });

        let html = snap.render(OutputFormat::Html).expect("Html should be emitted");
        let html = String::from_utf8(html).expect("output is UTF-8");

        // The emitter is wired to the inliner (same bytes) and preserves body text.
        assert_eq!(html, inline::mhtml_to_single_file_html(mhtml));
        assert!(
            html.contains("Hello Amber"),
            "single-file HTML should keep the body text:\n{html}"
        );
    }

    /// Without a captured MHTML there is nothing to flatten — a clear error,
    /// not a panic.
    #[test]
    fn html_emitter_without_mhtml_errors() {
        let snap = snapshot_from(RawCapture::default());
        assert!(matches!(
            snap.render(OutputFormat::Html),
            Err(Error::Browser(_))
        ));
    }
}
