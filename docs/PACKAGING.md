# Packaging

Status of the distribution channels (Plans.md Phase 6 / 8.6). Several are
**WIP scaffolding**: the configuration is in place but the build/publish step
needs a toolchain not present in every dev environment, so it hasn't been
validated end-to-end here.

## Python (UniFFI) — 6.1, WIP

The Rust core builds as a `cdylib` and exports a small UniFFI surface
(`capture_markdown`, `capture_readable`; see `crates/amber-core/src/ffi.rs`).

**Validated:** generating + importing + calling the bindings works:

```sh
cargo build -p amber-core
cargo run -p uniffi-bindgen -- generate \
    --library target/debug/libamber_core.dylib \
    --language python --out-dir bindings/python
# bindings/python/amber_core.py imports and exposes capture_markdown(url)
```

**Pending:** building/publishing the wheel needs [maturin](https://www.maturin.rs):

```sh
pip install maturin
maturin develop          # local install into the active venv
maturin build --release  # wheel under target/wheels/
```

`uniffi-bindgen` also targets `--language swift|kotlin|ruby` for the rest of
the UniFFI family.

## C ABI — 6.2, done

A hand-maintained header (`include/amber.h`) over the cdylib; `cbindgen.toml`
regenerates it. See `examples/c/example.c` for a wrapper that builds + links.

## Node (napi-rs) — 6.3, done

`crates/amber-node` is a napi-rs binding crate exposing `captureMarkdown(url)`
/ `captureReadable(url)` over the core.

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
