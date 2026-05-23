//! AmberHTML CLI (`amber`). See `Plans.md`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use amber_core::{
    snapshot, Action, BlockPolicy, CaptureOptions, Error, OutputFormat, RenderMode, ResourceLimits,
    SessionState,
};

/// AmberHTML — faithful local web-page capture (library + CLI).
#[derive(Parser, Debug)]
#[command(name = "amber", version, about)]
struct Cli {
    /// URL to capture. Omit when running with `--mcp`.
    url: Option<String>,

    /// Run as an MCP server over stdio (newline-delimited JSON-RPC), exposing a
    /// `snapshot` tool. Ignores the URL and output flags.
    #[arg(long)]
    mcp: bool,

    /// Single-file inlined HTML (.html).
    #[arg(long)]
    html: bool,
    /// MHTML bundle (.mhtml).
    #[arg(long)]
    mhtml: bool,
    /// Clean Markdown (.md).
    #[arg(long)]
    markdown: bool,
    /// Readable plain text (.txt).
    #[arg(long)]
    readable: bool,
    /// WARC archive (.warc).
    #[arg(long)]
    warc: bool,
    /// WACZ archive (.wacz).
    #[arg(long)]
    wacz: bool,
    /// Full-page screenshot (.png).
    #[arg(long)]
    screenshot: bool,
    /// PDF (.pdf).
    #[arg(long)]
    pdf: bool,

    /// Directory to write into (created if missing).
    #[arg(short = 'o', long = "output-dir", default_value = ".")]
    output_dir: PathBuf,
    /// Base filename, no extension. Default: `<safe-url> <date> <time>`.
    #[arg(short = 'n', long = "name")]
    name: Option<String>,

    /// Browser rendering policy.
    #[arg(long, value_enum, default_value = "auto")]
    render: RenderModeArg,
    /// Wait for a condition before capture (forces a browser): a CSS selector,
    /// or a JS boolean predicate prefixed with `js:` (e.g. `js:window.ready`).
    #[arg(long)]
    wait_for: Option<String>,
    /// Minimum static content length to treat as sufficient.
    #[arg(long)]
    min_content: Option<usize>,

    /// Extra request header `Name: Value` for behind-auth pages (repeatable);
    /// applied to both the static fetch and the browser render.
    #[arg(long = "header", value_name = "NAME: VALUE")]
    headers: Vec<String>,
    /// Session cookie `name=value` (repeatable); applied to fetch and render.
    #[arg(long = "cookie", value_name = "NAME=VALUE")]
    cookies: Vec<String>,

    /// Route fetches and the browser render through a proxy, e.g.
    /// `http://host:8080` or `socks5://user:pass@host:1080`.
    #[arg(long, value_name = "URL")]
    proxy: Option<String>,

    /// Per-capture byte budget: reject a response body larger than this.
    #[arg(long, value_name = "BYTES")]
    max_bytes: Option<u64>,
    /// Per-capture wall-clock budget in seconds (checked before rendering).
    #[arg(long, value_name = "SECONDS")]
    max_time: Option<u64>,

    /// Agent action to run before capture (repeatable; forces a browser):
    /// `click:<sel>`, `fill:<sel>=<val>`, `scroll-bottom`, `scrollby:<x>,<y>`,
    /// or `navigate:<url>`.
    #[arg(long = "action", value_name = "SPEC")]
    actions: Vec<String>,

    /// Block a common ad/tracker host preset during the render.
    #[arg(long)]
    block_ads: bool,
    /// Block requests whose URL contains this substring (repeatable).
    #[arg(long = "block", value_name = "SUBSTRING")]
    block: Vec<String>,
    /// Don't load images during the render.
    #[arg(long)]
    block_images: bool,
    /// Don't load media (audio/video) during the render.
    #[arg(long)]
    block_media: bool,
    /// Don't load web fonts during the render.
    #[arg(long)]
    block_fonts: bool,
}

/// Parse a `Name: Value` header argument, splitting on the first colon (so the
/// value may itself contain colons, e.g. a URL). Name is trimmed and required.
fn parse_header(s: &str) -> Result<(String, String), String> {
    let (name, value) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid --header {s:?} (expected \"Name: Value\")"))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("invalid --header {s:?} (empty name)"));
    }
    Ok((name.to_string(), value.trim().to_string()))
}

/// Parse a `name=value` cookie argument, splitting on the first `=`. Name is
/// trimmed and required; the value is taken verbatim.
fn parse_cookie(s: &str) -> Result<(String, String), String> {
    let (name, value) = s
        .split_once('=')
        .ok_or_else(|| format!("invalid --cookie {s:?} (expected \"name=value\")"))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("invalid --cookie {s:?} (empty name)"));
    }
    Ok((name.to_string(), value.to_string()))
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum RenderModeArg {
    Auto,
    Always,
    Never,
}

fn to_render_mode(v: RenderModeArg) -> RenderMode {
    match v {
        RenderModeArg::Auto => RenderMode::Auto,
        RenderModeArg::Always => RenderMode::Always,
        RenderModeArg::Never => RenderMode::Never,
    }
}

