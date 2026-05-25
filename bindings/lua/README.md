# AmberHTML for Lua

Lua bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Uses the **LuaJIT FFI** over the
`amber-core` C ABI. Requires **LuaJIT** (for the built-in `ffi` library); plain
PUC Lua has no FFI.

## Build

It loads a native library, so stage it from the repo first:

```sh
bindings/lua/generate.sh           # builds the native lib into lib/
cd bindings/lua && luajit test/smoke.lua
```

## Usage

```lua
local amber = require("amber")
local F = amber.Format

-- Text formats:
local md   = amber.capture_markdown("https://example.com")
local text = amber.capture_readable("https://example.com")

-- Any format as bytes (a binary string; binary formats too):
local pdf = amber.capture("https://example.com", F.PDF)
local png = amber.capture("https://example.com", F.SCREENSHOT)

-- Or write straight to a file (returns the written path):
local path = amber.save("https://example.com", F.HTML, "out", "page")

-- Capture once, emit many — one render serves every format:
local snap = amber.snapshot("https://example.com", { F.MARKDOWN, F.PDF })
local snap_md  = snap:markdown()
local snap_pdf = snap:render(F.PDF)
snap:close()
```

`Format` fields: `HTML`, `MHTML`, `MARKDOWN`, `READABLE`, `WARC`, `WACZ`,
`SCREENSHOT`, `PDF`. Failures raise a Lua error (`amber: …`); catch with
`pcall`.

By default the library loads from `lib/libamber_core.{dylib,so}` (relative to
the working directory); set `AMBER_LIB` to load it from elsewhere. The first
capture that needs a browser downloads a pinned Chrome for Testing into the
cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Note on distribution

The FFI loads the native `amber-core` library at run time, so the rock must ship
(or the user must stage) the right `libamber_core` for the platform. The pure
Lua module (`amber.lua`) only needs the LuaJIT `ffi` library at run time.

## License

MIT OR Apache-2.0.
