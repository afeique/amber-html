# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project aims to
adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Capture-once `Snapshot` across every binding** — `snapshot(url, formats)`
  returns a reusable handle that renders/saves any format from a single capture
  (no more one-render-per-format). Exposed in UniFFI, the C ABI (opaque
  `AmberSnapshot*`), Node, Ruby, Swift, Kotlin, Go, and C#.
- **New language bindings** over the C ABI / a NIF: **PHP** (FFI), **Dart**
  (`dart:ffi`), **Lua** (LuaJIT FFI), **R** (`.Call` shim), and **Elixir**
  (rustler NIF, dirty-IO scheduled) — each under `bindings/`.
- **Node surface parity** — `capture`/`captureText`/`save` + a `Format` enum +
  the `Snapshot` class.
- **Python** ships an `import amber` wrapper over the generated `amber_core`.
- **Linux packaging** — `.deb`/`.rpm` metadata, an AUR `PKGBUILD`, and a Nix
  `flake.nix`.
- **Release provenance** — CycloneDX SBOMs (core + CLI), keyless (sigstore)
  cosign signatures attached to the GitHub Release.
- **CI hardening** — every binding builds/loads per push; a C-ABI header
  consistency gate; an MSRV (1.88) + `cargo publish --dry-run` gate; a
  `cargo-public-api` baseline + diff gate for the 1.0 API contract.

### Changed
- **Windows:** browser-backed capture now returns a clean `Unsupported` error
  instead of panicking (the CDP debug pipe is Unix-only for now; static
  captures still work). Windows is gated out of the release artifacts until the
  pipe handle-inheritance lands and is validated on a Windows runner.

## [0.1.0] - 2026-05-23

First public release. A local-first web-page capture engine: a Rust core
(`amber-core`) + CLI (`amber`) that render any page in a pinned, auto-managed
Chrome for Testing over the CDP debug pipe — only when a page needs a browser —
and emit Markdown, readable text, single-file HTML, MHTML, WARC, WACZ,
screenshot, and PDF from one capture pass.

### Added
- **Tiered capture** — cheap HTTP-first fetch with sufficiency scoring,
  escalating to a real browser only when needed; render-once-emit-everything.
- **Outputs** — Markdown, readable text, single-file HTML, MHTML, WARC, WACZ
  (replay-ready, CDXJ-indexed), full-page screenshot, PDF.
- **Settle engine** — lifecycle/network-idle/fonts/auto-scroll/`--wait-for`.
- **Agent-native** — MCP server (`--mcp`), action primitives
  (click/fill/scroll/navigate) via core API, CLI (`--action`), and MCP.
- **Crawl** — multi-page crawl, robots/politeness, content-addressed cache,
  conditional/incremental re-crawl, diff feed, sitemap ingestion, crawl store,
  JSONL/parquet export, self-healing selectors.
- **Structured extraction** — schema + natural-language (bring-your-own LLM),
  with DOM/URL provenance anchoring and a provenance-tagged corpus builder.
- **Auth & privacy** — cookies/headers session state, bring-your-own proxy
  (fetch + render), secret redaction in logs, local-first with zero telemetry.
- **Ops** — HTTP daemon (`--serve`) with a bounded browser pool, browser-instance
  reuse, per-capture time/byte/memory/CPU limits, metrics, recurring captures,
  content/visual change monitoring.
- **Evidence** — tamper-evident manifests with ed25519 signatures.
- **Bindings** — a uniform capture surface (`capture` → bytes for any of the 8
  formats, `capture_markdown`/`capture_readable` text, `save` to file) across
  Python (UniFFI/maturin), Ruby (UniFFI), Swift (UniFFI xcframework), Kotlin/Java
  (UniFFI + JNA), Node (napi-rs), Go (cgo), C#/.NET (P/Invoke), and a C ABI
  (`include/amber.h`), plus the CLI and MCP server.
- **Reproducibility** — byte-stable output for a given capture + pinned browser.
- CI (build/test/clippy/fmt) and a release pipeline for crates.io, PyPI, npm,
  GHCR, GitHub binaries, and Homebrew.

[Unreleased]: https://github.com/afeique/amber-html/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/afeique/amber-html/releases/tag/v0.1.0
