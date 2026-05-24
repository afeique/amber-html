# AmberHTML for .NET

C#/.NET bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. P/Invoke over the `amber-core` C ABI; the
native library ships per-RID in the NuGet package.

## Install

```sh
dotnet add package AmberHtml
```

## Usage

```csharp
using AmberHtml;

// Text formats:
string md   = Amber.CaptureMarkdown("https://example.com");
string text = Amber.CaptureReadable("https://example.com");

// Any format as bytes (binary formats too):
byte[] pdf = Amber.Capture("https://example.com", Format.Pdf);
byte[] png = Amber.Capture("https://example.com", Format.Screenshot);

// Or write straight to a file (returns the written path):
string path = Amber.Save("https://example.com", Format.Html, "out", "page");

// Capture once, emit many — one render serves every format:
using var snap = Amber.Snapshot("https://example.com", Format.Markdown, Format.Pdf);
string snapMd  = snap.Markdown();
byte[] snapPdf = snap.Render(Format.Pdf);

// Failures throw AmberException.
```

`Format` values: `Html`, `Mhtml`, `Markdown`, `Readable`, `Warc`, `Wacz`,
`Screenshot`, `Pdf`.

The first capture that needs a browser downloads a pinned Chrome for Testing
into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Building from source

```sh
bindings/csharp/generate.sh                          # builds + stages the native lib per RID
cd bindings/csharp && dotnet test                    # runs the smoke tests
dotnet pack src/AmberHtml/AmberHtml.csproj -c Release # builds the NuGet package
```

`generate.sh` stages `libamber_core` under `src/AmberHtml/runtimes/<rid>/native/`;
.NET's RID resolution loads it. The published package bundles every supported
RID's native library (assembled in CI).

## License

MIT OR Apache-2.0.
