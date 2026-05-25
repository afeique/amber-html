# AmberHTML for R

R bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. R has no FFI, so a small C shim adapts the
`amber-core` C ABI to R's `.Call` interface (`src/amber.c`).

## Build

Stage the native library + header, then install the package:

```sh
bindings/r/generate.sh                  # builds + stages src/amber.h + libamber_core
R CMD INSTALL bindings/r                 # compiles the shim and links amber-core
Rscript -e 'testthat::test_dir("bindings/r/tests/testthat")'
```

If the linker's rpath can't resolve `libamber_core` at run time, point the
loader at it: `DYLD_LIBRARY_PATH` (macOS) / `LD_LIBRARY_PATH` (Linux).

## Usage

```r
library(amber)

# Text formats:
md   <- capture_markdown("https://example.com")
text <- capture_readable("https://example.com")

# Any format as raw bytes (binary formats too):
pdf <- capture("https://example.com", Format$PDF)
png <- capture("https://example.com", Format$SCREENSHOT)

# Or write straight to a file (returns the written path):
path <- save_capture("https://example.com", Format$HTML, "out", "page")

# Capture once, emit many — one render serves every format:
snap <- snapshot("https://example.com", c(Format$MARKDOWN, Format$PDF))
snap_md  <- snap$markdown()
snap_pdf <- snap$render(Format$PDF)
snap$close()
```

`Format` fields: `HTML`, `MHTML`, `MARKDOWN`, `READABLE`, `WARC`, `WACZ`,
`SCREENSHOT`, `PDF`. Failures raise an R error (`amber: …`).

The first capture that needs a browser downloads a pinned Chrome for Testing
into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Note on distribution

The package links the native `amber-core` library, so a binary build must ship
(or the user must stage) `libamber_core` for the platform and ensure the dynamic
loader can find it at run time.

## License

Dual MIT OR Apache-2.0 (the `License: MIT + file LICENSE` field reflects R's
single-license metadata; see the repository LICENSE files for both).
