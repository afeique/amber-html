# AmberHTML Plans.md

作成日: 2026-05-22 · 最終更新: 2026-05-24

AmberHTML is a faithful web-page capture engine: a Rust library (`amber-core`)
plus a CLI (`amber`) that renders any page in a real, pinned Chromium over the
Chrome DevTools Protocol — **only when a page actually needs a browser** — and
emits the requested representations from a single capture pass: Markdown,
readable text, single-file HTML, MHTML, WARC/WACZ, screenshot, and PDF. It is
embeddable in-process via thin, idiomatic multi-language bindings, ships as a
single binary, and runs locally with no service to operate.

This file is the single source of truth for the feature set and the execution
tasklist. It is harness-tracked; the status markers below drive progress and
drift detection.

**Status markers:** `cc:TODO` not started · `cc:WIP` in progress · `cc:完了`
worker-complete · `blocked` blocked (state the reason). **Priority:** P0
MVP-critical · P1 core · P2 later/optional. **`(user)`** = needs an account,
secret, or credential the agent doesn't have — agent prepares config; user runs
the publish.

---

## Locked technical decisions

- **Language:** Rust. `amber-core` is async inside (tokio), with a **blocking
  public API** so the FFI stays simple.
- **CDP transport:** a single hand-rolled client over Chromium's debug pipe
  (`--remote-debugging-pipe`); no open port, no WebSocket. NUL-delimited JSON
  over inherited file descriptors (fd 3 in / fd 4 out). The `CdpTransport` trait
  is a test-mock seam only. Chosen for security (a localhost debug *port* lets
  any local process hijack the browser) and leanness; only the ~20–40 CDP
  messages we use are implemented.
