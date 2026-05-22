//! AmberHTML core — the local-first web-capture engine.
//!
//! Renders a page in a real local browser *only when needed*, then emits the
//! requested representations from a single pass. The public API is intentionally
//! blocking (async lives inside). See `Plans.md` for the full design.

// Scaffold: several items are defined ahead of their implementations.
#![allow(dead_code)]

// UniFFI scaffolding for the language bindings (see `ffi`).
uniffi::setup_scaffolding!();

pub mod actions;
pub mod blocking;
pub mod browser;
pub mod budget;
pub mod cache;
pub mod capture;
pub mod cdp;
pub mod chromium;
pub mod crawl;
pub mod detect;
pub mod diff;
pub mod emulation;
pub mod error;
pub mod extract;
pub mod ffi;
pub mod fetch;
pub mod http;
pub mod inline;
pub mod limits;
pub mod mcp;
pub mod meta;
pub mod metrics;
pub mod naming;
pub mod output;
pub mod provenance;
pub mod recurring;
pub mod render;
pub mod robots;
pub mod selectors;
pub mod sitemap;
pub mod store;
pub mod structured;
pub mod wacz;
pub mod warc;

pub use actions::Action;
pub use blocking::{BlockPolicy, ResourceType};
pub use budget::{
    chunk_text, estimate_cost, estimate_tokens, truncate_to_tokens, TokenAccounting,
};
pub use cache::{content_hash, Cache, CacheEntry};
pub use capture::{CaptureOptions, RawCapture};
pub use crawl::{
    crawl, crawl_incremental, crawl_incremental_with, crawl_with, CrawlLimits, CrawlScope, Frontier,
};
pub use diff::{diff_lines, LineDiff};
pub use emulation::{EmulationConfig, Viewport};
pub use error::{Error, Result};
pub use extract::dedup_text;
pub use fetch::RenderMode;
pub use limits::{Deadline, ResourceLimits};
pub use meta::PageMetadata;
pub use metrics::{Metrics, MetricsSnapshot};
pub use output::OutputFormat;
pub use provenance::{anchor_for, Provenance};
pub use recurring::{run_schedule, Cadence};
pub use robots::Robots;
pub use selectors::{select_all_text, select_first_text};
pub use sitemap::{fetch_sitemap, parse_sitemap};
pub use store::{CrawlStore, StoredPage};
pub use structured::{extract_nl, extract_structured, LlmClient};
pub use warc::{http_response_block, WarcWriter};
pub use wacz::package as package_wacz;

use std::path::{Path, PathBuf};
use url::Url;

