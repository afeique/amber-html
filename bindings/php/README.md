# AmberHTML for PHP

PHP bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Uses [PHP FFI](https://www.php.net/manual/en/book.ffi.php)
over the `amber-core` C ABI (PHP ≥ 8.1 with `ext-ffi`).

## Build

It loads a native library, so stage it from the repo first:

```sh
bindings/php/generate.sh          # builds the native lib into lib/
php bindings/php/test/smoke.php    # runs the smoke test
```

## Usage

```php
use Amber\Amber;
use Amber\Format;

// Text formats:
$md   = Amber::captureMarkdown("https://example.com");
$text = Amber::captureReadable("https://example.com");

// Any format as bytes (binary formats too):
$pdf = Amber::capture("https://example.com", Format::PDF);
$png = Amber::capture("https://example.com", Format::SCREENSHOT);

// Or write straight to a file (returns the written path):
$path = Amber::save("https://example.com", Format::HTML, "out", "page");

// Capture once, emit many — one render serves every format:
$snap = Amber::snapshot("https://example.com", [Format::MARKDOWN, Format::PDF]);
$snapMd  = $snap->markdown();
$snapPdf = $snap->render(Format::PDF);
$snap->close();
```

`Format` constants: `HTML`, `MHTML`, `MARKDOWN`, `READABLE`, `WARC`, `WACZ`,
`SCREENSHOT`, `PDF`. Failures throw `Amber\CaptureException`.

By default the library is loaded from `lib/libamber_core.{dylib,so}`; set the
`AMBER_LIB` environment variable to load it from elsewhere. The first capture
that needs a browser downloads a pinned Chrome for Testing into the cache (set
`AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Note on distribution

FFI loads the native `amber-core` library at run time, so the published package
must ship (or fetch) the right `libamber_core` for the platform. The C
declarations are inlined in `src/Amber.php`, so only the library is staged.

## License

MIT OR Apache-2.0.
