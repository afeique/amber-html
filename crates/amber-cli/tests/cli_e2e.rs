//! End-to-end CLI test: run the built `amber` binary and check it writes the
//! requested outputs. Ignored by default — it drives a real browser (via the
//! `--screenshot` path) and downloads the pinned Chromium on first use.

use std::process::Command;

#[test]
#[ignore = "runs the amber binary against a real browser; run with --ignored"]
fn cli_writes_markdown_and_screenshot() {
    let dir = std::env::temp_dir().join(format!("amber-cli-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let status = Command::new(env!("CARGO_BIN_EXE_amber"))
        .arg("data:text/html,<h1>Hello Amber</h1><p>CLI body content here.</p>")
        .arg("-o")
        .arg(&dir)
        .arg("-n")
        .arg("page")
        .arg("--markdown")
        .arg("--screenshot")
        .status()
        .expect("run amber binary");
    assert!(status.success(), "amber exited with failure: {status:?}");

    let md = std::fs::read_to_string(dir.join("page.md")).expect("markdown file written");
    assert!(md.contains("Hello Amber"), "markdown missing content:\n{md}");

    let png = std::fs::read(dir.join("page.png")).expect("screenshot file written");
    assert!(
        png.starts_with(&[0x89, b'P', b'N', b'G']),
        "screenshot is not a PNG ({} bytes)",
        png.len()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