- **Browser:** always required; a managed, pinned
  [Chrome for Testing](https://googlechromelabs.github.io/chrome-for-testing/)
  build, checksum-verified and cached; `AMBER_CHROMIUM_PATH` escape hatch.
- **HTML capture:** `Page.captureSnapshot` (MHTML) baseline; optional
  single-file-HTML transform (`data:` URIs + inlined `<style>`).
- **Output policy:** **no default output** — the user selects ≥1 format
  explicitly (zero is a hard error); render-once-emit-everything; the requested
  set configures the pass.
- **CLI:** `amber <URL>` + boolean format flags + `-o <dir>` + `-n <name>`
  (default `<safe-url> <YYYY-MM-DD> <HH-MM-SS>`); no `--all`.
- **Bindings:** UniFFI (Python/Swift/Kotlin/Ruby) + a C ABI (cbindgen) for the
  long tail + napi-rs (Node); each a thin, idiomatic facade over `amber-core`.
  **WASM rejected** — it can't drive a native browser process.
- **License:** dual `MIT OR Apache-2.0`. **Naming:** package `amber-html`;
  brand `AmberHTML`. **Workflow:** feature branch → PR into `master`; no direct
  pushes to `master` (user publishes).

### Architecture

```
  Interfaces     CLI · MCP server · HTTP/daemon · language bindings
                                  │  blocking public API
  amber-core     Orchestrator: fetch-strategy → settle → single capture →
  (Rust, async    extraction → emitters → provenance → cache/crawl store
   inside)                       │
                    CdpTransport trait (test-mock seam only)
                                  │
       hand-rolled CDP client over the DEBUG PIPE (--remote-debugging-pipe;
              no open port; NUL-delimited JSON over inherited fd 3/4)
                                  │
                    managed, pinned Chrome for Testing
```

Capture pipeline (cheap-first, escalate-on-insufficiency, output-aware):
**output gate → HTTP fetch → sufficiency score → escalate-to-browser if
uncertain → settle → single capture → memoize verdict.** Bias when uncertain →
render (a wrong "static is fine" silently loses data; a wrong "needs browser"
only costs time).

---

## Shipped in v0.1.0 (engine complete — see CHANGELOG.md)

All of the original Phases 0–5, 7, and the Post-1.0 specialized modes are
`cc:完了` and were removed from this tasklist to keep it actionable. Summary of
what already works (covered by 371 unit tests, 16 ignored browser/live tests,
clippy/fmt-clean CI):

- **Engine** — pinned Chrome-for-Testing fetcher, hand-rolled CDP pipe client
  (Unix), tiered HTTP-first fetch + sufficiency scoring, settle engine,
  browser-render path, process supervision/recovery, structured logging,
  local-first zero-telemetry.
- **Outputs** — Markdown, readable text, single-file HTML, MHTML, WARC, WACZ
  (replayable), full-page screenshot, PDF; render-once-emit-everything.
- **Agent-native** — MCP server (`--mcp`), action primitives, emulation knobs,
  auto-scroll, custom `--wait-for`, token budgeting/accounting, language detect.
- **Crawl** — multi-page crawl, robots/politeness, content-addressed cache,
  conditional/incremental re-crawl, diff feed, sitemap, crawl store,
  JSONL/parquet export, self-healing selectors, pagination.
- **Extraction** — schema + natural-language (BYO LLM), DOM/URL provenance,
  dedup, provenance-tagged corpus builder.
- **Ops** — HTTP daemon + bounded browser pool, per-capture resource limits,
  metrics, recurring captures, content/visual monitoring.
- **Trust** — tamper-evident ed25519 evidence bundles, reproducible captures.
- **Bindings (surface)** — Rust crate; Python (UniFFI/maturin); Node (napi-rs);
  C ABI (cbindgen, `include/amber.h`); plus generated/staged packages under
  `bindings/` for **Ruby** (gem), **Swift** (SwiftPM xcframework),
  **Kotlin/JVM** (Gradle+JNA), **Go** (cgo), **C#/.NET** (P/Invoke). The
  UniFFI + C-ABI surface is widened to all formats (`capture(format)→bytes`,
  `capture_text`, `save`).
- **Release scaffolding** — `release.yml` (crates.io · PyPI · npm · GHCR ·
  GitHub binaries · Homebrew · RubyGems · NuGet), `bindings.yml` CI,
  `RELEASING.md`, `Dockerfile`, Homebrew formula.

---

## Phase 10: FFI ergonomics — capture once, emit many (P1)

The widened binding surface is still **stateless free-functions**: every format
call re-runs `snapshot(url, …)`, so capturing 3 formats = 3 browser renders.
This phase exposes the core's render-once promise across the FFI as a reusable
`Snapshot` object, then propagates it to every language.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 10.1 | **`Snapshot` object across UniFFI + C ABI** — `snapshot(url, formats)` returns a handle; `render`/`text`/`save`/`markdown`/`readable` methods reuse one capture; C side is an opaque handle + `*_free` | One capture serves N formats; UniFFI object + C handle build; error/null-path tests green | - | cc:完了 |
| 10.2 | **Node (napi) surface parity** — all-format `capture(format)`/`captureText`/`save` + a `Snapshot` object | `require('amber')` exposes the widened surface + capture-once object; smoke green | 10.1 | cc:完了 |
| 10.3 | **Propagate `Snapshot` object** into the Ruby/Swift/Kotlin/Go/C# wrappers + per-language smoke | Each wrapper exposes the object idiomatically; smoke compiles/passes | 10.1 | cc:完了 |
| 10.4 | **Python import ergonomics** — `import amber` (a `uniffi.toml` namespace or a thin `amber` wrapper re-exporting `amber_core`) | `import amber; amber.snapshot(url, [...])` works; docs updated | 10.1 | cc:完了 |

## Phase 11: Long-tail C-ABI language bindings (P1/P2)

Each is a thin wrapper over the existing C ABI (`include/amber.h`) + the
capture-once `Snapshot` handle (10.1). Smoke tests use a bad URL so they need no
browser/network (matching the C/Go/Ruby pattern already in the repo).

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 11.1 | **PHP** binding (PHP FFI over the C ABI) + `composer.json` + smoke | `Amber\capture_markdown($url)` returns text; bad-URL smoke throws; `bindings/php` documented | 10.1 | cc:完了 |
| 11.2 | **Dart/Flutter** binding (`dart:ffi`) + `pubspec.yaml` + smoke | `Amber.captureMarkdown(url)`; `dart test` smoke green; `bindings/dart` | 10.1 | cc:完了 (CI-validated — no Dart SDK locally) |
| 11.3 | **Lua** binding (LuaJIT FFI / C module) + `.rockspec` + smoke | `require('amber').capture_markdown(url)`; smoke green; `bindings/lua` | 10.1 | cc:完了 (CI-validated — no LuaJIT locally) |
| 11.4 | **R** binding (C interface via `.Call`/FFI) + package skeleton + smoke | `amber::capture_markdown(url)`; smoke green; `bindings/r` | 10.1 | cc:完了 (CI-validated — no R locally; least-validated, see notes) |
| 11.5 | **Elixir** binding (rustler NIF on amber-core, dirty IO) + `mix.exs` + smoke | `Amber.capture_markdown/1`; `mix test` smoke green; `bindings/elixir` | 10.1 | cc:完了 (NIF compiles+clippy locally; mix test CI-gated — no Elixir) |

## Phase 12: Windows compatibility (P0 — release blocker)

The CDP pipe transport is Unix-only: Chromium's `--remote-debugging-pipe` reads
the pipe ends as CRT fds 3/4, which on Windows must be passed via the MSVCRT
`lpReserved2` block of `STARTUPINFOW` (a raw `CreateProcessW` spawn) —
`std::process::Command` can't express that. **This cannot be developed safely on
the current macOS host:** there is no Windows runtime to validate the unsafe
spawn, and it can't even be cross-compile-checked (`ring`/rustls' C build script
fails for `x86_64-pc-windows-msvc` without a Windows cross-toolchain). So the
full implementation is **blocked on a Windows dev/CI environment**; meanwhile the
panic is removed and Windows artifacts are gated so nothing ships broken.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 12.1 | **No-panic Windows path** — browser capture returns a clean typed `CdpError` (`ErrorKind::Unsupported`) instead of `unimplemented!()`; the static HTTP-fetch path is unaffected | No reachable panic on Windows; static captures still work | - | cc:完了 |
| 12.2 | **Windows pipe handle-inheritance** — `CreateProcessW` + the CRT `lpReserved2` fd-3/4 block; real screenshot capture on a `windows-latest` CI runner | A browser capture succeeds end-to-end on Windows CI | 12.1 | blocked (needs a Windows dev/CI env; no local Windows runtime and `ring` won't cross-compile-check from macOS) |
| 12.3 | **Gate Windows release artifacts** — drop Windows from the binaries/wheels/npm matrices until 12.2 is green; document the Windows status | No shipped Windows artifact panics; status documented in README | 12.1 | cc:完了 |

## Phase 13: Release-blocking docs & CI hardening (P0/P1)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 13.1 | **README/docs accuracy** — drop the "early development" banner; reflect feature-complete v0.1 (front page for crates.io/PyPI/npm) | README has no stale "just beginning" copy | - | cc:完了 |
| 13.2 | **C-ABI header consistency gate** in CI — compile + link + run `examples/c` against `include/amber.h` (robust vs. a brittle cbindgen text-diff: proves the header matches the exported symbols) | CI fails if the header drifts from the ABI; validated locally (links + runs) | - | cc:完了 |
| 13.3 | **Binding smoke matrix** — `bindings.yml` builds/loads every binding per push: C ABI, Python, Node, Ruby, Go, C#, Swift, Kotlin, PHP, Lua, Dart, Elixir (hermetic: load + bad-URL/compile, no browser). **R** is a documented gap (external-dylib linking unvalidated) | Each wired binding compiles/loads per push; PHP/Node/Python commands validated locally | 11.x | cc:完了 |
| 13.4 | **MSRV + publish dry-run gates** — pin MSRV and check it; `cargo publish --dry-run` for core+cli; a `release.yml` `workflow_dispatch` dry-run path | Pre-tag gates pass; first real publish is de-risked | - | cc:TODO |

## Phase 14: Distribution GA & packaging (P1/P2)

`release.yml` already wires the first eight channels; these tasks take them from
"configured" to "published," then broaden reach. Most publishes need a
credential/account → marked `(user)`; the agent prepares all config/manifests.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 14.1 | **Registry name reservation/verification** — crates.io, PyPI, npm, RubyGems, NuGet, Maven coords | Each name confirmed free/reserved | - | cc:TODO (user) |
| 14.2 | **GA the wired channels** — crates.io · PyPI · npm · GHCR · GitHub binaries · Homebrew tap (`afeique/homebrew-amber`, fill `sha256`) | Installable from each; smoke-install verified | 14.1, 12.3 | cc:TODO (user) |
| 14.3 | **RubyGems + NuGet publish** — release jobs exist; add secrets | `gem install amber-html` / `dotnet add package` work | 14.2 | cc:TODO (user) |
| 14.4 | **Maven Central · SwiftPM/CocoaPods · Go module tag · conda-forge** | Installable from each ecosystem | 10.3 | cc:TODO |
| 14.5 | **Windows managers** — Scoop · WinGet · Chocolatey manifests | Each install path verified on Windows | 12.3, 14.2 | cc:TODO |
| 14.6 | **Linux** — AUR · Nix flake · `.deb` (cargo-deb) · `.rpm` (cargo-generate-rpm) | Each install path verified | 14.2 | cc:TODO |
| 14.7 | **C-ABI long-tail packaging** — vcpkg/Conan (C/C++); Packagist (PHP) · luarocks (Lua) · CRAN (R) · Hex (Elixir) · pub.dev (Dart) | Each language installs from its registry | 11.x | cc:TODO |
| 14.8 | **Release trust** — sign artifacts (cosign/sigstore) + attach an SBOM | Signatures + SBOM on each release | 14.2 | cc:TODO (user) |

## Phase 15: API stability for 1.0 (P1)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 15.1 | **Public API freeze review** — audit `amber-core` exports + the FFI/Snapshot surface; document the stability contract; deny accidental breakage with `cargo-public-api` in CI | Public API reviewed, documented, and CI-guarded for 1.0 | 10.x | cc:WIP |

---

## Notes

- **Build/test:** `cargo build --workspace --locked` + `cargo test --workspace`
  must stay green; browser/live tests are `#[ignore]`d (run with `-- --ignored`
  against the pinned Chromium).
- **Resolved (Phase 11/Elixir):** chose a **rustler NIF** over a hand C NIF — it
  calls `amber-core`'s Rust API directly (no C marshaling), is dirty-IO scheduled
  (captures block on a browser), and compiles under the workspace `cargo` build
  so CI compile-checks it. The NIF crate is a workspace member (like
  `amber-node`); the mix project lives in `bindings/elixir`.
