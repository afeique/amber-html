# AmberHTML for Ruby

Ruby bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Drives a pinned, auto-managed Chromium over
the CDP debug pipe (only when a page needs a browser) and emits Markdown,
readable text, single-file HTML, MHTML, WARC/WACZ, a screenshot, or a PDF.

The binding is generated with [UniFFI](https://mozilla.github.io/uniffi-rs/)
and loads a bundled native library via the [`ffi`](https://rubygems.org/gems/ffi)
gem.

## Install

```sh
gem install amber-html
```

## Usage

```ruby
require "amber_html"

# Text formats:
md   = AmberHtml.capture_markdown("https://example.com")
text = AmberHtml.capture_readable("https://example.com")

# Any format as bytes (binary formats too):
pdf = AmberHtml.capture("https://example.com", AmberHtml::OutputFormat::PDF)
png = AmberHtml.capture("https://example.com", AmberHtml::OutputFormat::SCREENSHOT)

# Or write straight to a file (returns the written path):
path = AmberHtml.save("https://example.com", AmberHtml::OutputFormat::HTML, "out", "page")

# Failures surface as AmberHtml::CaptureError::Failed.
```

`OutputFormat` values: `HTML`, `MHTML`, `MARKDOWN`, `READABLE`, `WARC`, `WACZ`,
`SCREENSHOT`, `PDF`.

The first capture that needs a browser downloads a pinned Chrome for Testing
into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Building from source

```sh
bindings/ruby/generate.sh          # builds the cdylib + generates the binding
ruby bindings/ruby/test/smoke.rb   # optional: run the smoke test
gem build bindings/ruby/amber-html.gemspec
```

`generate.sh` compiles `amber-core` as a `cdylib`, runs `uniffi-bindgen`, and
bundles the native library. Published gems are platform-specific (one per OS/arch),
built in CI.

## License

MIT OR Apache-2.0.
