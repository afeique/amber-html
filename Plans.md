# AmberHTML Plans.md

作成日: 2026-05-22

AmberHTML is a faithful web-page capture engine: a Rust library (`amber-core`)
plus a CLI (`amber`) that renders any page in a real, pinned Chromium over the
Chrome DevTools Protocol — **only when a page actually needs a browser** — and
emits the requested representations from a single capture pass: Markdown,
readable text, single-file HTML, MHTML, WARC/WACZ, screenshot, and PDF. It is
embeddable in-process via thin, idiomatic multi-language bindings, ships as a
single binary, and runs locally with no service to operate.

This file is the single source of truth for the feature set and the execution
tasklist. It is the harness-tracked plan; status markers below drive progress
and drift detection.

**Status markers:** `cc:TODO` not started · `cc:WIP` in progress · `cc:完了`
worker-complete · `blocked` blocked (state the reason). **Priority:** P0
MVP-critical · P1 core · P2 later/optional.

---

## Design notes

### Architecture

```
  Interfaces     CLI · MCP server · HTTP/daemon · language bindings
                                  │  blocking public API
  amber-core     Orchestrator
  (Rust, async     ├─ Fetch strategy: HTTP-first → escalate
   inside)         ├─ Settle engine (lifecycle/idle/fonts/…)
                   ├─ Capture pass (single render)
                   ├─ Extraction (markdown/readable/schema)
                   ├─ Emitters (html/mhtml/md/txt/warc/wacz/png/pdf)
                   ├─ Provenance map
                   └─ Cache + crawl store
                  CdpTransport trait (test-mock seam only)
                                  │
       hand-rolled CDP client over the DEBUG PIPE (--remote-debugging-pipe;
              no open port; NUL-delimited JSON over inherited fd 3/4)
                                  │
                    managed, pinned Chrome for Testing
```

- **`amber-core`** — async internally (tokio), **blocking public API** so FFI
  stays simple.
- **Transport** — a single hand-rolled CDP client over Chromium's debug pipe
  (`--remote-debugging-pipe`): no open debugging port; NUL-delimited JSON over
  inherited file descriptors (fd 3 in / fd 4 out). The `CdpTransport` trait
  exists only as a thin seam for test mocking.
