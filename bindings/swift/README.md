# AmberHTML for Swift

Swift bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Generated with
[UniFFI](https://mozilla.github.io/uniffi-rs/); the native engine ships as an
`xcframework` binary target.

## Add the package

```swift
// Package.swift
dependencies: [
    .package(url: "https://github.com/afeique/amber-html", from: "0.1.0")
]
```

(Released versions point the binary target at a zipped `xcframework` attached to
the GitHub Release.)

## Usage

```swift
import AmberHTML

// Text formats:
let md = try captureMarkdown(url: "https://example.com")
let text = try captureReadable(url: "https://example.com")

// Any format as Data (binary formats too):
let pdf = try capture(url: "https://example.com", format: .pdf)
let png = try capture(url: "https://example.com", format: .screenshot)

// Or write straight to a file (returns the written path):
let path = try save(url: "https://example.com", format: .html, dir: "out", name: "page")

// Capture once, emit many — one render serves every format:
let snap = try snapshot(url: "https://example.com", formats: [.markdown, .pdf])
let snapMd = try snap.markdown()
let snapPdf = try snap.render(format: .pdf)

// Failures throw CaptureError.Failed(reason:).
```

`OutputFormat` cases: `.html`, `.mhtml`, `.markdown`, `.readable`, `.warc`,
`.wacz`, `.screenshot`, `.pdf`.

The first capture that needs a browser downloads a pinned Chrome for Testing
into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Building from source

```sh
bindings/swift/build-xcframework.sh   # builds the native staticlib + xcframework
bindings/swift/generate.sh            # generates Sources/AmberHTML/amber_core.swift
cd bindings/swift && swift build && swift test
```

The committed `Package.swift` builds for the host macOS arch. The release
xcframework adds the other Apple slices (macOS arm64/x86_64, iOS, simulators).

## License

MIT OR Apache-2.0.
