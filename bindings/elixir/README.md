# AmberHTML for Elixir

Elixir bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Implemented as a
[rustler](https://github.com/rusterlium/rustler) NIF (`native/amber_nif`) that
calls `amber-core`'s Rust API directly. Captures run on **dirty IO schedulers**,
so they never stall a normal BEAM scheduler.

## Build

Requires Elixir and a Rust toolchain (rustler compiles the NIF via Cargo):

```sh
cd bindings/elixir
mix deps.get
mix test
```

## Usage

```elixir
# Text formats:
md   = Amber.capture_markdown("https://example.com")
text = Amber.capture_readable("https://example.com")

# Any format as bytes (a binary; binary formats too):
pdf = Amber.capture("https://example.com", :pdf)
png = Amber.capture("https://example.com", :screenshot)

# Or write straight to a file (returns the written path):
path = Amber.save("https://example.com", :html, "out", "page")

# Capture once, emit many — one render serves every format:
snap = Amber.snapshot("https://example.com", [:markdown, :pdf])
snap_md  = Amber.Snapshot.markdown(snap)
snap_pdf = Amber.Snapshot.render(snap, :pdf)
```

Formats are atoms: `:html`, `:mhtml`, `:markdown`, `:readable`, `:warc`,
`:wacz`, `:screenshot`, `:pdf`. Failures raise an `ErlangError`.

The first capture that needs a browser downloads a pinned Chrome for Testing
into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Note on distribution

The NIF is built from source via rustler at compile time. For prebuilt
artifacts, pair with
[`rustler_precompiled`](https://github.com/philss/rustler_precompiled).

## License

MIT OR Apache-2.0.