- **Browser management** — auto-download a pinned
  [Chrome for Testing](https://googlechromelabs.github.io/chrome-for-testing/)
  build, checksum-verified, cached; `AMBER_CHROMIUM_PATH` escape hatch. Always
  required — CDP drives a real browser; nothing bundles one.
- **Bindings** — UniFFI (Python/Swift/Kotlin/Ruby) and a C ABI (cbindgen) for
  the long tail; napi-rs for Node; all thin, idiomatic facades over `amber-core`.

### Capture pipeline (cheap-first, escalate-on-insufficiency, output-aware)

```
1. OUTPUT GATE   ─ does a requested output inherently need a browser?
2. HTTP FETCH    ─ plain GET + parse static HTML (cheap, ~ms)
3. SUFFICIENCY   ─ score: is the content actually present?
4. ESCALATE      ─ uncertain/insufficient → render in a browser
5. SETTLE        ─ (browser path) wait until the page is truly done
6. CAPTURE       ─ single pass; emit only the requested formats
7. MEMOIZE       ─ cache the browser-vs-static verdict per domain/pattern
```

- **Output gate:** `--screenshot`, `--pdf`, `--mhtml`, high-fidelity
  `--warc`/`--wacz`, and anything requiring rendered JS ⇒ browser always.
  Only `--markdown`, `--readable`, and plain `--html` on a possibly-static page
  are cheap-path candidates.
- **Sufficiency scoring:** combine framework-shell/`<noscript>`/content-floor
  markers with an actual main-content check. Framework markers alone ≠ needs
  browser (server-rendered Next/Nuxt often ship full content statically).
- **Bias when uncertain → render.** A wrong "static is fine" silently loses
  data; a wrong "needs browser" only costs time. The gray zone falls back to
  rendering.
- **Settle engine:** configurable policy composing lifecycle events
  (`load`, `networkIdle`), in-flight request count, settle delay,
  `document.fonts.ready`, auto-scroll for lazy-load, and an optional caller
  `--wait-for` selector / JS predicate. Default ≈ `load` + `networkAlmostIdle`
  + ~200 ms + `fonts.ready`.
- **Honest limit:** the heuristic detects *"is the page rendered,"* not
  *"is the specific datum you want present."* Late-XHR content needs
  `--render always` or `--wait-for`.

### Output formats & policy

**Policy: NO default output.** The user must explicitly request ≥1 format; zero
outputs is a hard error.

| Flag | Format | Ext | Needs browser? | Notes |
|---|---|---|---|---|
| `--html` | Single-file inlined HTML | `.html` | usually | From MHTML → `data:` URIs + inlined `<style>` |
| `--mhtml` | MHTML bundle | `.mhtml` | yes | `Page.captureSnapshot`; faithful baseline |
| `--markdown` | Clean Markdown | `.md` | maybe | HTML→MD converter |
| `--readable` | Readable plain text | `.txt` | maybe | Main-content extractor |
| `--warc` | WARC archive | `.warc` | yes (hi-fi) | Network recording enabled before navigate |
| `--wacz` | WACZ (replayable) | `.wacz` | yes (hi-fi) | Uses `warc`/`wacksy` crates |
| `--screenshot` | Full-page PNG | `.png` | yes | `Page.captureScreenshot` |
| `--pdf` | PDF | `.pdf` | yes | `Page.printToPDF` |

**Render once, emit everything:** a single capture pass produces all requested
formats; the requested output set configures the pass (WARC/WACZ enable network
recording up front; MHTML triggers `captureSnapshot` post-settle) and feeds the
tiered fetch (static page + Markdown-only ⇒ no browser).

### CLI

```
amber <URL> [OUTPUT FLAGS] [OPTIONS]

OUTPUT SELECTION  (at least one required — no default)
  --html  --mhtml  --markdown  --readable  --warc  --wacz  --screenshot  --pdf
  (no --all — select formats precisely)

NAMING / LOCATION
  -o, --output-dir <DIR>   default: current dir; created if missing
  -n, --name <NAME>        base filename, no extension; requires an arg when given
                           omitted → "<safe-url> <YYYY-MM-DD> <HH-MM-SS>" (local time)

RENDERING
  --render auto|always|never   (default auto — tiered fetch)
  --wait-for <SELECTOR>
  --min-content <N>
```

Format flags are boolean; destination = `-o` dir, basename = `-n` or the default
name, extension derived from format. Default name uses `HH-MM-SS` (colons are
illegal on Windows) and a filesystem-safe URL truncated to ~120 chars.

### Bindings

A single capture produces a `Snapshot` object; each language exposes ergonomic
save/get methods on it (`snap.save_markdown(...)`, `snap.markdown()`, …). Bindings
are thin, idiomatic facades over the C-ABI/UniFFI marshalling, not raw FFI
mirrors. Public FFI surface is **blocking** (tokio hidden inside core).
**WASM rejected** — it can't drive a native browser process.

### Locked technical decisions

- **Language:** Rust. `amber-core` async inside, blocking public API.
- **CDP transport:** single hand-rolled client over the debug pipe
  (`--remote-debugging-pipe`); no open port, no WebSocket. Chosen for security
  (a localhost debug *port* lets any local process hijack the browser; the pipe
  is reachable only by the parent that spawned it) and leanness. We always spawn
  our own pinned Chromium, so attaching to a remote browser — the only scenario
  WebSocket would serve — is out of scope. Implement only the ~20–40 CDP
  messages we use. The `CdpTransport` trait is kept as a test-mock seam and as
  the slot where a WebSocket transport could be added later if remote-attach is
  ever needed.
- **Browser:** always required; managed, pinned Chrome for Testing,
  checksum-verified, cached; `AMBER_CHROMIUM_PATH` escape hatch.
- **HTML capture:** `Page.captureSnapshot` (MHTML) baseline; optional
  single-file-HTML transform.
- **Output policy:** no default output; explicit selection; render-once-emit-
  everything; output set configures the pass.
- **CLI:** `-o` dir + boolean format flags + `-n` name (default
  `<safe-url> <date> <time>`); no `--all`.
- **Bindings:** UniFFI + C ABI, both first-tier; idiomatic per-language
  `Snapshot` facade; WASM rejected.
- **License:** dual `MIT OR Apache-2.0`.
- **Naming:** package `amber-html`; brand `AmberHTML`.
- **Workflow:** feature branch → PR into `main`; no direct pushes to `main`.

---

## Phase 0: Foundation

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 0.1 | Cargo workspace skeleton (`amber-core`, `amber-cli`) | `cargo build` succeeds across the workspace | - | cc:完了 |
| 0.2 | License (dual MIT OR Apache-2.0) + LICENSE files | Both LICENSE files present; manifests set `license` | - | cc:完了 |
| 0.3 | Managed pinned Chrome for Testing fetcher (download/cache/checksum-verify) | Fetcher downloads a pinned build, verifies checksum, caches; `AMBER_CHROMIUM_PATH` honored | 0.1 | cc:完了 |
| 0.4 | Hand-rolled CDP pipe client (`--remote-debugging-pipe`, fd 3/4, NUL-delimited JSON) | Client spawns Chromium and round-trips a CDP request/response over the pipe | 0.1 | cc:完了 |
| 0.5 | navigate + screenshot smoke test (end-to-end) | `cargo run` launches pinned Chromium and screenshots a URL to a file | 0.3, 0.4 | cc:WIP |
| 0.6 | CI (build + test + clippy + fmt) | A green CI run on push/PR covering build, test, clippy, fmt | 0.1 | cc:TODO |

## Phase 1: v0.1 — Reader MVP (P0)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 1.1 | HTTP-first static fetch tier (blocking GET + parse) | Static HTML is fetched and parsed with a bounded timeout | 0.1 | cc:完了 |
| 1.2 | Sufficiency scoring + tiered-fetch escalation (`--render auto`) | Static-vs-browser verdict computed from markers + content check; uncertain → render | 1.1, 0.4 | cc:完了 |
| 1.3 | Settle engine (lifecycle / network-idle / fonts / settle-delay) | Configurable policy; capture waits until settled before snapshot | 0.4 | cc:WIP |
| 1.4 | Browser-render capture path (drive CDP, produce `RawCapture`) | A JS-heavy page renders and yields rendered HTML | 0.4, 1.3 | cc:WIP |
| 1.5 | Clean Markdown emitter | `--markdown` writes a `.md` matching golden output for sample pages | 1.1 | cc:完了 |
| 1.6 | Readable plain-text emitter (main-content) | `--readable` writes a `.txt` with boilerplate removed | 1.1 | cc:完了 |
| 1.7 | Full-page screenshot emitter | `--screenshot` writes a `.png` of the rendered page | 1.4 | cc:WIP |
| 1.8 | No-default-output policy (≥1 format required) | Empty format set returns a hard error | 0.1 | cc:完了 |
| 1.9 | URL+datetime default naming (`-n` optional) | Default name = `<safe-url> <YYYY-MM-DD> <HH-MM-SS>`, filesystem-safe | 0.1 | cc:完了 |
| 1.10 | CLI (`amber <url>` + output flags + `-o`/`-n`/`--render`/`--wait-for`/`--min-content`) | `amber <url> -o ./out --markdown --readable --screenshot` works, HTTP-only when possible | 1.5, 1.6, 1.7, 1.9 | cc:WIP |
| 1.11 | Boilerplate/nav/ad/cookie-banner removal | Extracted Markdown/readable excludes common chrome on sample pages | 1.5, 1.6 | cc:WIP |
| 1.12 | Page metadata (title/lang/canonical/OpenGraph/links) | Metadata extracted and exposed on `Snapshot` | 1.1 | cc:完了 |
| 1.13 | Timeout / retry / partial-result handling | Per-tier timeouts enforced; partial results returned where safe | 1.1, 1.4 | cc:WIP |
| 1.14 | Process lifecycle / reconnection / crash recovery | Browser process is supervised; a crash is detected and surfaced cleanly | 0.4 | cc:TODO |
| 1.15 | Structured logging + tracing | `tracing` spans across fetch/settle/capture; configurable level | 0.1 | cc:完了 |
| 1.16 | Local-first, zero telemetry; airgapped operation after browser cached | No network calls except the target + one-time browser download; works offline once cached | 0.3 | cc:TODO |

## Phase 2: v0.2 — Agent-native (P0/P1)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 2.1 | MCP server (read/screenshot/extract tools) | An MCP client can request a capture and receive the output | Phase 1 | cc:TODO |
| 2.2 | Token-budget-aware output + measured token count | Output trimmed to a budget; token count reported | 1.5, 1.6 | cc:TODO |
| 2.3 | Token accounting & cost reporting | Per-capture token counts surfaced to the caller | 2.2 | cc:TODO |
| 2.4 | Resource blocking (ads/trackers/optional images) | Blocked resource classes are not fetched; faster renders | 1.4 | cc:TODO |
| 2.5 | Agent-native action primitives (navigate/click/fill/scroll/wait) | Actions executable via core API and MCP | 1.4, 2.1 | cc:TODO |
| 2.6 | Emulation knobs (viewport/device/locale/timezone/dark-mode) | Each knob measurably changes the rendered capture | 1.4 | cc:TODO |
| 2.7 | Auto-scroll for lazy-load | Lazy content loads before capture on sample pages | 1.3 | cc:TODO |
| 2.8 | Custom ready signal (`--wait-for` selector/predicate) | Capture waits for the selector/predicate before snapshot | 1.3 | cc:TODO |
| 2.9 | Language detection / encoding | Detected language and correct decoding on non-UTF-8 pages | 1.1 | cc:完了 |

## Phase 3: v0.3 — Crawl (P1)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 3.1 | Multi-page crawl (link follow, scope, depth/budget) | A bounded crawl visits in-scope pages up to depth/budget | Phase 1 | cc:TODO |
| 3.2 | robots.txt + politeness + honest UA | robots respected; configurable delay; identifiable UA | 3.1 | cc:TODO |
| 3.3 | Auth & session (cookies, headers, storage-state) | A behind-auth page is captured given supplied session state | 1.4 | cc:TODO |
| 3.4 | Content-addressed cache (hash → result) + conditional requests (ETag/IMS) | Re-capture uses cache / conditional GET; unchanged pages skipped | 3.1 | cc:TODO |
| 3.5 | Crawl store / index | Crawl results persisted and queryable | 3.1 | cc:TODO |
| 3.6 | Incremental crawl (content-hash + conditional GET) | Re-run returns only changed pages | 3.4 | cc:TODO |
| 3.7 | Change detection / diff feed | Diff between two captures of the same URL is produced | 3.4 | cc:TODO |
| 3.8 | Sitemap.xml ingestion | Sitemap URLs seed a crawl | 3.1 | cc:TODO |
| 3.9 | Browser sandboxing / process isolation | Renders run sandboxed per platform defaults | 1.4 | cc:TODO |
| 3.10 | Secrets handling for auth (never logged) | Auth secrets never appear in logs/traces | 3.3 | cc:TODO |

## Phase 4: v0.4 — Structured extraction (P1)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 4.1 | Schema-based structured extraction (external model, BYO) | A schema returns structured JSON via the caller's own model endpoint | Phase 1 | cc:TODO |
| 4.2 | Natural-language extraction | An NL instruction returns structured output | 4.1 | cc:TODO |
| 4.3 | Structured JSON output format | `Snapshot` exposes/saves the structured result | 4.1 | cc:TODO |
| 4.4 | Provenance map (fact → DOM node + screenshot region + URL) | Each extracted field carries a verifiable anchor | 4.1, 1.7 | cc:TODO |
| 4.5 | Deduplication | Repeated/boilerplate fragments removed from output | 1.5 | cc:TODO |

## Phase 5: v0.5 — Archive & fidelity (P1)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.1 | MHTML emitter (`Page.captureSnapshot`) | `--mhtml` writes a faithful `.mhtml` bundle | 1.4 | cc:WIP |
| 5.2 | Single-file inlined HTML (MHTML → `data:` URIs + inlined `<style>`) | `--html` writes a self-contained `.html` that opens offline | 5.1 | cc:WIP |
| 5.3 | WARC emitter (network recording before navigate) | `--warc` writes a valid WARC | 1.4 | cc:TODO |
| 5.4 | WACZ emitter (replayable) | `--wacz` round-trips as a replayable archive | 5.3 | cc:TODO |
| 5.5 | PDF export (`Page.printToPDF`) | `--pdf` writes a `.pdf` of the rendered page | 1.4 | cc:TODO |
| 5.6 | Accessibility-tree bundle | A11y tree exposed/saved for grounding | 1.4 | cc:TODO |
| 5.7 | Reproducible captures (pinned browser) | Same input + pinned browser → byte-stable-enough output for evals | 0.3 | cc:TODO |

## Phase 6: v0.6 — Bindings (P1/P2)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 6.1 | UniFFI bindings — Python first (maturin → PyPI) | `pip install amber-html` gives `amber.snapshot(url)` | Phase 1 | cc:TODO |
| 6.2 | C ABI (cbindgen) + long-tail wrappers | A C header + shared lib expose the core; one long-tail wrapper builds | 6.1 | cc:TODO |
| 6.3 | Node bindings (napi-rs) | `require('amber')` snapshots a URL | 6.1 | cc:TODO |
| 6.4 | Self-healing selectors | Extraction recovers when a selector drifts | 4.1 | cc:TODO |
| 6.5 | Chunking / summarization for downstream consumers | Output chunked with stable boundaries | 1.5 | cc:TODO |
| 6.6 | Pagination / infinite-scroll | Paginated content captured across pages | 2.7 | cc:TODO |
| 6.7 | Packaging (cargo / PyPI / npm / Homebrew / Docker / binaries) | Installable from each listed channel | 6.1, 6.2, 6.3 | cc:TODO |

## Phase 7: v0.7 — Scale & ops (P2)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 7.1 | Browser/tab pooling & reuse | Concurrent captures reuse a bounded pool | Phase 1 | cc:TODO |
| 7.2 | HTTP / daemon mode | A daemon serves concurrent capture requests | 7.1 | cc:TODO |
| 7.3 | Metrics (latency, render rate, cache hit) | Metrics exported and observable | 1.15 | cc:TODO |
| 7.4 | Resource limits | Per-capture memory/CPU/time caps enforced | 7.1 | cc:TODO |
| 7.5 | Local semantic memory (embeddings, offline search) | Captured corpus is searchable offline | 3.5 | cc:TODO |
| 7.6 | Dataset export (JSONL / parquet) | Crawl store exports to JSONL/parquet | 3.5 | cc:TODO |
| 7.7 | Scheduling / recurring captures | A schedule re-captures on cadence | 3.1 | cc:TODO |

## Phase 8: v1.0 — Polish & trust (P1/P2)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8.1 | Documentation (API + CLI + guides) | Public API and CLI documented; quickstart works | Phase 1 | cc:TODO |
| 8.2 | API stability pass | Public API reviewed and frozen for 1.0 | 8.1 | cc:TODO |
| 8.3 | Headed / stealth mode | Headed render available as an escalation | 1.4 | cc:TODO |
| 8.4 | Bring-your-own-proxy hook | A user-supplied proxy is used for fetches/renders | 1.1, 1.4 | cc:TODO |
| 8.5 | Reproducibility guarantees | Documented, tested reproducibility contract | 5.7 | cc:TODO |
| 8.6 | Broad packaging GA | Stable releases on all packaging channels | 6.7 | cc:TODO |

## Post-1.0: Specialized modes (P2)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 9.1 | Tamper-evident evidence capture (hash + timestamp + signature) | Capture produces a verifiable, signed snapshot | Phase 5 | cc:TODO |
| 9.2 | Provenance-tagged corpus builder | Bulk captures emit a provenance-tagged dataset | 4.4, 7.6 | cc:TODO |
| 9.3 | Visual regression / page monitoring | Scheduled captures flag visual/content changes | 3.7, 7.7 | cc:TODO |

---

## Open implementation questions

1. Markdown converter crate (`htmd`/`html2md`/`mdka`) and readable extractor
   crate (`dom_smoothie`/`readability`).
2. MCP tool surface — how many tools and their schemas (Phase 2).
3. Token counting — which tokenizer(s) to report against (Phase 2).
4. Structured-extraction model interface — OpenAI-compatible endpoint? local
   model? both? (Phase 4).
5. Crawl/cache storage — embedded (SQLite/sled) vs flat files (Phase 3).
6. Semantic memory — in `amber-core` (v0.7) or a separate companion crate?
