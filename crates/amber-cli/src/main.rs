//! AmberHTML CLI (`amber`). See `Plans.md`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use amber_core::{snapshot, CaptureOptions, Error, OutputFormat, RenderMode};

/// AmberHTML — local-first web capture for AI agents.
#[derive(Parser, Debug)]
#[command(name = "amber", version, about)]
struct Cli {
    /// URL to capture.
    url: String,

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
    /// CSS selector / JS predicate to wait for (forces a browser).
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

fn main() -> ExitCode {
    let cli = Cli::parse();

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

    match snapshot(&cli.url, &formats, opts) {
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
                cli.url,
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
