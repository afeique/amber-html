# AmberHTML — Product & Engineering Plan

> **Status:** Design phase (pre-alpha). Last updated 2026-05-21.
> This is the canonical strategy + feature + roadmap document. It is intentionally
> comprehensive; the live execution task list will be derived from it per phase.

---

## Table of contents

1. [Vision](#1-vision)
2. [Positioning & strategy](#2-positioning--strategy)
3. [Competitive landscape](#3-competitive-landscape)
4. [Target users & use cases](#4-target-users--use-cases)
5. [Architecture](#5-architecture)
6. [Feature catalog](#6-feature-catalog)
7. [Roadmap & milestones](#7-roadmap--milestones)
8. [Locked technical decisions](#8-locked-technical-decisions)
9. [Risks & mitigations](#9-risks--mitigations)
10. [Success metrics](#10-success-metrics)
11. [Open questions](#11-open-questions)

---

## 1. Vision

**AmberHTML is the open, local-first AI web-reading engine.** It faithfully renders
any web page in a *real local browser* — but only when a page actually needs it —
and then emits every representation an AI agent could want (Markdown, structured
JSON, screenshots, accessibility tree, WARC) from a single pass, with provenance,
under a token budget, model-agnostic, shipped as a library + MCP server.

The name is the spec: like amber preserving a whole organism intact, AmberHTML
freezes a fully-rendered page as a faithful, inspectable specimen.

**One-line pitch:**
> *"Firecrawl that runs on your own machine — free, private, reaches your internal
> apps, costs nothing per page, and tells you the token cost and where every fact
> came from."*

### What it is
- A local, embeddable engine for turning live web pages into AI-ready data.
- A library (Rust core + many language bindings), a CLI, and an MCP server.

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
- **Tiered fetching** — cheap HTTP fetch first, escalate to a browser only when JS is required. The single biggest lever for latency/CPU/memory.
- **Render once, emit everything** — one browser pass → all formats, never re-render.
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
(Markdown/structured/screenshot/WARC) + provenance + MCP + many bindings — is empty.
That intersection is the product.

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

## 5. Architecture

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
                  │   ├─ Extraction (content/markdown/schema/LLM)  │
                  │   ├─ Emitters (md/html/png/a11y/json/warc)     │
                  │   ├─ Provenance map                            │
                  │   └─ Cache + crawl store                       │
                  │  CdpTransport trait (swappable)                │
                  └──────────────────────────────────────────────┘
                                     │
              native WS CDP (default)  │  chromiumoxide (optional / oracle)
                                     │
                       managed, pinned Chrome for Testing
```

**Layers:**
- **`amber-core`** — single source of truth; async internally (tokio), **blocking public API** so FFI stays simple.
- **Transport** — `CdpTransport` trait; default `transport-native` (hand-rolled WebSocket CDP), optional `transport-chromiumoxide` (escape hatch + differential test oracle), feature-gated.
- **Browser management** — auto-download a pinned [Chrome for Testing](https://googlechromelabs.github.io/chrome-for-testing/) build, checksum-verified, cached; `AMBER_CHROMIUM_PATH` escape hatch.
- **Bindings** — UniFFI (Python/Swift/Kotlin/Ruby) and a C ABI (cbindgen) for the long tail; napi-rs for Node; all thin mechanical mappings of `amber-core`.

---

## 6. Feature catalog

Legend — Priority: **P0** (MVP-critical) · **P1** (core) · **P2** (later/optional). Phase column maps to §7.

### A. Fetch & render engine
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Tiered fetch (HTTP-first, escalate to browser) | P0 | v0.1 | Detect JS-dependence; biggest efficiency lever |
| Managed pinned Chrome for Testing (download/cache/verify) | P0 | v0.1 | Reproducibility |
| Headless (`--headless=new`) render via CDP | P0 | v0.1 | Native transport |
| Settle engine (lifecycle, network-idle, fonts.ready, settle delay) | P0 | v0.1 | Crux of fidelity |
| Auto-scroll for lazy-load | P1 | v0.2 | Part of settle |
| Custom ready signal (`wait_for` selector / JS predicate) | P1 | v0.2 | SPA support |
| Viewport / device / locale / timezone / dark-mode emulation | P1 | v0.2 | `Emulation.*` |
| Auth & session (cookies, headers, storage-state injection) | P1 | v0.3 | Behind-auth reach |
| Headed/stealth mode for fragile sites | P2 | v1.0 | More stealthy than cloud headless |
| Resource blocking (ads/trackers/images optional) | P1 | v0.2 | Speed/token savings |
| Browser/tab pooling & reuse | P1 | v0.7 | Scale |
| Timeout / retry / partial-result handling | P0 | v0.1 | Robustness |

### B. Output representations (render-once-emit-everything)
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Clean LLM-optimized Markdown | P0 | v0.1 | Headline output |
| Main-content extraction (Readability-style) | P0 | v0.1 | 60–80% token cut |
| Cleaned/normalized HTML | P1 | v0.2 | |
| Full-page & element screenshots (PNG/JPEG/WebP) | P0 | v0.1 | Vision models |
| Page metadata (title/lang/canonical/OpenGraph/links) | P0 | v0.1 | |
| Accessibility tree bundle | P1 | v0.5 | Computer-use/vision grounding |
| Structured JSON (schema-driven) | P1 | v0.4 | See §C |
| PDF export | P2 | v0.5 | `Page.printToPDF` |
| MHTML (`Page.captureSnapshot`) | P1 | v0.5 | Faithful archival baseline |
| Single-file HTML (data-URI inlined, from MHTML) | P2 | v0.5 | Portable; transform tax |
| WARC / WACZ (replayable) | P1 | v0.5 | Uses `warc`/`wacksy` crates |
| Provenance map (fact → DOM node + screenshot region + URL) | P1 | v0.4 | Grounding moat |

### C. Extraction intelligence
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Boilerplate/nav/ad/cookie-banner removal | P0 | v0.1 | |
| Token-budget-aware output + measured token count | P1 | v0.2 | "give me ≤2k tokens" |
| Deduplication (sections/pages) | P2 | v0.3 | Incremental savings |
| Schema-based structured extraction (BYO-LLM) | P1 | v0.4 | Model-agnostic; user's own LLM |
| Natural-language extraction ("extract all prices") | P1 | v0.4 | |
| Self-healing selectors (LLM re-map on layout change) | P2 | v0.6 | ~85% maintenance cut |
| Chunking/summarization for RAG | P2 | v0.6 | Optional |
| Language detection / encoding handling | P1 | v0.2 | |

### D. Crawling & navigation
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Single-page capture | P0 | v0.1 | |
| Multi-page crawl (link follow, domain scope, depth/page budget) | P1 | v0.3 | |
| Sitemap.xml ingestion | P2 | v0.3 | |
| robots.txt respect + polite rate limiting + honest UA | P1 | v0.3 | Ethical-by-default = trust moat |
| Pagination / infinite-scroll / "next" handling | P2 | v0.6 | |
| Agent-native actions (navigate/click/fill/scroll/wait) | P1 | v0.2 | MCP primitives |
| Incremental crawl (content-hash + conditional GET) | P1 | v0.3 | Only changed pages |
| Change detection / diff feed | P2 | v0.3 | |
| Scheduling / recurring crawls | P2 | v0.7 | Or defer to external schedulers |

### E. Caching, storage & memory
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Content-addressed cache (hash → result) | P1 | v0.3 | |
| Conditional requests (ETag / If-Modified-Since) | P1 | v0.3 | |
| Crawl store/index (what captured, when) | P1 | v0.3 | |
| Local semantic memory (embeddings over corpus, offline search) | P2 | v0.7 | Persistent web memory |
| Dataset export (JSONL / parquet) | P2 | v0.7 | RAG/training corpora |

### F. Interfaces & distribution
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Rust library (`amber-core`) | P0 | v0.1 | Primary API |
| CLI (`amber`) — one-shot + batch | P0 | v0.1 | |
| MCP server | P0 | v0.2 | Headline agent interface |
| HTTP / daemon server mode | P2 | v0.7 | Local service, multi-client |
| UniFFI bindings (Python first) | P1 | v0.6 | maturin → PyPI |
| C ABI (cbindgen) + long-tail wrappers | P1 | v0.6 | Go/C#/Java/Lua/… |
| Node bindings (napi-rs) | P2 | v0.6 | |
| Packaging: cargo, PyPI, npm, Homebrew, Docker, prebuilt binaries | P1 | v0.6+ | |

### G. Reliability, observability & ops
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Structured logging + tracing | P0 | v0.1 | |
| Token accounting & cost reporting | P1 | v0.2 | A differentiator users *see* |
| Metrics (latency, render rate, cache hit rate) | P2 | v0.7 | |
| Reproducible captures (pinned browser → deterministic) | P1 | v0.5 | Evals/datasets |
| Resource limits (memory, concurrency caps) | P1 | v0.7 | |

### H. Security & privacy
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Local-first, zero telemetry by default | P0 | v0.1 | |
| Airgapped operation | P0 | v0.1 | After browser is cached |
| Browser sandboxing / process isolation | P1 | v0.3 | |
| Secrets handling for auth (never logged) | P1 | v0.3 | |
| Bring-your-own-proxy hook | P2 | v1.0 | No built-in proxy empire |

### I. Optional / specialized modes (future)
| Feature | Pri | Phase | Notes |
|---|---|---|---|
| Tamper-evident evidence capture (hash + timestamp + signature) | P2 | post-1.0 | Compliance/legal niche, willingness-to-pay |
| AI training-corpus builder (provenance-tagged, deduped) | P2 | post-1.0 | Demand > supply |
| Visual regression / page monitoring | P2 | post-1.0 | Crowded; only if core makes it cheap |

---

## 7. Roadmap & milestones

Each phase ends with a working, demonstrable artifact. Phases are sequenced to
**prove the thesis early** and de-risk the hardest parts first.

### Phase 0 — Foundation *(now)*
Repo, license, CI, Cargo workspace skeleton, Chromium fetcher, native CDP transport,
`navigate` + screenshot smoke test.
**Done when:** `cargo run` launches pinned Chromium and screenshots a URL.

### Phase 1 — v0.1 "Reader MVP" *(thesis proof)*
Tiered fetch → settle → render → **Markdown + main-content extraction + screenshot**,
exposed via the **CLI**. Single-shot capture.
**Done when:** `amber read <url>` returns clean Markdown + a screenshot, using HTTP-only
when possible and a browser only when needed.

### Phase 2 — v0.2 "Agent-native"
**MCP server** exposing read/fetch tools; **token-budget output** + token accounting;
cleaned HTML; multiple representations from one pass; agent-action primitives;
resource blocking.
**Done when:** Claude Code/Cursor can call AmberHTML over MCP to read a page within a token budget.

### Phase 3 — v0.3 "Crawl"
Multi-page crawl (link follow, depth/budget), **robots.txt + politeness**, auth/session,
**caching + incremental** (content-hash + conditional GET), crawl store, change diffs.
**Done when:** a bounded, polite, incremental crawl of a site returns only changed pages on re-run.

### Phase 4 — v0.4 "Structured & Smart"
**Schema / NL structured extraction (BYO-LLM)**, **provenance anchoring**.
**Done when:** "extract `{title, price}` from these pages" returns structured JSON with per-field provenance, using the user's own LLM.

### Phase 5 — v0.5 "Archive & fidelity"
**WARC/WACZ**, MHTML, single-file HTML, PDF, accessibility-tree bundle, reproducible captures.
**Done when:** a capture round-trips as a replayable WACZ and a portable single-file HTML.

### Phase 6 — v0.6 "Bindings"
**UniFFI (Python via maturin)** first, then **C ABI**; Node via napi-rs; self-healing extraction.
**Done when:** `pip install amber-html` gives a Python user `amber.read(url)`.

### Phase 7 — v0.7 "Scale & ops"
Browser/tab pooling, concurrency, **daemon/HTTP server mode**, metrics, local semantic memory, dataset export.
**Done when:** a long-running daemon serves concurrent reads with bounded memory and a warm cache.

### Phase 8 — v1.0 "Polish & trust"
Docs, stability, headed/stealth mode, bring-your-own-proxy hook, reproducibility guarantees, broad packaging.
**Done when:** documented, stable API; installable via cargo/PyPI/npm/Homebrew/Docker.

### Post-1.0 — Specialized modes
Evidence capture, training-corpus builder, page monitoring (as demand warrants).

---

## 8. Locked technical decisions

(See `.claude` memory `amberhtml-plan` for the authoritative record.)

- **Language/core:** Rust; `amber-core` async inside, **blocking public API**.
- **Transport:** dual behind `CdpTransport` — native hand-rolled WebSocket (default), chromiumoxide (optional escape hatch + differential test oracle), feature-gated.
- **Browser:** always required; **managed, pinned Chrome for Testing**, checksum-verified, cached; `AMBER_CHROMIUM_PATH` escape hatch.
- **HTML capture:** `Page.captureSnapshot` (MHTML) baseline; optional single-file-HTML transform.
- **Bindings:** UniFFI + C ABI, both first-tier; WASM rejected (can't drive a native browser).
- **License:** dual **`MIT OR Apache-2.0`**.
- **Naming:** package `amber-html`; brand `AmberHTML`.

---

## 9. Risks & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Content-extraction quality lags Firecrawl (the hard craft) | High | Lean on proven Readability algorithms; iterate on real pages; let users plug their own extractor |
| Fidelity gap on JS-heavy pages | Med | Strong settle engine + headed fallback; differential tests vs chromiumoxide |
| Multi-language binding maintenance burden | Med | Thin mechanical bindings only; UniFFI for the curated tier; don't hand-maintain N reimplementations |
| Anti-bot blocks (Cloudflare etc.) | Med | Don't compete at scale; headed/stealth + BYO-proxy hook for the few-pages case |
| Chromium version drift breaks native CDP client | Low | Pin browser; codegen subset from that revision's protocol JSON; protocol-audit gate on bump |
| "Already done by SingleFile/monolith" perception | Med | Position as AI-reading engine for agents, not offline HTML; lead with local-first + provenance + MCP |
| Memory/CPU cost of real browser at scale | Med | Tiered fetch (skip browser when possible); pooling; resource limits |
| Scope sprawl (this catalog is large) | High | Strict phase gates; each phase ships a usable artifact before the next |

---

## 10. Success metrics

**Adoption (the real prize):** GitHub stars, crates.io/PyPI downloads, MCP-server installs, mentions in agent frameworks.

**Technical, per phase:**
- **Efficiency:** % of pages served HTTP-only (no browser); median read latency; memory/page.
- **Quality:** extraction fidelity vs a labeled set; token reduction vs raw HTML (target 70–90%).
- **Cost:** $0 per page (vs Firecrawl) — the headline comparison.
- **Reach:** can read `localhost`/intranet/auth pages competitors can't.

---

## 11. Open questions

1. **Default extractor** — bundle a Rust Readability port, or shell to an embedded one? Affects v0.1 quality.
2. **MCP tool surface** — how many tools (read / crawl / extract / screenshot) and their exact schemas?
3. **Token counting** — which tokenizer(s) to report against (tiktoken-compatible? model-specific)?
4. **Structured-extraction LLM interface** — abstraction for BYO-model (OpenAI-compatible endpoint? local GGUF? both?).
5. **Crawl state storage** — embedded (SQLite/sled) vs flat files for the cache/index?
6. **Single-file HTML** — ship the transform in v0.5 or defer to post-1.0?
7. **Semantic memory** — in-scope for v0.7 or a separate companion crate?
