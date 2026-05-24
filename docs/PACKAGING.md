# Packaging

Status of the distribution channels (Plans.md Phase 6 / 8.6). Several are
**WIP scaffolding**: the configuration is in place but the build/publish step
needs a toolchain not present in every dev environment, so it hasn't been
validated end-to-end here.

## Python (UniFFI) — 6.1, done

The Rust core builds as a `cdylib` and exports the full UniFFI surface
(`capture`/`capture_text`/`save`, `capture_markdown`/`capture_readable`, and the
capture-once `Snapshot` object; see `crates/amber-core/src/ffi.rs`). The wheel
also ships a thin pure-Python `amber` package (`python/amber/`, via maturin
`python-source`) that re-exports the generated `amber_core`, so users
`import amber` (Plans.md 10.4); `import amber_core` still works.

**Validated end-to-end** in this environment — `maturin build` produces a wheel
(invoking the `uniffi-bindgen` crate to generate the bindings), and the
pip-installed wheel imports as `amber` (or `amber_core`) and runs:

```sh
pip install maturin
maturin build --release                       # → target/wheels/amber_html-*.whl
pip install target/wheels/amber_html-*.whl
python -c "import amber; amber.capture_markdown('https://example.com')"  # or: import amber_core
```

For local dev, `maturin develop` installs into the active venv. `uniffi-bindgen`
also targets `--language swift|kotlin|ruby` for the rest of the UniFFI family.
**Remaining for distribution:** `maturin publish` + per-platform wheels (8.6).

## C ABI — 6.2, done

A hand-maintained header (`include/amber.h`) over the cdylib; `cbindgen.toml`
regenerates it. See `examples/c/example.c` for a wrapper that builds + links.

## Node (napi-rs) — 6.3, done

`crates/amber-node` is a napi-rs binding crate exposing the full surface over
the core: `capture(url, format)` → `Buffer`, `captureText`, `save`,
`captureMarkdown`/`captureReadable`, a `Format` enum, and a `snapshot(url,
formats)` → `Snapshot` object that renders/saves many formats from one capture
(Plans.md 10.2).

**Validated:** the addon builds and loads + runs under Node:

```sh
cargo build -p amber-node
cp target/debug/libamber_node.dylib crates/amber-node/amber.node   # .so on Linux
node crates/amber-node/__test__/smoke.test.js   # require + call (a bad URL throws)
```

For distribution, `@napi-rs/cli` builds per-platform prebuilds:
`cd crates/amber-node && npm install && npm run build` (then `npm publish`).

## Docker — 6.7, WIP

`Dockerfile` builds the `amber` CLI in a multi-stage image. Not built here
(no Docker); see the file header for build/run commands. A pinned Chrome for
Testing downloads on first browser capture (mount a volume to persist it).

## Cargo / Homebrew / GA — 8.6, TODO

`cargo install` works from source today. Homebrew formula, prebuilt binaries,
and a GA release across channels remain.
