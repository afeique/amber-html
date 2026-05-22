//! WARC/1.1 record serialization. See `Plans.md` (task 5.3).
//!
//! [`WarcWriter`] accumulates `warcinfo` and `response` records into a WARC
//! byte stream. This is the pure format layer; the capture pipeline supplies the
//! recorded HTTP exchanges (browser network recording is the remaining
//! integration). [`http_response_block`] builds the HTTP/1.1 response message
//! that a `response` record wraps.
//!
//! Record-IDs are content-addressed (`<urn:sha256:…>` over the URI + date +
//! block), so output is deterministic and records are uniquely identified
//! without a UUID dependency.

use crate::cache::content_hash;

/// Accumulates WARC/1.1 records into an in-memory byte buffer.
#[derive(Debug, Default)]
pub struct WarcWriter {
    buf: Vec<u8>,
}

impl WarcWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Write a `warcinfo` record. `fields` is the `application/warc-fields`
    /// block (e.g. `software: AmberHTML\r\nformat: WARC File Format 1.1`).
    pub fn warcinfo(&mut self, date: &str, fields: &str) {
        let block = fields.as_bytes();
        let id = record_id("warcinfo", date, block);
        self.write_record(
            &[
                ("WARC-Type", "warcinfo"),
                ("WARC-Date", date),
                ("WARC-Record-ID", &id),
                ("Content-Type", "application/warc-fields"),
            ],
            block,
        );
    }

    /// Write a `response` record wrapping the full HTTP response message
    /// (`http_response`, e.g. from [`http_response_block`]) fetched from
    /// `target_uri` at `date` (ISO 8601, e.g. `2026-01-01T00:00:00Z`).
    pub fn response(&mut self, target_uri: &str, date: &str, http_response: &[u8]) {
        let id = record_id(target_uri, date, http_response);
        self.write_record(
            &[
                ("WARC-Type", "response"),
                ("WARC-Target-URI", target_uri),
                ("WARC-Date", date),
                ("WARC-Record-ID", &id),
                ("Content-Type", "application/http;msgtype=response"),
            ],
            http_response,
        );
    }

    /// Consume the writer, returning the WARC bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    fn write_record(&mut self, headers: &[(&str, &str)], block: &[u8]) {
        self.buf.extend_from_slice(b"WARC/1.1\r\n");
        for (name, value) in headers {
            self.buf.extend_from_slice(name.as_bytes());
            self.buf.extend_from_slice(b": ");
            self.buf.extend_from_slice(value.as_bytes());
            self.buf.extend_from_slice(b"\r\n");
        }
        self.buf
            .extend_from_slice(format!("Content-Length: {}\r\n", block.len()).as_bytes());
        self.buf.extend_from_slice(b"\r\n");
        self.buf.extend_from_slice(block);
        // Each record is terminated by two CRLFs.
        self.buf.extend_from_slice(b"\r\n\r\n");
    }
}

/// Build a full HTTP/1.1 response message (status line + minimal headers +
/// body) suitable as the block of a WARC `response` record.
pub fn http_response_block(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        format!("HTTP/1.1 {status} {}\r\n", reason_phrase(status)).as_bytes(),
    );
    out.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    out.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    out
}

/// A content-addressed, angle-bracketed WARC-Record-ID.
fn record_id(uri: &str, date: &str, block: &[u8]) -> String {
    let mut material = Vec::with_capacity(uri.len() + date.len() + block.len());
    material.extend_from_slice(uri.as_bytes());
    material.extend_from_slice(date.as_bytes());
    material.extend_from_slice(block);
    format!("<urn:sha256:{}>", content_hash(&material))
}

/// Reason phrase for the common status codes; empty for the rest.
fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the `Content-Length` header value out of a record's header block.
    fn content_length(record: &str) -> usize {
        record
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .and_then(|v| v.trim().parse().ok())
            .expect("Content-Length present")
    }

    #[test]
    fn http_response_block_is_well_formed() {
        let block = http_response_block(200, "text/html", b"<html>hi</html>");
        let s = String::from_utf8(block).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(s.contains("Content-Type: text/html\r\n"));
        assert!(s.contains("Content-Length: 15\r\n")); // body is 15 bytes
        assert!(s.ends_with("\r\n\r\n<html>hi</html>"));
    }

    #[test]
    fn warcinfo_record_structure() {
        let mut w = WarcWriter::new();
        w.warcinfo("2026-01-01T00:00:00Z", "software: AmberHTML");
        let out = String::from_utf8(w.into_bytes()).unwrap();
        assert!(out.starts_with("WARC/1.1\r\n"));
        assert!(out.contains("WARC-Type: warcinfo\r\n"));
        assert!(out.contains("Content-Type: application/warc-fields\r\n"));
        assert!(out.contains("WARC-Record-ID: <urn:sha256:"));
        // Content-Length matches the block ("software: AmberHTML" = 19 bytes).
        assert_eq!(content_length(&out), 19);
        assert!(out.ends_with("\r\n\r\n"));
    }

    #[test]
    fn response_record_wraps_http_message() {
        let mut w = WarcWriter::new();
        let block = http_response_block(200, "text/html", b"<html></html>");
        w.response("https://example.com/", "2026-01-01T00:00:00Z", &block);
        let out = String::from_utf8(w.into_bytes()).unwrap();
        assert!(out.contains("WARC-Type: response\r\n"));
        assert!(out.contains("WARC-Target-URI: https://example.com/\r\n"));
        assert!(out.contains("Content-Type: application/http;msgtype=response\r\n"));
        assert!(out.contains("HTTP/1.1 200 OK"), "wraps the HTTP response");
        // The record's Content-Length equals the HTTP block length.
        assert_eq!(content_length(&out), block.len());
    }

    #[test]
    fn multiple_records_concatenate() {
        let mut w = WarcWriter::new();
        w.warcinfo("2026-01-01T00:00:00Z", "software: AmberHTML");
        w.response(
            "https://example.com/",
            "2026-01-01T00:00:00Z",
            &http_response_block(200, "text/html", b"x"),
        );
        let out = String::from_utf8(w.into_bytes()).unwrap();
        // Two records → two "WARC/1.1" version lines.
        assert_eq!(out.matches("WARC/1.1\r\n").count(), 2);
    }

    #[test]
    fn record_ids_differ_for_different_content() {
        let a = record_id("https://a.com/", "2026-01-01T00:00:00Z", b"one");
        let b = record_id("https://a.com/", "2026-01-01T00:00:00Z", b"two");
        assert_ne!(a, b);
        assert!(a.starts_with("<urn:sha256:") && a.ends_with('>'));
    }
}
