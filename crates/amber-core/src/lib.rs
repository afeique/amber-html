//! AmberHTML core — the local-first web-capture engine.
//!
//! Renders a page in a real local browser *only when needed*, then emits the
//! requested representations from a single pass. The public API is intentionally
//! blocking (async lives inside). See `docs/PLAN.md` for the full design.

// Scaffold: several items are defined ahead of their implementations.
#![allow(dead_code)]

pub mod browser;
pub mod capture;
pub mod detect;
pub mod error;
pub mod fetch;
pub mod naming;
pub mod output;

pub use capture::{CaptureOptions, RawCapture};
pub use error::{Error, Result};
pub use fetch::RenderMode;
pub use output::OutputFormat;

use std::path::{Path, PathBuf};
use url::Url;

/// Capture `url`, returning a [`Snapshot`] that can emit the requested formats.
///
/// `formats` must be non-empty — there is no default output (PLAN.md §8). The
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
        let _ = (&self.raw, format); // TODO(phase1): dispatch to output emitters.
        Err(Error::NotImplemented("Snapshot::render (output emitters)"))
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
}
