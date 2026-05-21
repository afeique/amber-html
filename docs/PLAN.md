# AmberHTML — Product & Engineering Plan

> **Status:** Design phase (pre-alpha). Last updated 2026-05-21.
> **This is the single source of truth** for AmberHTML's strategy, decisions,
> features, CLI, and roadmap. It is intentionally comprehensive; the live
> execution task list is derived from it per phase.

---

## Table of contents

1. [Vision](#1-vision)
2. [Positioning & strategy](#2-positioning--strategy)
3. [Competitive landscape](#3-competitive-landscape)
4. [Target users & use cases](#4-target-users--use-cases)
5. [Language choice & rationale](#5-language-choice--rationale)
6. [Architecture](#6-architecture)
7. [Capture pipeline & tiered-fetch detection](#7-capture-pipeline--tiered-fetch-detection)
8. [Output formats & policy](#8-output-formats--policy)
9. [CLI specification](#9-cli-specification)
10. [Bindings & language ergonomics](#10-bindings--language-ergonomics)
11. [Feature catalog](#11-feature-catalog)
12. [Roadmap & milestones](#12-roadmap--milestones)
13. [Locked technical decisions](#13-locked-technical-decisions)
14. [Risks & mitigations](#14-risks--mitigations)
15. [Success metrics](#15-success-metrics)
16. [Open questions](#16-open-questions)

---

## 1. Vision

**AmberHTML is the open, local-first AI web-reading engine.** It faithfully renders
any web page in a *real local browser* — but only when a page actually needs it —
and then emits the representations the user asks for (HTML, MHTML, Markdown,
readable text, WARC, WACZ, screenshot, PDF) from a single pass, with provenance,
under a token budget, model-agnostic, shipped as a library + CLI + MCP server.

The name is the spec: like amber preserving a whole organism intact, AmberHTML
freezes a fully-rendered page as a faithful, inspectable specimen.

**One-line pitch:**
> *"Firecrawl that runs on your own machine — free, private, reaches your internal
> apps, costs nothing per page, and tells you the token cost and where every fact
> came from."*

### What it is
- A local, embeddable engine for turning live web pages into AI-ready data.
- A library (Rust core + idiomatic bindings to many languages), a CLI, and an MCP server.

### What it is *not*
- Not a cloud SaaS, not a per-page metered API.
- Not an industrial adversarial-scraping / proxy-rotation product (see §2, "where not to fight").

---

## 2. Positioning & strategy

### The core insight
Firecrawl/Jina are powerful but their worst problems are **structural consequences
of being a paid cloud API** — not bugs. A local-first engine attacks exactly those,
and they cannot copy the response without dismantling their business model.

### Differentiation in three tiers

**Tier 1 — wins competitors structurally cannot copy (cloud-SaaS-bound):**
- **Free & local** — no per-page billing, no credit math, no charge for failed requests.
- **Private/sovereign by construction** — URLs and data never leave the machine; runs airgapped. The enterprise unlock for compliance/legal/healthcare/finance.
- **Reaches what cloud can't** — `localhost`, intranet, VPN, behind-auth/session pages.
- **Self-host as a first-class citizen**, not a crippled tier.

**Tier 2 — efficiency moat (where Rust earns its keep):**
- **Tiered fetching** — cheap HTTP fetch first, escalate to a browser only when JS is required (see §7).
- **Render once, emit everything** — one browser pass → all requested formats, never re-render.
- **Token-budget-aware output** — main-content extraction + boilerplate removal, with measured token count returned.
- **Caching + incremental** — content-hash + conditional GET; skip unchanged pages, return only diffs.
- **Efficient browser/tab pooling** — lower memory/latency than a Node browser farm.

**Tier 3 — ride the 2026 wave (model-agnostic / BYO-LLM):**
- Schema/natural-language **structured extraction** built in (vs. Firecrawl's separate paid Extract).
- **Self-healing extraction** — re-map when layouts shift.
- **Agent-native MCP primitives** — navigate/click/paginate with a budget.
- **Provenance / citeable extraction** — every fact anchors to a DOM node + screenshot region + URL, for LLM grounding.

### Where *not* to fight
Adversarial mass-scraping of Cloudflare-Turnstile-class sites needs rotating proxy
pools and IP infrastructure — that is cloud's home turf and a different (sketchier)
business. **Do not build a proxy empire.** Counter-intuitively, a *real local
browser in headed mode is more stealthy* than a detectable cloud headless farm, so
for "an agent reads a handful of real pages" we are often *better*. Position for:
agent-reading, research, internal/private data, training-corpus building. Offer a
bring-your-own-proxy hook and stop there.

### Moats, ranked
1. **Privacy/locality** (structural, uncopyable).
2. **Cost** (structural).
3. **Efficiency/architecture** (tiered fetch, render-once) — hard to retrofit.
4. **Provenance** (few do it well; high value for grounding).
5. **Becoming the open standard** — the real long-term prize for a free tool.

---

## 3. Competitive landscape

| Tool | Shape | Strength | Where we differ |
|---|---|---|---|
| **Firecrawl** | Cloud SaaS + MCP | Mature, clean markdown, anti-bot at scale | Local/free/private; reaches internal pages; no per-page cost |
| **Jina Reader** | Cloud (`r.jina.ai`) | Frictionless, cheap/free tier | Local/private; structured + screenshots + provenance |
| **Crawl4AI / ScrapeGraphAI** | OSS Python | NL/schema extraction, agentic | Rust core (faster/leaner); local-first; render-once; MCP-native |
| **SingleFile** | Browser ext + CLI (AGPL) | Best-in-class faithful single-file HTML | We target AI-readable output + agents, not just offline HTML |
| **monolith** | Rust CLI/lib (no JS engine) | Lightweight single-file HTML | We render JS natively; emit many formats |
| **Browsertrix (Webrecorder)** | Node + Docker | High-fidelity WARC, institutional | Single native binary, embeddable, lighter |

**Takeaway:** the *intersection* — Rust core + local-first + render-once-emit-everything
(HTML/MHTML/Markdown/readable/WARC/WACZ/screenshot) + provenance + MCP + many
bindings — is empty. That intersection is the product.

---

## 4. Target users & use cases

**Primary (lead here):**
- **AI agent / app developers** who need to feed live web content to an LLM, locally and cheaply (RAG, research agents, browser-using agents).
- **Privacy/compliance-bound orgs** (legal, healthcare, finance, gov) that cannot send URLs/data to a third party.
- **Internal-tooling builders** who must read `localhost`/intranet/behind-auth pages.

**Secondary:**
- **Researchers / data scientists** building clean, provenance-tagged corpora/datasets.
- **Archivists / hobbyists** wanting lightweight standards-compliant capture (WARC/WACZ) without Docker.

**Tertiary / optional modes:**
- **Compliance/legal evidence capture** (tamper-evident snapshots).
- **Page-change monitoring**.

---

## 5. Language choice & rationale

**Decision: Rust** for `amber-core` and all first-party components.

### What drives the choice
The workload is: orchestrate an *external* Chromium over CDP (the heavy rendering is
Chromium's, not ours), do I/O-concurrent connection management, some CPU-bound HTML
post-processing — and, decisively, **be an embeddable native core** for thin bindings
to many languages, with lean memory atop an already-heavy browser and fast CLI startup.

### The decisive lens: embeddability
The binding requirement demands a **self-contained native library with a C ABI and no
required runtime/GC**. That single criterion eliminates or wounds most candidates.

| Language | Embeddable core (C ABI, no runtime) | Runtime speed | Memory | Concurrency | Dev speed | Adoptability | Memory safety |
|---|---|---|---|---|---|---|---|
| **Rust** | ✅ best-in-class | ✅ native (≈C) | ✅ no GC, minimal | ✅ async/tokio, race-free | 🟧 steeper | ✅ growing; MIT/Apache norm | ✅ |
| **C/C++** | ✅ native | ✅ native | ✅ minimal | 🟧 manual | ❌ slow & dangerous | 🟧 universal, high friction | ❌ |
| **Go** | 🟧 `c-shared` drags Go runtime | ✅ fast (GC pauses) | 🟧 GC | ✅ goroutines (simplest) | ✅ fast | ✅ strong | ✅ (GC) |
| **Node/TS** | ❌ ships V8 | 🟧 JIT, single loop | ❌ heavy | 🟧 loop + workers | ✅ fastest | ✅ huge | ✅ (GC) |
| **Python** | ❌ it's the host | ❌ slow, GIL | ❌ high | 🟧 asyncio/GIL | ✅ fastest | ✅ huge (AI) | ✅ (GC) |
| **Java/JVM** | ❌ JVM (GraalVM partial) | ✅ JIT after warmup | ❌ heavy heap | ✅ threads/virtual | 🟧 verbose | ✅ enterprise | ✅ (GC) |

### Reads
- **Node/TS** — most mature CDP ecosystem (Puppeteer/Playwright/SingleFile/Firecrawl), but can't be a thin embeddable core, has no efficiency edge, and competes on the incumbents' home turf. Great *binding target*, wrong core.
- **Python** — irrelevant as a core (GIL, slow, heavy, it's the *host*). The fastest Python tools (Polars, Pydantic-core, ruff, uv, tokenizers) are Rust under the hood. Top binding target.
- **Java/JVM** — heavy heap, slow startup, poor embedding — all our sensitive axes.
- **C/C++** — matches Rust on embeddability + speed, but slow/dangerous to write and a memory-safety liability when parsing hostile HTML; fragmented tooling. Rust dominates it here.
- **Go** — the strongest alternative (fast dev, goroutines, single binary, `chromedp`). If we ever drop multi-language bindings and ship only server+CLI, **revisit Go**. It loses today because `-buildmode=c-shared` drags the Go runtime/GC/scheduler into the shared lib with cgo boundary costs — a poor embeddable core.

### Verdict
Rust uniquely satisfies all three non-negotiables at once: **embeddable C-ABI core (no GC)** + **C-class speed & low/deterministic memory** + **memory safety & fearless async concurrency**, with modern tooling (cargo/serde/tokio/html5ever). AmberHTML is textbook-shaped for the proven "fast Rust core + thin bindings everywhere" pattern. Accepted costs: steeper learning curve, slower initial dev, longer compiles.

**Reconsider trigger:** if multi-language bindings are dropped (server+CLI only), revisit Go.

---

## 6. Architecture

```
                  ┌──────────────────────────────────────────────┐
  Interfaces      │ CLI · MCP server · HTTP/daemon · lang bindings │
                  └──────────────────────────────────────────────┘
                                     │  blocking public API
                  ┌──────────────────────────────────────────────┐
  amber-core      │  Orchestrator                                  │
  (Rust, async    │   ├─ Fetch strategy: HTTP-first → escalate     │
   inside)        │   ├─ Settle engine (lifecycle/idle/fonts/...)  │
                  │   ├─ Capture pass (single render)              │
                  │   ├─ Extraction (markdown/readable/schema/LLM) │
                  │   ├─ Emitters (html/mhtml/md/txt/warc/wacz/...)│
                  │   ├─ Provenance map                            │
                  │   └─ Cache + crawl store                       │
                  │  CdpTransport trait (test-mock seam only)      │
                  └──────────────────────────────────────────────┘
                                     │
                  hand-rolled WebSocket CDP client (the ONLY transport)
                                     │
                       managed, pinned Chrome for Testing
```

**Layers:**
- **`amber-core`** — single source of truth; async internally (tokio), **blocking public API** so FFI stays simple.
- **Transport** — a **single hand-rolled WebSocket CDP client** (chromiumoxide is *not* used; see §13). The `CdpTransport` trait exists only as a thin seam for test mocking.
- **Browser management** — auto-download a pinned [Chrome for Testing](https://googlechromelabs.github.io/chrome-for-testing/) build, checksum-verified, cached; `AMBER_CHROMIUM_PATH` escape hatch. **Always required** — CDP drives a real browser; nothing bundles one.
- **Bindings** — UniFFI (Python/Swift/Kotlin/Ruby) and a C ABI (cbindgen) for the long tail; napi-rs for Node; all thin, idiomatic facades over `amber-core` (see §10).

---

## 7. Capture pipeline & tiered-fetch detection

The pipeline is **cheap-first, escalate-on-insufficiency, output-aware, and
correctness-biased.**

```
1. OUTPUT GATE   ─ does a requested output inherently need a browser?
2. HTTP FETCH    ─ plain GET + parse static HTML (cheap, ~ms)
3. SUFFICIENCY   ─ score: is the content actually present?
4. ESCALATE      ─ uncertain/insufficient → render in a browser
5. SETTLE        ─ (browser path) wait until the page is truly done
6. CAPTURE       ─ single pass; emit only the requested formats
7. MEMOIZE       ─ cache the browser-vs-static verdict per domain/pattern
```

### Step 1 — Output gate
`--screenshot`, `--pdf`, `--mhtml`, high-fidelity `--warc`/`--wacz`, and anything
requiring rendered JS ⇒ **browser always; skip detection.** Only `--markdown`,
`--readable`, and plain `--html` on a possibly-static page are cheap-path candidates.

### Step 3 — Sufficiency scoring (the meat)
**Hard "needs browser":**
- `<noscript>` "please enable JavaScript" text.
- Empty app-shell: a framework root (`#root`/`#app`/`#__next`/`<app-root>`/`__nuxt`) with only `<script>` content.
- Visible text below a content floor (~500 chars), skeleton/loading markers, meta/JS redirects.

**Hard "static is fine":**
- The main-content extractor yields a confident, lengthy result on the static HTML.
- High text-to-markup ratio.

**Crucial nuance:** framework markers *alone* ≠ needs browser. Next/Nuxt pages are
often server-rendered or embed full content as JSON (`__NEXT_DATA__`, JSON-LD) — the
static fetch already has it. Always combine marker detection with an actual-content
check. (Future fast-path: harvest `__NEXT_DATA__`/JSON-LD directly.)

### Step 4 — Bias when uncertain → render
A wrong "static is fine" silently produces an empty/incomplete result (bad — data
loss); a wrong "needs browser" only costs time/CPU (safe). **The gray zone always
falls back to rendering.** Correctness beats speed at the margin.

### Step 5 — Settle engine (browser path)
"Settle" decides the page is *done* before capture (capture freezes the current DOM).
Configurable policy composing: lifecycle events (`load`, `networkIdle`), in-flight
request count, settle delay, `document.fonts.ready`, auto-scroll for lazy-load, and an
optional caller `--wait-for` selector / JS predicate. Default ≈ `load` +
`networkAlmostIdle` + ~200 ms + `fonts.ready`. This is where "fully/correctly rendered"
is won or lost — a first-class, tested module, not a hardcoded sleep.

### Overrides & honest limit
Overrides: `--render auto|always|never`, `--min-content <N>`, `--wait-for <selector>`.
**Limitation:** the heuristic detects *"is the page rendered,"* not *"is the specific
datum you want present."* Late-XHR content needs `--render always` or `--wait-for`.
Document this clearly.

---

## 8. Output formats & policy

**Policy: NO default output.** The user must explicitly request ≥1 format; zero
outputs is a hard error. (This replaces any notion of a "default extractor.")

| Flag | Format | Ext | Needs browser? | Notes |
|---|---|---|---|---|
| `--html` | Single-file inlined HTML | `.html` | usually | From MHTML → `data:` URIs + inlined `<style>` |
| `--mhtml` | MHTML bundle | `.mhtml` | yes | `Page.captureSnapshot`; faithful baseline |
| `--markdown` | Clean Markdown | `.md` | maybe | HTML→MD converter (candidates: `htmd`/`html2md`/`mdka`) |
| `--readable` | Readable plain text | `.txt` | maybe | Main-content extractor (candidates: `dom_smoothie`/`readability`) |
| `--warc` | WARC archive | `.warc` | yes (hi-fi) | Network recording enabled before navigate |
| `--wacz` | WACZ (replayable) | `.wacz` | yes (hi-fi) | Uses `warc`/`wacksy` crates |
| `--screenshot` | Full-page PNG | `.png` | yes | `Page.captureScreenshot` |
| `--pdf` | PDF | `.pdf` | yes | `Page.printToPDF` |

**"No default" ≠ "no extraction code":** the faithful captures (HTML/MHTML/WARC/WACZ)
need no content algorithm, but Markdown and readable text still require real
implementations *when requested*.

**Render once, emit everything:** a single capture pass produces all requested
formats; the requested **output set configures the pass** (e.g. WARC/WACZ enable
network recording up front; MHTML triggers `captureSnapshot` post-settle). The output
set also feeds tiered-fetch (§7): static page + Markdown-only ⇒ no browser.

---

## 9. CLI specification

```
amber <URL> [OUTPUT FLAGS] [OPTIONS]

OUTPUT SELECTION  (at least one required — no default)
  --html         single-file inlined HTML        → <name>.html
  --mhtml        MHTML bundle                     → <name>.mhtml
  --markdown     clean Markdown                   → <name>.md
  --readable     readable plain text              → <name>.txt
  --warc         WARC archive                     → <name>.warc
  --wacz         WACZ archive                     → <name>.wacz
  --screenshot   full-page PNG                    → <name>.png
  --pdf          PDF                              → <name>.pdf
  (no --all — select formats precisely)

NAMING / LOCATION
  -o, --output-dir <DIR>   where to write (default: current dir; created if missing)
  -n, --name <NAME>        base filename, no extension; REQUIRES an arg when given
                           omitted → "<safe-url> <YYYY-MM-DD> <HH-MM-SS>" (local time)

RENDERING
  --render auto|always|never   (default auto — tiered fetch)
  --wait-for <SELECTOR>
  --min-content <N>
```

**Example:**
```
amber https://example.com -o ./my-output-dir --html --mhtml
  → "./my-output-dir/example.com 2026-05-21 14-30-05.html"
  → "./my-output-dir/example.com 2026-05-21 14-30-05.mhtml"
```

**Rules:**
- Format flags are **boolean**; destination = `-o` dir, basename = `-n` or the default name, extension derived from format.
- `-o` defaults to the current directory and is **created if missing**.
- `-n` is optional but **requires an argument when present**.
- **Default name** (no `-n`): `<safe-url> <YYYY-MM-DD> <HH-MM-SS>` in **local time**, e.g. `example.com-blog-rust 2026-05-21 14-30-05`. `<safe-url>` = the fetched URL made filesystem-safe (scheme dropped; chars outside `[A-Za-z0-9._-]` → `-`; consecutive separators collapsed; trailing slash dropped; truncated to ~120 chars). Time uses `HH-MM-SS` because colons are illegal on Windows. *(Re-capturing the same URL later yields a new timestamp — the datetime is what keeps repeated captures distinct, since long URLs are truncated.)*
- **No `--all`** — the user selects formats precisely. **≥1 output flag required**, else a hard error.

---

## 10. Bindings & language ergonomics

**Requirement:** bindings must be **easy, idiomatic, language-specific utilities — not
raw FFI mirrors.** A single capture produces a `Snapshot` object; each language exposes
ergonomic save/get methods on it.

```python
# Python
import amber
snap = amber.snapshot("https://example.com")     # one call
snap.save_html("page.html")
snap.save_mhtml("page.mhtml")
snap.save_markdown("page.md")
md   = snap.markdown()
text = snap.readable()
```
```rust
// Rust
let snap = amber::snapshot("https://example.com").await?;
snap.save_html("page.html")?;
let md = snap.markdown()?;
```

Same shape in Node/Ruby/Go/etc. — idiomatic per language. Design principle: **`Snapshot`
is the central object; format methods hang off it**, and the binding layer is a thin
facade over the C-ABI/UniFFI marshalling. Calling with no output requested is an error
(consistent with §8). **Stack:** `amber-core` (truth) → `amber-uniffi`
(Python/Swift/Kotlin/Ruby) + `amber-capi` (C ABI → long tail) + napi-rs (Node). Public
FFI surface is **blocking** (tokio hidden inside core). **WASM rejected** — it can't
drive a native browser process.

---

## 11. Feature catalog

Legend — Priority: **P0** (MVP-critical) · **P1** (core) · **P2** (later/optional). Phase column maps to §12.

### A. Fetch & render engine
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Tiered fetch (HTTP-first, escalate) — see §7 | P0 | v0.1 | Biggest efficiency lever |
| Hand-rolled WebSocket CDP client | P0 | v0.1 | The only transport |
| Managed pinned Chrome for Testing (download/cache/verify) | P0 | v0.1 | Reproducibility |
| Headless (`--headless=new`) render | P0 | v0.1 | |
| Settle engine (lifecycle/network-idle/fonts/settle-delay) | P0 | v0.1 | Crux of fidelity |
| Auto-scroll for lazy-load | P1 | v0.2 | |
| Custom ready signal (`--wait-for`) | P1 | v0.2 | SPA support |
| Viewport/device/locale/timezone/dark-mode emulation | P1 | v0.2 | |
| Auth & session (cookies, headers, storage-state) | P1 | v0.3 | Behind-auth reach |
| Headed/stealth mode | P2 | v1.0 | More stealthy than cloud headless |
| Resource blocking (ads/trackers/images optional) | P1 | v0.2 | Speed/token savings |
| Browser/tab pooling & reuse | P1 | v0.7 | Scale |
| Timeout/retry/partial-result handling | P0 | v0.1 | |
| Process lifecycle / reconnection / crash recovery | P0 | v0.1 | We own this (no chromiumoxide) |

### B. Output representations (render-once-emit-everything) — see §8
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Clean LLM-optimized Markdown | P0 | v0.1 | |
| Readable plain text (main-content) | P0 | v0.1 | |
| Single-file inlined HTML | P1 | v0.5 | From MHTML transform |
| MHTML (`Page.captureSnapshot`) | P1 | v0.5 | Faithful baseline |
| Full-page & element screenshots | P0 | v0.1 | |
| PDF export | P2 | v0.5 | |
| WARC / WACZ (replayable) | P1 | v0.5 | `warc`/`wacksy` |
| Page metadata (title/lang/canonical/OpenGraph/links) | P0 | v0.1 | |
| Accessibility tree bundle | P1 | v0.5 | Vision/computer-use grounding |
| Structured JSON (schema-driven) | P1 | v0.4 | See §C |
| Provenance map (fact → DOM node + screenshot region + URL) | P1 | v0.4 | Grounding moat |
| URL+datetime default naming | P0 | v0.1 | CLI default when `-n` omitted |

### C. Extraction intelligence
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Boilerplate/nav/ad/cookie-banner removal | P0 | v0.1 | |
| Token-budget-aware output + measured token count | P1 | v0.2 | |
| Deduplication | P2 | v0.3 | |
| Schema-based structured extraction (BYO-LLM) | P1 | v0.4 | Model-agnostic |
| Natural-language extraction | P1 | v0.4 | |
| Self-healing selectors | P2 | v0.6 | |
| Chunking/summarization for RAG | P2 | v0.6 | |
| Language detection / encoding | P1 | v0.2 | |

### D. Crawling & navigation
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Single-page capture | P0 | v0.1 | |
| Multi-page crawl (link follow, scope, depth/budget) | P1 | v0.3 | |
| Sitemap.xml ingestion | P2 | v0.3 | |
| robots.txt + politeness + honest UA | P1 | v0.3 | Trust moat |
| Pagination / infinite-scroll | P2 | v0.6 | |
| Agent-native actions (navigate/click/fill/scroll/wait) | P1 | v0.2 | MCP primitives |
| Incremental crawl (content-hash + conditional GET) | P1 | v0.3 | |
| Change detection / diff feed | P2 | v0.3 | |
| Scheduling / recurring | P2 | v0.7 | |

### E. Caching, storage & memory
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Content-addressed cache (hash → result) | P1 | v0.3 | Internal content hash (≠ output filename) |
| Conditional requests (ETag / If-Modified-Since) | P1 | v0.3 | |
| Crawl store/index | P1 | v0.3 | |
| Local semantic memory (embeddings, offline search) | P2 | v0.7 | |
| Dataset export (JSONL / parquet) | P2 | v0.7 | |

### F. Interfaces & distribution — see §9, §10
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Rust library (`amber-core`) | P0 | v0.1 | Primary API |
| CLI (`amber`) | P0 | v0.1 | §9 |
| MCP server | P0 | v0.2 | Headline agent interface |
| HTTP / daemon mode | P2 | v0.7 | |
| UniFFI bindings (Python first) | P1 | v0.6 | maturin → PyPI |
| C ABI (cbindgen) + long-tail wrappers | P1 | v0.6 | |
| Node bindings (napi-rs) | P2 | v0.6 | |
| Packaging: cargo/PyPI/npm/Homebrew/Docker/binaries | P1 | v0.6+ | |

### G. Reliability, observability & ops
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Structured logging + tracing | P0 | v0.1 | |
| Token accounting & cost reporting | P1 | v0.2 | |
| Metrics (latency, render rate, cache hit) | P2 | v0.7 | |
| Reproducible captures (pinned browser) | P1 | v0.5 | Evals/datasets |
| Resource limits | P1 | v0.7 | |

### H. Security & privacy
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Local-first, zero telemetry by default | P0 | v0.1 | |
| Airgapped operation | P0 | v0.1 | After browser cached |
| Browser sandboxing / process isolation | P1 | v0.3 | |
| Secrets handling for auth (never logged) | P1 | v0.3 | |
| Bring-your-own-proxy hook | P2 | v1.0 | No proxy empire |

### I. Optional / specialized modes (future)
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Tamper-evident evidence capture (hash + timestamp + signature) | P2 | post-1.0 | Compliance/legal |
| AI training-corpus builder (provenance-tagged) | P2 | post-1.0 | |
| Visual regression / page monitoring | P2 | post-1.0 | |

---

## 12. Roadmap & milestones

Each phase ends with a working, demonstrable artifact. Sequenced to prove the thesis
early and de-risk the hardest parts first.

### Phase 0 — Foundation *(in progress)*
Repo, license, CI, Cargo workspace, Chromium fetcher, hand-rolled CDP client,
`navigate` + screenshot smoke test.
**Done when:** `cargo run` launches pinned Chromium and screenshots a URL.

### Phase 1 — v0.1 "Reader MVP" *(thesis proof)*
Tiered fetch → settle → render → **Markdown + readable + screenshot + content-MD5
naming**, via the **CLI** (§9). Single-shot capture.
**Done when:** `amber <url> -o ./out --markdown --readable --screenshot` works, using
HTTP-only when possible and a browser only when needed.

### Phase 2 — v0.2 "Agent-native"
**MCP server**; **token-budget output** + token accounting; resource blocking;
agent-action primitives; emulation knobs.
**Done when:** an MCP client can read a page within a token budget.

### Phase 3 — v0.3 "Crawl"
Multi-page crawl, **robots.txt + politeness**, auth/session, **caching + incremental**,
crawl store, change diffs.
**Done when:** a bounded, polite, incremental crawl returns only changed pages on re-run.

### Phase 4 — v0.4 "Structured & Smart"
**Schema/NL structured extraction (BYO-LLM)**, **provenance anchoring**.
**Done when:** schema extraction returns structured JSON with per-field provenance via the user's own LLM.

### Phase 5 — v0.5 "Archive & fidelity"
**WARC/WACZ**, MHTML, single-file HTML, PDF, accessibility-tree bundle, reproducible captures.
**Done when:** a capture round-trips as a replayable WACZ and a portable single-file HTML.

### Phase 6 — v0.6 "Bindings"
**UniFFI (Python via maturin)** first, then **C ABI**; Node via napi-rs; self-healing extraction.
**Done when:** `pip install amber-html` gives a Python user `amber.snapshot(url)`.

### Phase 7 — v0.7 "Scale & ops"
Browser/tab pooling, concurrency, **daemon/HTTP mode**, metrics, semantic memory, dataset export.
**Done when:** a daemon serves concurrent reads with bounded memory and a warm cache.

### Phase 8 — v1.0 "Polish & trust"
Docs, stability, headed/stealth mode, BYO-proxy hook, reproducibility guarantees, broad packaging.
**Done when:** documented, stable API; installable via cargo/PyPI/npm/Homebrew/Docker.

### Post-1.0 — Specialized modes
Evidence capture, training-corpus builder, page monitoring (as demand warrants).

---

## 13. Locked technical decisions

- **Language:** Rust (§5). `amber-core` async inside, **blocking public API**.
- **CDP transport:** a **single hand-rolled WebSocket CDP client**. **chromiumoxide is NOT used** — not even as a dev/test dependency. No user-facing scenario prefers it; its only advantages were dev-side (faster MVP, full CDP coverage, a differential test oracle, pre-solved lifecycle/reconnection), all deliberately forgone to build a lean, controlled core. Implement only the ~20–40 CDP messages we use. **Correctness oracle = the real pinned browser (integration tests) + golden-file outputs.** `CdpTransport` trait kept only as a test-mock seam. *(Future hardening, parked: `--remote-debugging-pipe` over stdio vs the WebSocket TCP port.)*
- **Browser:** always required; **managed, pinned Chrome for Testing**, checksum-verified, cached; `AMBER_CHROMIUM_PATH` escape hatch.
- **HTML capture:** `Page.captureSnapshot` (MHTML) baseline; optional single-file-HTML transform.
- **Output policy:** **no default output**; explicit selection (§8); render-once-emit-everything; output set configures the pass.
- **CLI:** `-o` dir + boolean format flags + `-n` name (default `<safe-url> <date> <time>`); no `--all` (§9).
- **Bindings:** UniFFI + C ABI, both first-tier; idiomatic per-language `Snapshot` facade (§10); WASM rejected.
- **License:** dual **`MIT OR Apache-2.0`**.
- **Naming:** package `amber-html`; brand `AmberHTML`.
- **Workflow:** feature branch → PR into `main`; no direct pushes to `main`.

---

## 14. Risks & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Content-extraction quality lags Firecrawl | High | Lean on proven Readability algorithms; iterate on real pages; pluggable extractor |
| Fidelity gap on JS-heavy pages | Med | Strong settle engine + headed fallback; integration tests vs real browser |
| Owning CDP lifecycle/reconnection (no chromiumoxide) | Med | Narrow message surface; robust integration tests against the pinned browser |
| Multi-language binding maintenance burden | Med | Thin mechanical bindings; UniFFI for the curated tier |
| Anti-bot blocks (Cloudflare etc.) | Med | Don't compete at scale; headed/stealth + BYO-proxy hook for the few-pages case |
| Chromium version drift breaks CDP client | Low | Pin browser; codegen subset from that revision's protocol JSON; protocol-audit gate on bump |
| "Already done by SingleFile/monolith" perception | Med | Position as AI-reading engine for agents; lead with local-first + provenance + MCP |
| Memory/CPU at scale | Med | Tiered fetch; pooling; resource limits |
| Scope sprawl | High | Strict phase gates; each phase ships a usable artifact |

---

## 15. Success metrics

**Adoption (the real prize):** GitHub stars, crates.io/PyPI downloads, MCP-server installs, mentions in agent frameworks.

**Technical, per phase:**
- **Efficiency:** % of pages served HTTP-only (no browser); median read latency; memory/page.
- **Quality:** extraction fidelity vs a labeled set; token reduction vs raw HTML (target 70–90%).
- **Cost:** $0 per page (vs Firecrawl) — the headline comparison.
- **Reach:** can read `localhost`/intranet/auth pages competitors can't.

---

## 16. Open questions

**Implementation (decide per phase):**
1. Markdown converter crate (`htmd`/`html2md`/`mdka`) and readable extractor crate (`dom_smoothie`/`readability`).
2. MCP tool surface — how many tools (read/crawl/extract/screenshot) and their schemas (Phase 2).
3. Token counting — which tokenizer(s) to report against (Phase 2).
4. Structured-extraction LLM interface — OpenAI-compatible endpoint? local model? both? (Phase 4).
5. Crawl/cache storage — embedded (SQLite/sled) vs flat files (Phase 3).
6. Semantic memory — in `amber-core` (v0.7) or a separate companion crate?