impl Cli {
    /// Collect the selected output formats in CLI order.
    fn formats(&self) -> Vec<OutputFormat> {
        let mut f = Vec::new();
        if self.html {
            f.push(OutputFormat::Html);
        }
        if self.mhtml {
            f.push(OutputFormat::Mhtml);
        }
        if self.markdown {
            f.push(OutputFormat::Markdown);
        }
        if self.readable {
            f.push(OutputFormat::Readable);
        }
        if self.warc {
            f.push(OutputFormat::Warc);
        }
        if self.wacz {
            f.push(OutputFormat::Wacz);
        }
        if self.screenshot {
            f.push(OutputFormat::Screenshot);
        }
        if self.pdf {
            f.push(OutputFormat::Pdf);
        }
        f
    }
}

/// Initialize structured logging. Level is controlled by `AMBER_LOG` (falling
/// back to `RUST_LOG`), e.g. `AMBER_LOG=debug`; defaults to `warn`. Logs go to
/// stderr so stdout stays reserved for the `wrote <path>` output lines.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("AMBER_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Run the MCP server over stdio, exposing a `snapshot` tool backed by the core.
fn run_mcp() -> ExitCode {
    let capture = |url: &str, format: &str, action_specs: &[String]| -> Result<String, String> {
        let fmt = match format {
            "readable" => OutputFormat::Readable,
            _ => OutputFormat::Markdown,
        };
        let mut actions = Vec::new();
        for spec in action_specs {
            actions.push(Action::parse(spec)?);
        }
        let opts = CaptureOptions {
            actions,
            ..Default::default()
        };
        let snap = snapshot(url, &[fmt], opts).map_err(|e| e.to_string())?;
        let bytes = snap.render(fmt).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    };
    match amber_core::mcp::serve(std::io::stdin().lock(), std::io::stdout().lock(), capture) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("amber mcp: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    init_tracing();

    let cli = Cli::parse();

    if cli.mcp {
        return run_mcp();
    }

    let Some(url) = cli.url.clone() else {
        eprintln!("error: a URL is required (or use --mcp to run as an MCP server)");
        return ExitCode::from(2);
    };

    let formats = cli.formats();
    if formats.is_empty() {
        eprintln!(
            "error: select at least one output format \
             (--html --mhtml --markdown --readable --warc --wacz --screenshot --pdf)"
        );
        return ExitCode::from(2);
    }

    let mut session = SessionState::default();
    for h in &cli.headers {
        match parse_header(h) {
            Ok(pair) => session.headers.push(pair),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        }
    }
    for c in &cli.cookies {
        match parse_cookie(c) {
            Ok(pair) => session.cookies.push(pair),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        }
    }

    let mut actions = Vec::new();
    for spec in &cli.actions {
        match Action::parse(spec) {
            Ok(a) => actions.push(a),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        }
    }

    let mut block = BlockPolicy::default();
    if cli.block_ads {
        block = block.with_ad_trackers();
    }
    block
        .blocked_url_substrings
        .extend(cli.block.iter().cloned());
    block.block_images = cli.block_images;
    block.block_media = cli.block_media;
    block.block_fonts = cli.block_fonts;

    let opts = CaptureOptions {
        render: to_render_mode(cli.render),
        wait_for: cli.wait_for.clone(),
        min_content: cli.min_content,
        session,
        proxy: cli.proxy.clone(),
        limits: ResourceLimits {
            max_bytes: cli.max_bytes,
            max_duration: cli.max_time.map(std::time::Duration::from_secs),
        },
        actions,
        block,
        ..Default::default()
    };

    match snapshot(&url, &formats, opts) {
        Ok(snap) => {
            let mut code = ExitCode::SUCCESS;
            for fmt in &formats {
                match snap.save(*fmt, &cli.output_dir, cli.name.as_deref()) {
                    Ok(path) => println!("wrote {}", path.display()),
                    Err(e) => {
                        eprintln!("error: failed to write .{}: {e}", fmt.extension());
                        code = ExitCode::FAILURE;
                    }
                }
            }
            code
        }
        Err(Error::NotImplemented(what)) => {
            eprintln!(
                "amber: capture is not implemented yet (scaffold): {what}\n  \
                 url={}  formats={:?}  output-dir={}",
                url,
                formats.iter().map(|f| f.extension()).collect::<Vec<_>>(),
                cli.output_dir.display()
            );
            ExitCode::from(3)
        }
        Err(e) => {
            eprintln!("amber: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header_splits_on_first_colon() {
        assert_eq!(
            parse_header("Authorization: Bearer x").unwrap(),
            ("Authorization".to_string(), "Bearer x".to_string())
        );
        // The value may itself contain colons (e.g. a URL).
        assert_eq!(
            parse_header("X-Origin: http://a/b").unwrap().1,
            "http://a/b"
        );
        assert!(parse_header("no-colon-here").is_err());
        assert!(parse_header(":   value").is_err());
    }

    #[test]
    fn parse_cookie_splits_on_first_equals() {
        assert_eq!(
            parse_cookie("sid=abc").unwrap(),
            ("sid".to_string(), "abc".to_string())
        );
        // Split on the first `=` only; the value keeps the rest verbatim.
        assert_eq!(parse_cookie("token=a=b").unwrap().1, "a=b");
        assert!(parse_cookie("noequals").is_err());
    }
}
