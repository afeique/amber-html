# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project aims to
adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-25

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
- **Bindings** — a uniform capture surface across many languages. One capture
  yields a reusable `Snapshot` (`snapshot(url, formats)`) that renders or saves
  any format with no re-render, alongside `capture` (bytes for any of the 8
  formats), `capture_markdown`/`capture_readable`, and `save`. Available in
  Python (UniFFI/maturin, with an `import amber` wrapper), Node (napi-rs, full
  surface + `Snapshot`), Ruby (UniFFI), Swift (UniFFI xcframework), Kotlin/Java
  (UniFFI + JNA), Go (cgo), C#/.NET (P/Invoke), a C ABI (`include/amber.h`,
  opaque `AmberSnapshot*`), and — over the C ABI / a NIF — PHP (FFI), Dart
  (`dart:ffi`), Lua (LuaJIT FFI), R (`.Call`), and Elixir (rustler); plus the
  CLI and MCP server.
- **Reproducibility** — byte-stable output for a given capture + pinned browser.
- **Packaging** — a tag-driven release pipeline publishing to crates.io, PyPI,
  npm, RubyGems, NuGet, GHCR (Docker), GitHub Release binaries, and Homebrew,
  plus `.deb`/`.rpm` metadata, an AUR `PKGBUILD`, and a Nix `flake.nix` for the
  CLI.
- **Release provenance** — CycloneDX SBOMs (core + CLI) and keyless (sigstore)
  cosign signatures attached to the GitHub Release.
- **CI** — build/test/clippy/fmt; every binding builds/loads per push; a C-ABI
  header-consistency gate; an MSRV (1.88) + `cargo publish --dry-run` gate; and
  a `cargo-public-api` baseline + diff gate for the 1.0 API contract.

### Known limitations
- **Windows:** the 0.1.0 artifacts are Unix-only. Static HTTP captures work on
  Windows, but browser-backed capture is gated out of the release pending
  validation of the `CreateProcessW` debug-pipe path on a Windows CI runner;
  until that job is green it returns a clean `Unsupported` error rather than
  running (the implementation exists but is unverified off-Windows).

[Unreleased]: https://github.com/afeique/amber-html/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/afeique/amber-html/releases/tag/v0.1.0
