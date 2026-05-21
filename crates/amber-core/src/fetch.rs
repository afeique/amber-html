//! Tiered-fetch decision helpers. See `docs/PLAN.md` §7.

use crate::output::OutputFormat;

/// User control over browser rendering (CLI `--render`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// HTTP-first; escalate to a browser only when needed.
    #[default]
    Auto,
    /// Always render in a browser.
    Always,
    /// Never use a browser; fail if static content is insufficient.
    Never,
}

/// Output gate (PLAN.md §7, step 1): is a browser required *before* fetching,
/// purely from the requested outputs and the render mode? When this is true we
/// skip static detection and go straight to the browser.
pub fn browser_required_upfront(formats: &[OutputFormat], mode: RenderMode) -> bool {
    match mode {
        RenderMode::Always => true,
        RenderMode::Never => false,
        RenderMode::Auto => formats.iter().any(|f| f.requires_browser()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_forces_browser_in_auto() {
        assert!(browser_required_upfront(
            &[OutputFormat::Screenshot],
            RenderMode::Auto
        ));
    }

    #[test]
    fn markdown_alone_does_not_force_browser_in_auto() {
        assert!(!browser_required_upfront(
            &[OutputFormat::Markdown],
            RenderMode::Auto
        ));
    }

    #[test]
    fn always_forces_browser_even_for_markdown() {
        assert!(browser_required_upfront(
            &[OutputFormat::Markdown],
            RenderMode::Always
        ));
    }

    #[test]
    fn never_never_uses_browser() {
        assert!(!browser_required_upfront(
            &[OutputFormat::Screenshot],
            RenderMode::Never
        ));
    }
}
