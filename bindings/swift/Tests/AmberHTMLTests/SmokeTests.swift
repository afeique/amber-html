import XCTest
@testable import AmberHTML

final class SmokeTests: XCTestCase {
    // A data: URL keeps the test self-contained (still exercises the real
    // capture pipeline through the bundled engine).
    let url = "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"

    func testMarkdown() throws {
        let md = try captureMarkdown(url: url)
        XCTAssertTrue(md.contains("Smoke"))
    }

    func testBinaryFormats() throws {
        let pdf = try capture(url: url, format: .pdf)
        XCTAssertGreaterThan(pdf.count, 4)
        XCTAssertEqual(Array(pdf.prefix(4)), Array("%PDF".utf8))

        let png = try capture(url: url, format: .screenshot)
        XCTAssertEqual(Array(png.prefix(4)), [0x89, 0x50, 0x4E, 0x47])
    }

    func testSave() throws {
        let dir = NSTemporaryDirectory() + "amber-swift-smoke"
        let path = try save(url: url, format: .html, dir: dir, name: "page")
        XCTAssertTrue(path.hasSuffix("page.html"))
        XCTAssertTrue(FileManager.default.fileExists(atPath: path))
    }

    func testBadURLThrows() {
        XCTAssertThrowsError(try captureMarkdown(url: "not a url"))
    }
}
