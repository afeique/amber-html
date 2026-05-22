//! WACZ packaging: bundle a WARC into a Web Archive Collection Zipped (WACZ).
//! See `Plans.md` (task 5.4).
//!
//! [`package`] produces a `.wacz` (a ZIP) containing the WARC under
//! `archive/data.warc`, a frictionless `datapackage.json` describing it (with a
//! SHA-256 of the WARC), and a `pages/pages.jsonl` page list. This is the
//! packaging layer; producing a fully replay-tool-indexed archive (CDXJ) is the
//! remaining work. Build the WARC with [`crate::warc`].

use std::io::{Cursor, Write};

use serde_json::json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::cache::content_hash;
use crate::error::{Error, Result};

/// Package `warc` bytes plus a `pages` list (`(url, iso_timestamp)`) into WACZ
/// (ZIP) bytes.
pub fn package(warc: &[u8], pages: &[(&str, &str)]) -> Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    let map_zip = |e: zip::result::ZipError| Error::Browser(format!("WACZ zip error: {e}"));

    // 1. The WARC payload.
    zip.start_file("archive/data.warc", opts).map_err(map_zip)?;
    zip.write_all(warc)?;

    // 2. pages/pages.jsonl — a header line followed by one line per page.
    let mut pages_jsonl =
        json!({ "format": "json-pages-1.0", "id": "pages", "title": "Pages" }).to_string();
    for (url, ts) in pages {
        pages_jsonl.push('\n');
        pages_jsonl.push_str(&json!({ "url": url, "ts": ts }).to_string());
    }
    zip.start_file("pages/pages.jsonl", opts).map_err(map_zip)?;
    zip.write_all(pages_jsonl.as_bytes())?;

    // 3. datapackage.json — frictionless descriptor of the resources.
    let datapackage = json!({
        "profile": "data-package",
        "resources": [{
            "name": "data.warc",
            "path": "archive/data.warc",
            "hash": format!("sha256:{}", content_hash(warc)),
            "bytes": warc.len(),
        }],
        "software": "AmberHTML",
        "wacz_version": "1.1.1",
    });
    zip.start_file("datapackage.json", opts).map_err(map_zip)?;
    zip.write_all(
        serde_json::to_string_pretty(&datapackage)
            .unwrap_or_default()
            .as_bytes(),
    )?;

    let cursor = zip.finish().map_err(map_zip)?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        let mut file = archive.by_name(name).unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        buf
    }

    #[test]
    fn package_contains_expected_entries() {
        let warc = b"WARC/1.1\r\nWARC-Type: warcinfo\r\n\r\n";
        let bytes = package(warc, &[("https://example.com/", "2026-01-01T00:00:00Z")]).unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"archive/data.warc".to_string()));
        assert!(names.contains(&"datapackage.json".to_string()));
        assert!(names.contains(&"pages/pages.jsonl".to_string()));
    }

    #[test]
    fn warc_payload_round_trips() {
        let warc = b"WARC/1.1\r\nWARC-Type: response\r\n\r\nbody";
        let bytes = package(warc, &[]).unwrap();
        assert_eq!(entry(&bytes, "archive/data.warc"), warc);
    }

    #[test]
    fn datapackage_describes_the_warc() {
        let warc = b"hello warc";
        let bytes = package(warc, &[]).unwrap();
        let dp: serde_json::Value =
            serde_json::from_slice(&entry(&bytes, "datapackage.json")).unwrap();
        let res = &dp["resources"][0];
        assert_eq!(res["path"], "archive/data.warc");
        assert_eq!(res["bytes"], warc.len());
        assert_eq!(res["hash"], format!("sha256:{}", content_hash(warc)));
    }

    #[test]
    fn pages_jsonl_has_header_and_one_line_per_page() {
        let bytes = package(
            b"warc",
            &[
                ("https://example.com/a", "2026-01-01T00:00:00Z"),
                ("https://example.com/b", "2026-01-02T00:00:00Z"),
            ],
        )
        .unwrap();
        let pages = String::from_utf8(entry(&bytes, "pages/pages.jsonl")).unwrap();
        let lines: Vec<&str> = pages.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 pages
        assert!(lines[0].contains("json-pages-1.0"));
        assert!(lines[1].contains("https://example.com/a"));
    }
}
