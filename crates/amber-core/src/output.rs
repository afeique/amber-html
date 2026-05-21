//! The set of representations AmberHTML can emit from one capture pass.
//! See `docs/PLAN.md` §8.

/// A single output representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    /// Single-file inlined HTML (`data:` URIs + inlined `<style>`).
    Html,
    /// MHTML bundle (`Page.captureSnapshot`).
    Mhtml,
    /// Clean, LLM-optimized Markdown.
    Markdown,
    /// Readable plain text (main-content extraction).
    Readable,
    /// WARC archive.
    Warc,
    /// WACZ archive (replayable).
    Wacz,
    /// Full-page screenshot (PNG).
    Screenshot,
    /// PDF.
    Pdf,
}

impl OutputFormat {
    /// Every supported format, in CLI order.
    pub const ALL: [OutputFormat; 8] = [
        OutputFormat::Html,
        OutputFormat::Mhtml,
        OutputFormat::Markdown,
        OutputFormat::Readable,
        OutputFormat::Warc,
        OutputFormat::Wacz,
        OutputFormat::Screenshot,
        OutputFormat::Pdf,
    ];

    /// File extension (without the leading dot).
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Html => "html",
            OutputFormat::Mhtml => "mhtml",
            OutputFormat::Markdown => "md",
            OutputFormat::Readable => "txt",
            OutputFormat::Warc => "warc",
            OutputFormat::Wacz => "wacz",
            OutputFormat::Screenshot => "png",
            OutputFormat::Pdf => "pdf",
        }
    }

    /// Whether producing this format inherently requires a real browser, versus
    /// possibly being satisfiable from a static HTTP fetch (PLAN.md §7, §8).
    pub fn requires_browser(self) -> bool {
        match self {
            OutputFormat::Mhtml
            | OutputFormat::Warc
            | OutputFormat::Wacz
            | OutputFormat::Screenshot
            | OutputFormat::Pdf => true,
            // These may come from static HTML when the page is server-rendered.
            OutputFormat::Html | OutputFormat::Markdown | OutputFormat::Readable => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_distinct() {
        let mut exts: Vec<&str> = OutputFormat::ALL.iter().map(|f| f.extension()).collect();
        exts.sort_unstable();
        exts.dedup();
        assert_eq!(exts.len(), OutputFormat::ALL.len());
    }

    #[test]
    fn browser_requirements() {
        assert!(OutputFormat::Screenshot.requires_browser());
        assert!(OutputFormat::Mhtml.requires_browser());
        assert!(!OutputFormat::Markdown.requires_browser());
        assert!(!OutputFormat::Readable.requires_browser());
    }
}
