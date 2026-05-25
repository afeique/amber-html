# AmberHTML for Dart / Flutter

Dart bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Uses `dart:ffi` over the `amber-core` C
ABI (Dart SDK ≥ 3.0).

## Build

It loads a native library, so stage it from the repo first:

```sh
bindings/dart/generate.sh         # builds the native lib into native/
cd bindings/dart && dart pub get && dart test
```

## Usage

```dart
import 'package:amber_html/amber.dart';

// Text formats:
final md = captureMarkdown('https://example.com');
final text = captureReadable('https://example.com');

// Any format as bytes (Uint8List; binary formats too):
final pdf = capture('https://example.com', Format.pdf);
final png = capture('https://example.com', Format.screenshot);

// Or write straight to a file (returns the written path):
final path = save('https://example.com', Format.html, 'out', 'page');

// Capture once, emit many — one render serves every format:
final snap = Snapshot.capture('https://example.com', [Format.markdown, Format.pdf]);
final snapMd = snap.markdown();
final snapPdf = snap.render(Format.pdf);
snap.close();
```

`Format` values: `html`, `mhtml`, `markdown`, `readable`, `warc`, `wacz`,
`screenshot`, `pdf`. Failures throw `CaptureException`.

By default the library loads from `native/libamber_core.{dylib,so}` (relative to
the working directory); set `AMBER_LIB` to an absolute path to load it from
elsewhere. The first capture that needs a browser downloads a pinned Chrome for
Testing into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Note on distribution

`dart:ffi` loads the native `amber-core` library at run time, so a published
package must ship (or fetch) the right `libamber_core` per platform. For Flutter,
bundle it as a plugin asset.

## License

MIT OR Apache-2.0.
