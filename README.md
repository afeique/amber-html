# AmberHTML

> A Rust library and CLI that drives a local, pinned Chromium over the Chrome
> DevTools Protocol to faithfully render any web page — but only when a page
> actually needs a browser — and emits the requested representations from a
> single capture pass: Markdown, readable text, single-file HTML, MHTML,
> WARC/WACZ, screenshot, and PDF.

> 🚧 **Status: early development.** The architecture is set; implementation is
> just beginning. Expect rapid change.

## What it is

AmberHTML captures web pages locally, with no service to run. It tries a cheap
HTTP fetch first and escalates to a real browser only when the page requires
JavaScript, then produces every requested format from one render. It runs on
your machine — including against `localhost`, intranet, and behind-auth pages —
and can be embedded in-process via thin, idiomatic bindings to other languages.

## Quickstart

Build the CLI from this workspace (a pinned Chrome for Testing is downloaded and
cached automatically the first time a capture needs a browser):

```sh
cargo build --release            # binary at target/release/amber
```

Capture a page — pick the formats explicitly (there is no default output):

```sh
# Static-friendly pages stay on the cheap HTTP path:
amber https://example.com --markdown --readable -o ./out

# A screenshot (or --mhtml/--pdf/--html) forces a real browser render:
amber https://example.com --screenshot --markdown -o ./out -n example

# Wait for a condition, or emulate a viewport, before capturing:
amber https://app.example.com --markdown --render always --wait-for "js:window.ready === true"
```

Outputs are written as `<output-dir>/<name>.<ext>`; with no `-n`, the name is
`<safe-url> <YYYY-MM-DD> <HH-MM-SS>`.

Run it as an **MCP server** over stdio (exposes a `snapshot` tool to agents):

```sh
amber --mcp
```

Use it as a **Rust library**:

```rust
use amber_core::{snapshot, CaptureOptions, OutputFormat};

let snap = snapshot(
    "https://example.com",
    &[OutputFormat::Markdown],
    CaptureOptions::default(),
)?;
let markdown = String::from_utf8(snap.render(OutputFormat::Markdown)?)?;
# Ok::<(), amber_core::Error>(())
```

Set `AMBER_LOG=debug` (or `RUST_LOG`) for structured logs on stderr.

## Goals

- **Tiered fetching** — try a cheap HTTP fetch first; escalate to a headless
  browser only when the page actually needs JavaScript.
- **Render once, emit everything** — Markdown, readable text, single-file HTML,
  MHTML, WARC/WACZ, screenshot, and PDF from a single browser pass.
- **No default output** — you select formats explicitly; requesting none is an
  error, and the requested set configures the capture pass.
- **Faithful rendering** — a first-class settle engine (lifecycle events,
  network-idle, fonts, lazy-load scroll, custom wait conditions) decides a page
  is truly done before capture.
- **Caching & incremental crawling** — content-hash + conditional requests; skip
  unchanged pages, return only diffs.
- **Provenance** — extracted facts can anchor back to a DOM node, screenshot
  region, and source URL.
- **Embeddable everywhere** — a Rust core with thin bindings to many languages,
  plus a standalone CLI and an MCP server.

## Design (in brief)

- Rust core (`amber-core`) driving Chromium over CDP, with a blocking public API.
- A single hand-rolled CDP client over Chromium's debug pipe
  (`--remote-debugging-pipe`) — no open debugging port.
- A pinned, auto-managed [Chrome for Testing](https://googlechromelabs.github.io/chrome-for-testing/)
  build for reproducible rendering.
- Bindings via UniFFI (Python/Swift/Kotlin/Ruby), a C ABI (the long tail), and
  napi-rs (Node).

## Roadmap

The full feature catalog and phased execution tasklist live in
[Plans.md](Plans.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