/// Capture `url`, returning a [`Snapshot`] that can emit the requested formats.
///
/// `formats` must be non-empty — there is no default output (Plans.md). The
/// requested set also configures the capture pass and whether a browser is used.
#[tracing::instrument(level = "info", name = "snapshot", skip(opts), fields(format_count = formats.len()))]
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
    /// Extract structured page metadata (title, lang, description, canonical,
    /// OpenGraph, links) from the captured HTML.
    ///
    /// Prefers the browser-rendered DOM, falling back to the static fetch;
    /// relative URLs are resolved against the capture's URL. Returns empty
    /// metadata when no HTML was captured (e.g. a screenshot-only pass).
    pub fn metadata(&self) -> PageMetadata {
        let html = self
            .raw
            .rendered_html
            .as_deref()
            .or(self.raw.static_html.as_deref());
        match html {
            Some(html) => meta::extract(html, &self.url),
            None => PageMetadata::default(),
        }
    }

    /// Detect the page's natural language (ISO 639-3, e.g. `"eng"`) from the
    /// extracted readable text. Complements the declared `<html lang>` exposed
    /// by [`Snapshot::metadata`]. Returns `None` when no HTML was captured or no
    /// language can be confidently determined.
    pub fn detected_language(&self) -> Option<String> {
        let html = self
            .raw
            .rendered_html
            .as_deref()
            .or(self.raw.static_html.as_deref())?;
        extract::detect_language(&extract::to_readable(html))
    }

    /// The page's Markdown trimmed to at most `max_tokens`, returned with its
    /// estimated token count. The count is approximate and model-agnostic (see
    /// [`budget::estimate_tokens`]); re-measure with a model's own tokenizer if
    /// exact counts are required.
    pub fn markdown_within(&self, max_tokens: usize) -> Result<(String, usize)> {
        let bytes = self.render(OutputFormat::Markdown)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Ok(budget::truncate_to_tokens(&text, max_tokens))
    }

    /// The page's readable text trimmed to at most `max_tokens`, with its
    /// estimated token count. See [`Snapshot::markdown_within`].
    pub fn readable_within(&self, max_tokens: usize) -> Result<(String, usize)> {
        let bytes = self.render(OutputFormat::Readable)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Ok(budget::truncate_to_tokens(&text, max_tokens))
    }

    /// The page's readable text with duplicate paragraphs removed (see
    /// [`extract::dedup_text`]). Useful for pages that repeat boilerplate
    /// fragments in the main content.
    pub fn readable_deduped(&self) -> Result<String> {
        let bytes = self.render(OutputFormat::Readable)?;
        Ok(extract::dedup_text(&String::from_utf8_lossy(&bytes)))
    }

    /// Per-capture token accounting for the text renderings (Markdown and
    /// readable text). Counts are approximate (see [`budget::estimate_tokens`]);
    /// pair with [`TokenAccounting::markdown_cost`] and a caller-supplied price
    /// for cost reporting. Empty when no HTML was captured.
    pub fn token_accounting(&self) -> TokenAccounting {
        let count = |format| {
            self.render(format)
                .ok()
                .map(|b| budget::estimate_tokens(&String::from_utf8_lossy(&b)))
                .unwrap_or(0)
        };
        TokenAccounting {
            markdown: count(OutputFormat::Markdown),
            readable: count(OutputFormat::Readable),
        }
    }

    /// The captured page as readable text (browser-rendered DOM preferred,
    /// static fallback) — the input for structured extraction.
    fn readable_text(&self) -> String {
        match self
            .raw
            .rendered_html
            .as_deref()
            .or(self.raw.static_html.as_deref())
        {
            Some(html) => extract::to_readable(html),
            None => String::new(),
        }
    }

    /// Extract structured JSON from the captured page against `schema`, using
    /// the caller's own model via `client` (see [`structured::LlmClient`]). The
    /// extraction input is the page's readable text. (Plans.md task 4.1/4.3.)
    pub fn extract<C: structured::LlmClient>(
        &self,
        schema: &str,
        client: &C,
    ) -> Result<serde_json::Value> {
        structured::extract_structured(&self.readable_text(), schema, client)
    }

    /// Extract JSON answering a natural-language `instruction` about the
    /// captured page, using the caller's model via `client`. (Plans.md 4.2/4.3.)
    pub fn extract_nl<C: structured::LlmClient>(
        &self,
        instruction: &str,
        client: &C,
    ) -> Result<serde_json::Value> {
        structured::extract_nl(&self.readable_text(), instruction, client)
    }

    /// The captured accessibility tree (`Accessibility.getFullAXTree` nodes),
    /// when `CaptureOptions::accessibility` was set on a browser capture.
    /// `None` for static captures or when not requested.
    pub fn accessibility_tree(&self) -> Option<&serde_json::Value> {
        self.raw.accessibility_tree.as_ref()
    }

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

    /// `Snapshot::metadata()` exposes page metadata extracted from the captured
    /// HTML, resolving relative URLs against the capture's URL.
    #[test]
    fn snapshot_exposes_metadata_from_html() {
        let snap = snapshot_from(RawCapture {
            static_html: Some(
                r#"<html lang="en"><head><title>Hi</title>
                   <link rel="canonical" href="/canon"></head>
                   <body><a href="/p">p</a></body></html>"#
                    .to_string(),
            ),
            ..Default::default()
        });
        let m = snap.metadata();
        assert_eq!(m.title.as_deref(), Some("Hi"));
        assert_eq!(m.lang.as_deref(), Some("en"));
        assert_eq!(m.canonical.as_deref(), Some("https://ex.com/canon"));
        assert_eq!(m.links, vec!["https://ex.com/p".to_string()]);
    }

    /// With no captured HTML (e.g. a screenshot-only pass) metadata is empty,
    /// not an error.
    #[test]
    fn snapshot_metadata_empty_without_html() {
        let snap = snapshot_from(RawCapture::default());
        assert_eq!(snap.metadata(), PageMetadata::default());
    }

    /// `Snapshot::detected_language()` classifies the captured page's text.
    #[test]
    fn snapshot_detects_language_from_html() {
        let snap = snapshot_from(RawCapture {
            static_html: Some(
                "<html><body><article><p>The quick brown fox jumps over the lazy \
                 dog, and this paragraph is unmistakably written in English prose \
                 so the detector has plenty to work with.</p></article></body></html>"
                    .to_string(),
            ),
            ..Default::default()
        });
        assert_eq!(snap.detected_language().as_deref(), Some("eng"));
    }

    /// No captured HTML → no language, not an error.
    #[test]
    fn snapshot_detected_language_none_without_html() {
        let snap = snapshot_from(RawCapture::default());
        assert_eq!(snap.detected_language(), None);
    }

    /// `markdown_within` trims the captured Markdown to a token budget and
    /// reports the (approximate) count.
    #[test]
    fn snapshot_markdown_within_trims_to_budget() {
        let para = "<p>word word word word word word word word</p>".repeat(40);
        let snap = snapshot_from(RawCapture {
            static_html: Some(format!("<html><body><article>{para}</article></body></html>")),
            ..Default::default()
        });

        let full = String::from_utf8(snap.render(OutputFormat::Markdown).unwrap()).unwrap();
        let (trimmed, count) = snap.markdown_within(15).unwrap();

        assert!(count <= 15, "reported count {count} exceeds budget");
        assert!(!trimmed.is_empty(), "should keep some content");
        assert!(
            trimmed.len() < full.len(),
            "budgeted output should be shorter than the full Markdown"
        );
    }

    /// `token_accounting` reports non-zero counts for the text outputs.
    #[test]
    fn snapshot_token_accounting_counts_text_outputs() {
        let snap = snapshot_from(RawCapture {
            static_html: Some(
                "<html><body><article><p>one two three four five six seven eight nine \
                 ten eleven twelve</p></article></body></html>"
                    .to_string(),
            ),
            ..Default::default()
        });
        let acct = snap.token_accounting();
        assert!(acct.markdown > 0, "markdown tokens: {}", acct.markdown);
        assert!(acct.readable > 0, "readable tokens: {}", acct.readable);
    }

    /// No captured HTML → zeroed accounting, not an error.
    #[test]
    fn snapshot_token_accounting_zero_without_html() {
        let snap = snapshot_from(RawCapture::default());
        assert_eq!(snap.token_accounting(), TokenAccounting::default());
    }

    /// `Snapshot::extract` runs structured extraction over the captured text
    /// using the caller's model client.
    #[test]
    fn snapshot_extract_runs_structured_extraction() {
        struct Mock;
        impl structured::LlmClient for Mock {
            fn complete(&self, _prompt: &str) -> Result<String> {
                Ok(r#"{"title":"Amber","ok":true}"#.to_string())
            }
        }
        let snap = snapshot_from(RawCapture {
            static_html: Some(
                "<html><body><article><p>An article about capture engines.</p>\
                 </article></body></html>"
                    .to_string(),
            ),
            ..Default::default()
        });
        let value = snap.extract(r#"{"type":"object"}"#, &Mock).unwrap();
        assert_eq!(value["title"], "Amber");
        assert_eq!(value["ok"], true);
    }

    /// `readable_deduped` returns the readable text with duplicate paragraphs
    /// removed (delegates to `extract::dedup_text`).
    #[test]
    fn snapshot_readable_deduped_drops_duplicate_paragraphs() {
        let snap = snapshot_from(RawCapture {
            static_html: Some(
                "<html><body><article>\
                 <p>Alpha beta gamma delta epsilon.</p>\
                 <p>Alpha beta gamma delta epsilon.</p>\
                 <p>A distinct unique closing sentence.</p>\
                 </article></body></html>"
                    .to_string(),
            ),
            ..Default::default()
        });
        let raw = String::from_utf8(snap.render(OutputFormat::Readable).unwrap()).unwrap();
        let deduped = snap.readable_deduped().unwrap();
        assert_eq!(deduped, dedup_text(&raw));
        assert!(deduped.len() <= raw.len());
    }
}
