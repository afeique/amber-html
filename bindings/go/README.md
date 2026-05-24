# AmberHTML for Go

Go bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Wraps the `amber-core` C ABI via cgo.

## Build

Because it links a native library, build it from the repo (cgo needs the header
and library staged):

```sh
bindings/go/generate.sh         # builds the native lib + stages include/ and lib/
cd bindings/go && go test ./...
```

```go
import amber "github.com/afeique/amber-html/bindings/go"
```

## Usage

```go
// Text formats:
md, err := amber.CaptureMarkdown("https://example.com")
text, err := amber.CaptureReadable("https://example.com")

// Any format as bytes (binary formats too):
pdf, err := amber.Capture("https://example.com", amber.FormatPDF)
png, err := amber.Capture("https://example.com", amber.FormatScreenshot)

// Or write straight to a file (returns the written path):
path, err := amber.Save("https://example.com", amber.FormatHTML, "out", "page")
```

`Format` values: `FormatHTML`, `FormatMHTML`, `FormatMarkdown`, `FormatReadable`,
`FormatWARC`, `FormatWACZ`, `FormatScreenshot`, `FormatPDF`. Errors are
`ErrInvalidInput` (bad argument/format) or `ErrCapture` (the capture failed);
check with `errors.Is`.

The first capture that needs a browser downloads a pinned Chrome for Testing
into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Note on distribution

cgo links the native `amber-core` library, so this isn't a pure `go get`
package: the library must be built (via `generate.sh`) and locatable at run time
(the build embeds an rpath to `lib/`). A standalone, prebuilt-per-platform
companion module can be published separately.

## License

MIT OR Apache-2.0.
