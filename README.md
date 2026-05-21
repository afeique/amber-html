# AmberHTML

> A Rust engine that drives a local Chromium via the Chrome DevTools Protocol to faithfully render any page, then emits Markdown, structured JSON, screenshots, and WARC from a single pass — with provenance and a token budget. Ships as a library + MCP server.

**Local-first, private web-reading engine for AI agents.** AmberHTML renders pages in a real browser *only when needed*, then emits clean Markdown, structured data, and screenshots. A free, self-hosted alternative to Firecrawl.

> 🚧 **Status: early development (design phase).** The direction and architecture are set; implementation is just beginning. Expect rapid change.

## Why

Cloud web-reading APIs (Firecrawl, Jina, …) are powerful but, by being cloud SaaS, they are structurally:

- **expensive** — per-page billing, and failed requests still cost you;
- **non-private** — every URL you read, including internal ones, leaves your machine;
- **blind to private surfaces** — they can't reach `localhost`, intranet, VPN, or behind-auth pages.

AmberHTML attacks exactly those by running on *your* machine: free, private, and able to read the pages a cloud service never can.

## Goals

- **Tiered fetching** — try a cheap HTTP fetch first; escalate to a headless browser only when the page actually needs JavaScript.
- **Render once, emit everything** — Markdown, cleaned HTML, screenshot, accessibility tree, structured JSON, and WARC from a single browser pass.
- **Token-budget-aware output** — main-content extraction and boilerplate removal, with the resulting token count reported.
- **Caching & incremental crawling** — content-hash + conditional requests; skip unchanged pages, return only diffs.
- **Model-agnostic structured extraction** — describe a schema, run it against *your own* LLM (cloud or local).
- **Provenance** — every extracted fact anchors back to a DOM node, screenshot region, and source URL.
- **Embeddable everywhere** — a Rust core with thin bindings to many languages, plus a standalone CLI and MCP server.

## Design (in brief)

- Rust core driving Chromium over CDP, behind a swappable transport.
- A pinned, auto-managed [Chrome for Testing](https://googlechromelabs.github.io/chrome-for-testing/) build for reproducible rendering.
- Bindings via UniFFI (Python/Swift/Kotlin/Ruby) and a C ABI (the long tail).

## Roadmap

The full product strategy, feature catalog, and phased roadmap live in
[docs/PLAN.md](docs/PLAN.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
