import uniffi.amber_core.CaptureException
import uniffi.amber_core.OutputFormat
import uniffi.amber_core.capture
import uniffi.amber_core.captureMarkdown
import uniffi.amber_core.save
import uniffi.amber_core.snapshot
import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertTrue
import kotlin.test.assertFailsWith

class SmokeTest {
    // A data: URL keeps the test self-contained while still exercising the
    // real capture pipeline through the bundled engine.
    private val url =
        "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"

    @Test
    fun markdown() {
        val md = captureMarkdown(url)
        assertContains(md, "Smoke")
    }

    @Test
    fun binaryFormats() {
        val pdf = capture(url, OutputFormat.PDF)
        assertTrue(pdf.size > 4)
        assertTrue(pdf.copyOfRange(0, 4).contentEquals("%PDF".toByteArray()))

        val png = capture(url, OutputFormat.SCREENSHOT)
        assertTrue(png.copyOfRange(0, 4).contentEquals(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)))
    }

    @Test
    fun saveToFile() {
        val dir = System.getProperty("java.io.tmpdir") + "/amber-kt-smoke"
        val path = save(url, OutputFormat.HTML, dir, "page")
        assertTrue(path.endsWith("page.html"))
        assertTrue(java.io.File(path).exists())
    }

    @Test
    fun badUrlThrows() {
        assertFailsWith<CaptureException.Failed> { captureMarkdown("not a url") }
    }

    @Test
    fun snapshotRendersManyFromOneCapture() {
        // One capture, many formats (Plans.md 10.1/10.3); Snapshot is AutoCloseable.
        snapshot(url, listOf(OutputFormat.MARKDOWN, OutputFormat.PDF)).use { snap ->
            assertContains(snap.markdown(), "Smoke")

            val pdf = snap.render(OutputFormat.PDF)
            assertTrue(pdf.copyOfRange(0, 4).contentEquals("%PDF".toByteArray()))

            val dir = System.getProperty("java.io.tmpdir") + "/amber-kt-smoke"
            val path = snap.save(OutputFormat.READABLE, dir, "snap")
            assertTrue(path.endsWith("snap.txt"))
            assertTrue(java.io.File(path).exists())
        }
    }

    @Test
    fun snapshotBadUrlThrows() {
        assertFailsWith<CaptureException.Failed> {
            snapshot("not a url", listOf(OutputFormat.MARKDOWN))
        }
    }
}
