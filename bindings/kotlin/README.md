# AmberHTML for Kotlin / Java

Kotlin/JVM bindings for [AmberHTML](https://github.com/afeique/amber-html) — a
local-first web-page capture engine. Generated with
[UniFFI](https://mozilla.github.io/uniffi-rs/); the native library is loaded via
[JNA](https://github.com/java-native-access/jna) and bundled in the jar. Usable
from Java as well (it's plain JVM bytecode).

## Add the dependency

```kotlin
// build.gradle.kts
dependencies {
    implementation("io.github.afeique:amber-html:0.1.0")
}
```

## Usage

```kotlin
import uniffi.amber_core.*

// Text formats:
val md = captureMarkdown("https://example.com")
val text = captureReadable("https://example.com")

// Any format as a ByteArray (binary formats too):
val pdf = capture("https://example.com", OutputFormat.PDF)
val png = capture("https://example.com", OutputFormat.SCREENSHOT)

// Or write straight to a file (returns the written path):
val path = save("https://example.com", OutputFormat.HTML, "out", "page")

// Failures throw CaptureException.Failed.
```

`OutputFormat` values: `HTML`, `MHTML`, `MARKDOWN`, `READABLE`, `WARC`, `WACZ`,
`SCREENSHOT`, `PDF`.

The first capture that needs a browser downloads a pinned Chrome for Testing
into the cache (set `AMBER_CHROMIUM_PATH` to reuse an existing Chromium).

## Building from source

```sh
bindings/kotlin/generate.sh        # builds the cdylib + generates the binding + bundles the lib
cd bindings/kotlin && ./gradlew test && ./gradlew build
```

`generate.sh` places the JNA-loadable native library under
`src/main/resources/<os-arch>/`. Published jars bundle every supported platform's
library (assembled in CI).

## License

MIT OR Apache-2.0.
