//! WACZ packaging: bundle a WARC into a Web Archive Collection Zipped (WACZ).
//! See `Plans.md` (task 5.4).
//!
//! [`package`] produces a `.wacz` (a ZIP) containing the WARC under
//! `archive/data.warc`, a frictionless `datapackage.json` describing it (with a
//! SHA-256 of the WARC), a `pages/pages.jsonl` page list, and an
//! `indexes/index.cdx` CDXJ index that points a replay tool (pywb /
//! ReplayWeb.page) at each response record by byte offset. Build the WARC with
//! [`crate::warc`].

use std::io::{Cursor, Write};

use serde_json::json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::cache::content_hash;
use crate::error::{Error, Result};
use crate::warc::RecordLoc;

/// The WARC's path inside the WACZ ZIP, and the bare name used in the CDXJ.
const WARC_PATH: &str = "archive/data.warc";
const WARC_NAME: &str = "data.warc";

/// Package `warc` bytes plus a `pages` list (`(url, iso_timestamp)`) and the
/// WARC's response-record locations (for the CDXJ index) into WACZ (ZIP) bytes.
pub fn package(warc: &[u8], pages: &[(&str, &str)], records: &[RecordLoc]) -> Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    let map_zip = |e: zip::result::ZipError| Error::Browser(format!("WACZ zip error: {e}"));

    // 1. The WARC payload.
    zip.start_file(WARC_PATH, opts).map_err(map_zip)?;
    zip.write_all(warc)?;

    // 2. indexes/index.cdx — CDXJ pointing at each response record by offset.
    zip.start_file("indexes/index.cdx", opts).map_err(map_zip)?;
    zip.write_all(cdxj_index(records).as_bytes())?;

    // 3. pages/pages.jsonl — a header line followed by one line per page.
    let mut pages_jsonl =
        json!({ "format": "json-pages-1.0", "id": "pages", "title": "Pages" }).to_string();
    for (url, ts) in pages {
        pages_jsonl.push('\n');
        pages_jsonl.push_str(&json!({ "url": url, "ts": ts }).to_string());
    }
    zip.start_file("pages/pages.jsonl", opts).map_err(map_zip)?;
    zip.write_all(pages_jsonl.as_bytes())?;

    // 4. datapackage.json — frictionless descriptor of the resources.
    let datapackage = json!({
        "profile": "data-package",
        "resources": [{
            "name": WARC_NAME,
            "path": WARC_PATH,
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

/// Build a CDXJ index (one sortable line per response record) referencing the
/// WARC stored at [`WARC_NAME`]. Each line is `<surt> <ts14> <json>`, the CDXJ
/// shape pywb / ReplayWeb.page expect: a SURT-canonicalized key, a 14-digit
/// timestamp, and a JSON record with the byte `offset`/`length` into the WARC.
pub fn cdxj_index(records: &[RecordLoc]) -> String {
    let mut lines: Vec<String> = records
        .iter()
        .map(|r| {
            let meta = json!({
                "url": r.target_uri,
                "mime": "text/html",
                "status": "200",
                "digest": format!("sha256:{}", r.block_digest),
                "length": r.length.to_string(),
                "offset": r.offset.to_string(),
                "filename": WARC_NAME,
            });
            format!("{} {} {}", surt(&r.target_uri), timestamp14(&r.date), meta)
        })
        .collect();
    // CDXJ must be sorted by the surt+timestamp key for binary search at replay.
    lines.sort();
    lines.join("\n")
}

/// SURT-canonicalize a URL: `https://www.example.com/p?q=1` →
/// `com,example,www)/p?q=1` (host labels reversed, scheme dropped). An
/// unparseable URL is returned unchanged.
fn surt(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return url.to_string();
    };
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let mut labels: Vec<&str> = host.split('.').collect();
    labels.reverse();
    let mut out = labels.join(",");
    out.push(')');
    out.push_str(parsed.path());
    if let Some(query) = parsed.query() {
        out.push('?');
        out.push_str(query);
    }
    out
}

/// Compact an ISO 8601 timestamp to the 14-digit CDXJ form
/// (`2026-01-02T03:04:05Z` → `20260102030405`).
fn timestamp14(iso: &str) -> String {
    iso.chars().filter(char::is_ascii_digit).take(14).collect()
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

    fn rec(uri: &str, offset: usize, length: usize) -> RecordLoc {
        RecordLoc {
            target_uri: uri.to_string(),
            date: "2026-01-02T03:04:05Z".to_string(),
            offset,
            length,
            block_digest: "abc123".to_string(),
        }
    }

    #[test]
    fn package_contains_expected_entries() {
        let warc = b"WARC/1.1\r\nWARC-Type: warcinfo\r\n\r\n";
        let bytes = package(
            warc,
            &[("https://example.com/", "2026-01-01T00:00:00Z")],
            &[],
        )
        .unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"archive/data.warc".to_string()));
        assert!(names.contains(&"datapackage.json".to_string()));
        assert!(names.contains(&"pages/pages.jsonl".to_string()));
        assert!(names.contains(&"indexes/index.cdx".to_string()));
    }

    #[test]
    fn warc_payload_round_trips() {
        let warc = b"WARC/1.1\r\nWARC-Type: response\r\n\r\nbody";
        let bytes = package(warc, &[], &[]).unwrap();
        assert_eq!(entry(&bytes, "archive/data.warc"), warc);
    }

    #[test]
    fn datapackage_describes_the_warc() {
        let warc = b"hello warc";
        let bytes = package(warc, &[], &[]).unwrap();
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
            &[],
        )
        .unwrap();
        let pages = String::from_utf8(entry(&bytes, "pages/pages.jsonl")).unwrap();
        let lines: Vec<&str> = pages.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 pages
        assert!(lines[0].contains("json-pages-1.0"));
        assert!(lines[1].contains("https://example.com/a"));
    }

    #[test]
    fn surt_reverses_host_and_drops_scheme() {
        assert_eq!(
            surt("https://www.example.com/p?q=1"),
            "com,example,www)/p?q=1"
        );
        assert_eq!(surt("http://ex.com/"), "com,ex)/");
    }

    #[test]
    fn timestamp14_compacts_iso_8601() {
        assert_eq!(timestamp14("2026-01-02T03:04:05Z"), "20260102030405");
    }

    #[test]
    fn cdxj_index_line_carries_offset_length_and_surt_key() {
        let index = cdxj_index(&[rec("https://example.com/page", 42, 1000)]);
        let mut parts = index.splitn(3, ' ');
        assert_eq!(parts.next().unwrap(), "com,example)/page"); // SURT key
        assert_eq!(parts.next().unwrap(), "20260102030405"); // 14-digit ts
        let meta: serde_json::Value = serde_json::from_str(parts.next().unwrap()).unwrap();
        assert_eq!(meta["offset"], "42");
        assert_eq!(meta["length"], "1000");
        assert_eq!(meta["filename"], "data.warc");
        assert_eq!(meta["url"], "https://example.com/page");
    }

    #[test]
    fn cdxj_index_is_sorted_by_key() {
        let index = cdxj_index(&[
            rec("https://example.com/z", 0, 1),
            rec("https://example.com/a", 1, 1),
        ]);
        let keys: Vec<&str> = index
            .lines()
            .map(|l| l.split(' ').next().unwrap())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "CDXJ lines must be sorted by surt key");
    }

    /// The CDXJ offset/length point at a real WARC record boundary end-to-end.
    #[test]
    fn cdxj_offsets_address_the_real_record_in_the_packaged_warc() {
        use crate::warc::{http_response_block, WarcWriter};

        let mut w = WarcWriter::new();
        w.warcinfo("2026-01-02T03:04:05Z", "software: AmberHTML");
        let block = http_response_block(200, "text/html", b"<html>hi</html>");
        w.response("https://example.com/", "2026-01-02T03:04:05Z", &block);
        let records = w.records().to_vec();
        let warc = w.into_bytes();

        let wacz = package(&warc, &[], &records).unwrap();
        let stored_warc = entry(&wacz, "archive/data.warc");
        let cdx = String::from_utf8(entry(&wacz, "indexes/index.cdx")).unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(cdx.splitn(3, ' ').nth(2).unwrap()).unwrap();
        let offset: usize = meta["offset"].as_str().unwrap().parse().unwrap();
        let length: usize = meta["length"].as_str().unwrap().parse().unwrap();
        let record = &stored_warc[offset..offset + length];
        assert!(record.starts_with(b"WARC/1.1\r\n"));
        assert!(std::str::from_utf8(record)
            .unwrap()
            .contains("WARC-Type: response\r\n"));
    }
}
