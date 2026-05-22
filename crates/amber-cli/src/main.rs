//! AmberHTML CLI (`amber`). See `Plans.md`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use amber_core::{snapshot, CaptureOptions, Error, OutputFormat, RenderMode};

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
    /// Base filename, no extension. Default: "<safe-url> <date> <time>".
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
    let capture = |url: &str, format: &str| -> Result<String, String> {
        let fmt = match format {
            "readable" => OutputFormat::Readable,
            _ => OutputFormat::Markdown,
        };
        let snap =
            snapshot(url, &[fmt], CaptureOptions::default()).map_err(|e| e.to_string())?;
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

    let opts = CaptureOptions {
        render: to_render_mode(cli.render),
        wait_for: cli.wait_for.clone(),
        min_content: cli.min_content,
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
