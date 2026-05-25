# Distribution & Bindings — Audit Report + Plan

作成日: 2026-05-24

This is a distribution **audit + planning document**. The single source of truth
for the feature set remains [`Plans.md`](../Plans.md); this file scopes the work
to get AmberHTML **shipped onto as many package managers as possible**, plus
fixes found during the audit and the full bindings status.

> **Status update (2026-05-24).** The binding **surface** and **new-language
> packaging** tracks below are now largely implemented (the "second agent"
> framing earlier in this doc referred to concurrent work that has since landed
> on `master` — it was reconciled into this session). What's done:
>
> - **FFI surface widened** on both engines — `capture(format)→bytes`,
>   `capture_text`, and `save` across UniFFI *and* the C ABI; `OutputFormat` is a
>   `uniffi::Enum`; `CaptureError` uses a named field (Ruby-backend compat).
> - **`cbindgen.toml` fixed** (finding 1.5 #2 / D0.3) — the export allowlist now
>   covers the full C ABI; the double-`amber_` prefix is gone.
> - **New language packages** under `bindings/`: **Ruby** (gem), **Swift**
>   (SwiftPM xcframework), **Kotlin/Java** (Gradle + JNA), **Go** (cgo),
>   **C#/.NET** (P/Invoke). Ruby/Swift/Go validated end-to-end locally; Kotlin/C#
>   generation/staging validated (full build runs in CI).
> - **CI** `bindings.yml` compile-checks every binding; **release.yml** gained
>   `gem` + `nuget` publish jobs; Maven/SwiftPM/Go publish paths are documented
>   in `RELEASING.md` §5.
>
> Still open (good next steps): the **D0** cross-cutting blockers below —
> Windows transport (1.5 #1), the README status banner (1.5 #3), registry name
> reservations (D0.4), and richer CI gates (D0.5) — plus the **`Snapshot`-object
> across FFI** ergonomic win (3.2 / D2.1), still the highest-leverage item.

---

## 1. Audit — current state (verified 2026-05-24)

### 1.1 Engine

- **Feature-complete.** Phases 0–5, 7, and Post-1.0 in `Plans.md` are all
  `cc:完了`. Open items there: 6.7 packaging (`cc:WIP`), 8.2 API-stability
  (`cc:WIP`), 8.6 broad-packaging GA (`cc:WIP`).
- **Build is green.** `cargo build --workspace --locked` succeeds; **371 test
  functions**, 16 `#[ignore]`d (browser/live-network) tests. clippy/fmt gated in
  CI.
- Workspace: `amber-core` (lib: `cdylib`+`rlib`), `amber-cli` (bin: `amber`),
  `amber-node` (napi cdylib, `publish=false`), `uniffi-bindgen` (CLI,
  `publish=false`).
- Toolchain present here: cargo/rustc 1.95, node 24, npm 11, python 3.13.
  **Absent here: `maturin`, `cbindgen`, `docker`** (needed for some validations).

### 1.2 Bindings inventory

| Binding | Mechanism | Surface today | Registry target | Status |
|---|---|---|---|---|
| **Rust** | native crate | full API | crates.io | ✅ ready (config in `release.yml`) |
| **Python** | UniFFI → maturin | `capture`, `capture_text`, `save`, `capture_markdown`, `capture_readable` | PyPI | ✅ generated + wheel validated locally; **module is `amber_core`** (awkward) |
| **Node** | napi-rs (`amber-node`) | only `captureMarkdown` / `captureReadable` | npm | ⚠️ **surface lags** UniFFI/C-ABI (no all-format `capture`/`save`) |
| **C ABI** | `capi.rs` + `include/amber.h` | `amber_capture_markdown/readable`, `amber_capture` (bytes, any format), `amber_save`, `*_free` | GitHub Release (header+lib), vcpkg/Conan later | ✅ widened; ⚠️ **`cbindgen.toml` stale** (see 1.5) |
| **Swift** | UniFFI | `capture`/`capture_text`/`save`/markdown/readable | SwiftPM / CocoaPods | ✅ `bindings/swift` (xcframework); `swift build`+`swift test` green locally |
| **Kotlin/Java** | UniFFI + JNA | same | Maven Central | ✅ `bindings/kotlin` (Gradle); generation/staging validated, build in CI |
| **Ruby** | UniFFI + `ffi` gem | same | RubyGems | ✅ `bindings/ruby` (gem); smoke + `gem build` green locally; `gem` release job |
| **C#** | C-ABI P/Invoke | same | NuGet | ✅ `bindings/csharp` (P/Invoke); staging validated, build in CI; `nuget` release job |
| **Go** | C-ABI cgo | same | Go modules | ✅ `bindings/go` (cgo); `go test` green locally |

**Key fact (corrects an earlier assumption):** UniFFI **0.31.1** *does* still
ship a Ruby backend, and `uniffi-bindgen generate --language {python,swift,
kotlin,ruby}` is reachable in proc-macro/library mode (the `--library` flag is
now auto-detected). So Swift/Kotlin/Ruby are a **packaging** gap, not a
generation gap.

### 1.3 Packaging / distribution channels

`.github/workflows/release.yml` fires on a `vX.Y.Z` tag and already wires six
channels; configs validated locally, but **none have been published** (needs
accounts/secrets + a tag). `RELEASING.md` documents the one-time setup.

| Channel | Artifact | In `release.yml`? | Gap to GA |
|---|---|---|---|
| crates.io | `amber-core`, `amber-cli` | ✅ | `CARGO_REGISTRY_TOKEN`; name check |
| PyPI | maturin wheels ×5 platforms | ✅ | Trusted-Publishing/`pypi` env; name check |
| npm | napi prebuilds ×4 | ✅ | `NPM_TOKEN`; name check; **surface parity (1.2)** |
| GHCR (Docker) | multi-arch image | ✅ | make package public after 1st push |
| GitHub Releases | CLI binaries ×5 (incl. Windows) | ✅ | **Windows panics (1.5)** |
| Homebrew | source-build formula | ⚠️ formula exists | create tap `afeique/homebrew-amber`; fill `sha256` |

### 1.4 The other agent's work (inferred from git)

All work is on `master`; the two other branches (`feat/comprehensive-plan`,
`chore/reconcile-plans-drift`) are stale (May 21–22, behind `master`). The two
most recent commits (2026-05-23 ~15:00) are the active binding track:

- `2ad8fb0 feat(ffi): widen UniFFI surface to all formats, bytes, and save`
- `7e0fb9d feat(capi): widen C ABI to all formats, raw bytes, and save`

**Pattern → likely ownership.** The agent is widening each binding's *surface*
to reach parity with the core (`capture(format)→bytes`, `capture_text`, `save`).
They have done **UniFFI** and **C-ABI**; **Node is the obvious next** (it still
exposes only markdown/readable). Treat as **theirs**:

- Node surface parity (`capture`/`save`/all formats) — task 6.3 follow-up.
- Any further surface-shaping of UniFFI/C-ABI (incl. a possible `Snapshot`
  object — see 3.2, flag for coordination).

### 1.5 Findings — bugs & inconsistencies (fix before/with GA)

1. **Windows transport is `unimplemented!()`** — `cdp.rs:414` panics on browser
   spawn (fd 3/4 inheritance is Unix-only; the Windows HANDLE path is a stub).
   Yet `release.yml` ships **Windows CLI binaries, Windows wheels, and Windows
   npm prebuilds**. Static-only paths (e.g. `--markdown` on a server-rendered
   page) may work, but `--screenshot/--pdf/--mhtml` and any JS page **panic**.
   → P0: either implement Windows, or drop Windows from all release matrices and
   document it as unsupported.
2. **`cbindgen.toml` `[export] include` is stale** — lists only
   `amber_capture_markdown`, `amber_capture_readable`, `amber_string_free`.
   Regenerating `include/amber.h` would **drop** `amber_capture`, `amber_save`,
   `amber_bytes_free`. → fix the include list (or remove it and let all
   `amber_`-prefixed exports through), add a header-drift check.
3. **README banner is wrong** — "🚧 early development… implementation is just
   beginning" while the engine is feature-complete with a written v0.1.0
   changelog. → must update before any public release (it's the crates.io /
   PyPI / npm front page).
4. **Python module name is `amber_core`** — users `import amber_core`, not
   `import amber`. No `uniffi.toml`. → add `uniffi.toml` namespace and/or a thin
   `amber` wrapper package so the import matches the brand.
5. **Bindings are stateless free-functions** — every format call re-runs
   `snapshot(url, …)`, so capturing 3 formats = **3 browser renders**. The
   core's "render once, emit everything" (`snapshot` takes `&[OutputFormat]`) is
   **not exposed across FFI**. Diverges from the documented "one capture →
   `Snapshot` object with format methods" promise (see [[amberhtml-plan]] binding
   ergonomics). → see 3.2 (coordinate with other agent).
6. **No binding/release validation in CI** — `ci.yml` builds/tests Rust only.
   No Python/Node import smoke, no `cbindgen` header-drift check, no MSRV, no
   `cargo publish --dry-run`, no `release.yml` dry-run. First publish is
   effectively untested until the tag is pushed.
7. **Registry name availability unverified** — `amber-html` (PyPI/npm/RubyGems),
   `amber-core`/`amber-cli` (crates.io), NuGet id, Maven coordinates. Must check
   before the first publish; squatting/collision would force a rename.
8. **Homebrew formula builds from source** (`depends_on "rust"`) — slow install;
   a binary/bottle formula pulling the GitHub Release asset is much faster.
   Minor, post-GA acceptable.

---

## 2. Package-manager reach map

Which existing artifact reaches which registry (drives the plan in §3):

```
Rust core ───────────────► crates.io
CLI binary ──────────────► Homebrew · Scoop · WinGet · Chocolatey · AUR ·
                            Nix · deb/rpm · MacPorts · GHCR · GitHub Releases
UniFFI ──────────────────► PyPI (Python) · RubyGems (Ruby) ·
                            Maven Central (Kotlin/Java) · SwiftPM/CocoaPods (Swift)
UniFFI 3rd-party gens ───► NuGet (C#, uniffi-bindgen-cs) ·
                            Go modules (uniffi-bindgen-go) · pub.dev (Dart)
napi-rs ─────────────────► npm
C ABI (fallback) ────────► vcpkg/Conan (C/C++) and any FFI language:
                            Packagist (PHP) · luarocks (Lua) · CRAN (R) ·
                            Hex (Elixir/rustler) · Julia · Perl · Zig/Nim/Crystal
conda-forge ─────────────► Python (alt to PyPI)
```

---

## 3. Plan — tasks (prioritized, tiered)

Status legend mirrors `Plans.md` (`cc:TODO` etc.). Suggested IDs use a `D.`
(distribution) prefix to avoid colliding with `Plans.md` numbering; fold into
`Plans.md` Phase 6/8 if desired.

### 3.0 Cross-cutting blockers — **P0** (do first; gate GA)

| ID | Task | DoD |
|---|---|---|
| D0.1 | **Windows decision**: implement CDP HANDLE inheritance on Windows *or* remove Windows from the binaries/wheels/npm matrices and document it unsupported | No shipped artifact panics on capture; Windows status documented |
| D0.2 | Update README (drop "early development" banner; reflect feature-complete v0.1.0) | README front-page accurate for registry listings |
| D0.3 | Fix `cbindgen.toml` include list (+ regenerate `include/amber.h`, verify diff) | Generated header matches `capi.rs` (all 6 symbols) |
| D0.4 | Reserve/verify names on every registry (crates.io, PyPI, npm, RubyGems, NuGet, Maven) | Each name confirmed free or reserved |
| D0.5 | CI: add binding smoke (py import, node require), header-drift check, `cargo publish --dry-run`, MSRV | CI fails on binding/publish regressions before a tag |

### 3.1 Tier 0 — finish the six already-wired channels to GA — **P0/P1**

| ID | Task | DoD |
|---|---|---|
| D1.1 | crates.io: add `CARGO_REGISTRY_TOKEN`; publish `amber-core` then `amber-cli` | `cargo install amber-cli` works |
| D1.2 | PyPI: configure Trusted Publishing + `pypi` environment; first wheel set | `pip install amber-html` works on 5 platforms |
| D1.3 | npm: add `NPM_TOKEN`; publish (after Node surface parity, see §4) | `npm i -g amber-html` works |
| D1.4 | GHCR: first image push, mark package public | `docker run ghcr.io/afeique/amber-html …` works |
| D1.5 | GitHub Releases: binaries attach on tag (Windows per D0.1) | release assets present for each supported target |
| D1.6 | Homebrew: create `afeique/homebrew-amber` tap, fill `sha256` post-release | `brew install afeique/amber/amber` works |

### 3.2 Binding ergonomics — **P1** (coordinate; possibly other agent's)

| ID | Task | DoD |
|---|---|---|
| D2.1 | Expose a **`Snapshot` object across FFI** (UniFFI `#[uniffi::export]` object + C-ABI opaque handle + `*_free`) so one capture serves many formats | `snap = amber.snapshot(url, formats); snap.markdown(); snap.save_pdf(dir)` — one render, idiomatic per language |
| D2.2 | Python: `uniffi.toml` namespace + thin `amber` package so users `import amber` | `import amber; amber.snapshot(...)` |
| D2.3 | Node surface parity with UniFFI/C-ABI (all formats, `save`) | **likely other agent** — confirm before starting |

### 3.3 Tier 1 — user-requested new managers — **P1**

| ID | Task | Mechanism | DoD |
|---|---|---|---|
| D3.1 | **RubyGems** gem | UniFFI `--language ruby` + gemspec bundling the native lib (`fat`/platform gems via `rake-compiler`/`oxidize`) | `gem install amber-html`; `require 'amber'` captures |
| D3.2 | **NuGet** package (C#) | preferred: `uniffi-bindgen-cs`; fallback: hand C-ABI P/Invoke | `dotnet add package AmberHtml`; RID-specific native runtimes resolve on win/linux/osx |
| D3.3 | conda-forge feedstock (Python alt) | wrap the PyPI wheel/recipe | `conda install -c conda-forge amber-html` |

### 3.4 Tier 2 — high-value ecosystems — **P1/P2**

| ID | Task | Mechanism | DoD |
|---|---|---|---|
| D4.1 | **Maven Central** (Kotlin/Java) | UniFFI Kotlin + JNA + native libs in jar; Sonatype publish | `implementation("io.github.afeique:amber-html:…")` works |
| D4.2 | **Swift Package Manager** (+ optional CocoaPods) | UniFFI Swift + `xcframework` | `.package(url: …)` builds; macOS capture works |
| D4.3 | **Go module** | `uniffi-bindgen-go` or C-ABI cgo; likely companion repo `amber-html-go` | `go get` + capture |
| D4.4 | Windows managers: **Scoop**, **WinGet**, **Chocolatey** (after D0.1) | CLI binary | each install path verified on Windows |
| D4.5 | **AUR** + **Nix** (`nixpkgs`/flake) | CLI binary/source | `paru -S amber-html`; `nix run` |

### 3.5 Tier 3 — long tail — **P2**

| ID | Task | Mechanism |
|---|---|---|
| D5.1 | Dart/Flutter → pub.dev | `uniffi-bindgen-dart` |
| D5.2 | `.deb` / `.rpm` for the CLI | `cargo-deb` / `cargo-generate-rpm` (+ optional Debian/Fedora-COPR/Ubuntu-PPA) |
| D5.3 | Alpine apk; MacPorts | CLI binary |
| D5.4 | vcpkg / Conan (C/C++) | C-ABI + `include/amber.h` |
| D5.5 | PHP (Packagist, FFI), Lua (luarocks), R (CRAN), Elixir (Hex/rustler), Julia, Perl | C-ABI fallback per language |

### 3.6 Release-trust hardening — **P2**

| ID | Task | DoD |
|---|---|---|
| D6.1 | Consider `cargo-dist` to unify binary/installer generation | one config drives binaries + shell/PS install scripts |
| D6.2 | Sign release artifacts (sigstore/cosign) + SBOM | signatures + SBOM attached; aligns with the project's tamper-evident positioning |
| D6.3 | Per-channel install-smoke job that runs *after* publish | a scheduled job installs from each registry and runs `--help`/import |

---

## 4. Coordination with the other agent

- **Theirs (do not duplicate):** the binding **surface** — which functions each
  binding exposes — for UniFFI, C-ABI, and (next, likely) Node. D2.3 is almost
  certainly theirs.
- **Possibly theirs / coordinate:** D2.1 (`Snapshot` object across FFI). It is
  surface work but is the single highest-leverage binding improvement (fixes the
  N-renders-for-N-formats issue and matches the product promise). **Confirm
  before either side starts it.**
- **Ours (this plan):** packaging/distribution to every registry, new language
  **packaging** (gemspec, `.nupkg`, Maven, SwiftPM, Go module), wiring 3rd-party
  generators (`uniffi-bindgen-cs/-go/-dart`), CI/release hardening, and the
  cross-cutting blockers in §3.0. Note the C#/Ruby *generation* step lightly
  overlaps the bindings track — the *packaging* is unambiguously ours; sync on
  the generation step.

## 5. Suggested sequencing

1. **§3.0 blockers** (Windows decision, README, cbindgen, name checks, CI gates).
2. **§3.1 Tier 0** GA on the six wired channels (the fastest reach: Rust,
   Python, Node, Docker, binaries, Homebrew).
3. **§3.3 Tier 1** (Ruby, C#, conda) — the explicit asks.
4. **§3.4 Tier 2**, then **§3.5 Tier 3** opportunistically.
5. **§3.2 ergonomics** in parallel, gated on coordination with the other agent.
