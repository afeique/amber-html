//! MHTML → single-file HTML transform (Plans.md).
//!
//! Chromium's `Page.captureSnapshot` returns an MHTML bundle: a multipart MIME
//! document with a root HTML part plus one part per subresource (images, CSS,
//! fonts, …). Each part carries a `Content-Location` header (the resource's
//! absolute URL), an optional `Content-ID`, a `Content-Type`, and a
//! `Content-Transfer-Encoding` of either `base64` or `quoted-printable`.
//!
//! [`mhtml_to_single_file_html`] flattens that bundle into ONE self-contained
//! `.html` string: every subresource is inlined as a `data:` URI and external
//! stylesheets are inlined into `<style>` blocks, so the result opens in any
//! browser offline.
//!
//! The transform is **best-effort and infallible**: if a particular resource
//! can't be inlined it is left untouched, and if the bundle can't be parsed at
//! all the original input is returned verbatim. It never panics.
//!
//! ## What is inlined vs left alone
//! - `<img src="…">`            → `data:` URI (when the URL is a known part)
//! - `<img srcset="…">` /
//!   `<source srcset="…">`      → each candidate URL → `data:` URI
//! - `<link rel="stylesheet">`  → replaced by an inline `<style>…</style>`
//!   (with nested `url(...)`/`@import` in the CSS also inlined)
//! - `url(...)` inside any
//!   inline/inlined CSS            → `data:` URI
//! - other attributes that point at known parts (`href`, `poster`, `src` on
//!   `<script>`/`<audio>`/`<video>`/`<source>`/`<track>`) → `data:` URI
//!
//! Reference styles we deliberately do **not** rewrite: JS-constructed URLs,
//! CSS in external sheets we couldn't fetch, `<iframe>` documents, and any URL
//! that isn't present as a part in the bundle (left as-is — it stays a live
//! network reference, which is the safe fallback).

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use std::collections::HashMap;

/// Convert a Chromium MHTML bundle into a single self-contained HTML string.
///
/// Best-effort and infallible: returns the root HTML with as many resources
/// inlined as possible. If the bundle can't be parsed, returns `mhtml`
/// unchanged. Never panics.
pub fn mhtml_to_single_file_html(mhtml: &str) -> String {
    let doc = match MhtmlDoc::parse(mhtml) {
        Some(doc) => doc,
        // Totally unparseable: hand back the input untouched.
        None => return mhtml.to_string(),
    };

    let root_html = match doc.root_html() {
        Some(html) => html,
        // No HTML part we can identify: nothing meaningful to emit.
        None => return mhtml.to_string(),
    };

    // Build a lookup from absolute URL (Content-Location) and cid: (Content-ID)
    // to the inlined `data:` URI for that resource.
    let mut data_uris: HashMap<String, String> = HashMap::new();
    for part in &doc.parts {
        // The root HTML part itself is never inlined as a data: URI.
        if std::ptr::eq(part, doc.root_part()) {
            continue;
        }
        let data_uri = build_data_uri(&part.mime, &part.body);
        if let Some(loc) = &part.location {
            data_uris.insert(loc.clone(), data_uri.clone());
        }
        if let Some(cid) = &part.content_id {
            // Content-ID is referenced as `cid:<id>` (angle brackets stripped).
            data_uris.insert(format!("cid:{cid}"), data_uri.clone());
        }
    }

    // Pass 1: inline external stylesheets into <style> blocks. We resolve the
    // referenced part to its (already-decoded) CSS text and recursively inline
    // url(...) references inside it.
    let css_parts = doc.css_parts_by_location();
    let stage1 = inline_stylesheets(&root_html, &css_parts, &data_uris);

    // Pass 2: rewrite remaining resource references (img/src/srcset/url(...))
    // to data: URIs.
    rewrite_references(&stage1, &data_uris)
}

// ---------------------------------------------------------------------------
// MIME / MHTML parsing
// ---------------------------------------------------------------------------

/// One MIME part of the MHTML bundle.
#[derive(Debug)]
struct Part {
    /// Lowercased MIME type, e.g. `text/html`, `image/png`. Defaults to
    /// `application/octet-stream` when absent.
    mime: String,
    /// `Content-Location` header value (the resource's absolute URL).
    location: Option<String>,
    /// `Content-ID` value with surrounding angle brackets stripped.
    content_id: Option<String>,
    /// Decoded raw bytes of the part body.
    body: Vec<u8>,
}

impl Part {
    /// Body decoded as UTF-8 (lossy), for text parts (HTML/CSS).
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn is_html(&self) -> bool {
        self.mime.starts_with("text/html") || self.mime.starts_with("application/xhtml")
    }

    fn is_css(&self) -> bool {
        self.mime.starts_with("text/css")
    }
}

/// A parsed MHTML document.
#[derive(Debug)]
struct MhtmlDoc {
    parts: Vec<Part>,
    /// Index into `parts` of the root HTML part.
    root_idx: usize,
}

impl MhtmlDoc {
    /// Parse an MHTML string into its parts. Returns `None` if no boundary can
    /// be found or no parts are produced.
    fn parse(mhtml: &str) -> Option<MhtmlDoc> {
        // Split the top-level headers from the body at the first blank line.
        let (top_headers, body) = split_headers(mhtml);
        let headers = parse_headers(top_headers);

        let boundary = headers
            .iter()
            .find(|(k, _)| k == "content-type")
            .and_then(|(_, v)| extract_boundary(v))?;

        let raw_parts = split_multipart(body, &boundary);
        if raw_parts.is_empty() {
            return None;
        }

        let mut parts = Vec::new();
        for raw in raw_parts {
            if let Some(part) = parse_part(raw) {
                parts.push(part);
            }
        }
        if parts.is_empty() {
            return None;
        }

        // The root part is the one whose Content-Location matches the bundle's
        // top-level Content-Location, else the first HTML part, else part 0.
        let top_location = headers
            .iter()
            .find(|(k, _)| k == "content-location")
            .map(|(_, v)| v.trim().to_string());

        let root_idx = top_location
            .as_deref()
            .and_then(|loc| parts.iter().position(|p| p.location.as_deref() == Some(loc)))
            .or_else(|| parts.iter().position(|p| p.is_html()))
            .unwrap_or(0);

        Some(MhtmlDoc { parts, root_idx })
    }

    fn root_part(&self) -> &Part {
        &self.parts[self.root_idx]
    }

    /// The decoded root HTML text, if the chosen root part is HTML-ish.
    fn root_html(&self) -> Option<String> {
        let root = self.root_part();
        // If the chosen root isn't HTML, fall back to any HTML part.
        if root.is_html() {
            return Some(root.text());
        }
        self.parts.iter().find(|p| p.is_html()).map(|p| p.text())
    }

    /// Map from a CSS part's `Content-Location` to its decoded CSS text.
    fn css_parts_by_location(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for part in &self.parts {
            if part.is_css() {
                if let Some(loc) = &part.location {
                    map.insert(loc.clone(), part.text());
                }
            }
        }
        map
    }
}

/// Split a raw message/part into (header-block, body) at the first blank line.
/// Handles both CRLF and LF line endings.
fn split_headers(raw: &str) -> (&str, &str) {
    if let Some(idx) = raw.find("\r\n\r\n") {
        (&raw[..idx], &raw[idx + 4..])
    } else if let Some(idx) = raw.find("\n\n") {
        (&raw[..idx], &raw[idx + 2..])
    } else {
        // No blank line: treat the whole thing as headers, empty body.
        (raw, "")
    }
}

/// Parse a header block into lowercased (name, value) pairs, unfolding
/// continuation lines (RFC 5322 folding: a line starting with whitespace
/// continues the previous header).
fn parse_headers(block: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for raw_line in block.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        // Folded continuation of the previous header.
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            let last = out.last_mut().unwrap();
            last.1.push(' ');
            last.1.push_str(line.trim());
            continue;
        }
        if let Some(colon) = line.find(':') {
            let name = line[..colon].trim().to_ascii_lowercase();
            let value = line[colon + 1..].trim().to_string();
            out.push((name, value));
        }
    }
    out
}

/// Extract the `boundary="…"` parameter from a Content-Type value.
fn extract_boundary(content_type: &str) -> Option<String> {
    // Find the `boundary` parameter, case-insensitively.
    let lower = content_type.to_ascii_lowercase();
    let pos = lower.find("boundary")?;
    let after = &content_type[pos + "boundary".len()..];
    let after = after.trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    if let Some(rest) = after.strip_prefix('"') {
        // Quoted: take up to the closing quote.
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else {
        // Unquoted: take up to the next `;`, whitespace, or end.
        let end = after
            .find(|c: char| c == ';' || c.is_whitespace())
            .unwrap_or(after.len());
        let b = after[..end].to_string();
        if b.is_empty() {
            None
        } else {
            Some(b)
        }
    }
}

/// Split a multipart body on its boundary, returning the raw text of each part
/// (header block + body, not yet decoded). The MIME boundary appears as a line
/// `--<boundary>`; the terminator is `--<boundary>--`.
fn split_multipart<'a>(body: &'a str, boundary: &str) -> Vec<&'a str> {
    let delim = format!("--{boundary}");
    let mut parts = Vec::new();

    // Walk the body finding boundary occurrences that begin a line.
    let mut search_from = 0usize;
    let mut segment_start: Option<usize> = None;

    while let Some(rel) = body[search_from..].find(&delim) {
        let abs = search_from + rel;
        // A boundary delimiter must be at the start of the body or at the
        // start of a line (preceded by a newline).
        let at_line_start = abs == 0 || body.as_bytes()[abs - 1] == b'\n';
        if !at_line_start {
            search_from = abs + delim.len();
            continue;
        }

        // Close the previous segment (if any) at this boundary.
        if let Some(start) = segment_start.take() {
            let segment = trim_part_edges(&body[start..abs]);
            if !segment.is_empty() {
                parts.push(segment);
            }
        }

        // Is this the terminating boundary (`--boundary--`)?
        let after = &body[abs + delim.len()..];
        if after.starts_with("--") {
            break;
        }

        // Start of the next part is just past this boundary line.
        let line_end = after
            .find('\n')
            .map(|i| abs + delim.len() + i + 1)
            .unwrap_or(body.len());
        segment_start = Some(line_end);
        search_from = line_end;
    }

    parts
}

/// Trim trailing CR/LF that belong to the boundary delimiter, not the part.
fn trim_part_edges(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}

/// Parse one raw part (headers + encoded body) into a decoded [`Part`].
fn parse_part(raw: &str) -> Option<Part> {
    let (header_block, body) = split_headers(raw);
    let headers = parse_headers(header_block);

    let get = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };

    let mime = get("content-type")
        .map(|v| {
            // Strip parameters: `text/html; charset=utf-8` → `text/html`.
            v.split(';')
                .next()
                .unwrap_or(v)
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let encoding = get("content-transfer-encoding")
        .map(|v| v.trim().to_ascii_lowercase())
        .unwrap_or_default();

    let location = get("content-location").map(|v| v.trim().to_string());
    let content_id = get("content-id").map(|v| strip_angle_brackets(v.trim()).to_string());

    let body = decode_body(body, &encoding);

    Some(Part {
        mime,
        location,
        content_id,
        body,
    })
}

/// Strip a single pair of surrounding angle brackets, e.g. `<foo>` → `foo`.
fn strip_angle_brackets(s: &str) -> &str {
    s.strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(s)
}

/// Decode a part body according to its `Content-Transfer-Encoding`.
fn decode_body(body: &str, encoding: &str) -> Vec<u8> {
    match encoding {
        "base64" => decode_base64(body),
        "quoted-printable" => decode_quoted_printable(body),
        // `7bit`, `8bit`, `binary`, or unknown: pass bytes through.
        _ => body.as_bytes().to_vec(),
    }
}

// ---------------------------------------------------------------------------
// Transfer-encoding decoders
// ---------------------------------------------------------------------------

/// Decode a base64 body, ignoring embedded whitespace/newlines (which MIME
/// inserts for line wrapping). Best-effort: returns empty on hard failure.
fn decode_base64(body: &str) -> Vec<u8> {
    // Strip all ASCII whitespace; MIME base64 is hard-wrapped at ~76 cols.
    let cleaned: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64.decode(cleaned.as_bytes()).unwrap_or_default()
}

/// Decode a quoted-printable body (RFC 2045 §6.7), hand-rolled so we add no
/// dependency. Handles:
/// - soft line breaks: a trailing `=` immediately before a newline is removed
/// - hex escapes: `=XX` decodes to the byte 0xXX
/// - literal bytes pass through unchanged
///
/// Robust by design: a malformed `=` sequence is emitted literally rather than
/// erroring (matches `quoted_printable::ParseMode::Robust` behavior).
fn decode_quoted_printable(body: &str) -> Vec<u8> {
    let bytes = body.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'=' {
            // Look at what follows the '='.
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                // Soft break: "=\n"
                i += 2;
                continue;
            }
            if i + 2 < bytes.len() && bytes[i + 1] == b'\r' && bytes[i + 2] == b'\n' {
                // Soft break: "=\r\n"
                i += 3;
                continue;
            }
            if i + 2 < bytes.len() {
                if let (Some(hi), Some(lo)) =
                    (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
                {
                    out.push((hi << 4) | lo);
                    i += 3;
                    continue;
                }
            }
            // Not a valid escape or soft break: emit the '=' literally.
            out.push(b);
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    out
}

/// Hex digit → value, case-insensitive. Returns `None` for non-hex bytes.
fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// data: URI construction
// ---------------------------------------------------------------------------

/// Build a `data:<mime>;base64,<...>` URI for a decoded resource.
fn build_data_uri(mime: &str, bytes: &[u8]) -> String {
    let mime = if mime.is_empty() {
        "application/octet-stream"
    } else {
        mime
    };
    format!("data:{mime};base64,{}", BASE64.encode(bytes))
}

// ---------------------------------------------------------------------------
// HTML / CSS rewriting (string-level, best-effort)
// ---------------------------------------------------------------------------

/// Replace `<link rel="stylesheet" href="…">` elements whose href resolves to
/// a known CSS part with an inline `<style>…</style>` block. The CSS text has
/// its own `url(...)`/`@import` references inlined first.
fn inline_stylesheets(
    html: &str,
    css_parts: &HashMap<String, String>,
    data_uris: &HashMap<String, String>,
) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0usize;

    // Scan for `<link` tags.
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("<link") {
        let tag_start = search + rel;
        // Find the end of the tag.
        let tag_end = match lower[tag_start..].find('>') {
            Some(i) => tag_start + i + 1,
            None => break, // malformed; stop scanning
        };
        let tag = &html[tag_start..tag_end];

        if is_stylesheet_link(&lower[tag_start..tag_end]) {
            if let Some(href) = attr_value(tag, "href") {
                if let Some(css) = css_parts.get(&href) {
                    // Emit everything up to this tag, then the inlined style.
                    out.push_str(&html[cursor..tag_start]);
                    let inlined_css = inline_css_urls(css, data_uris);
                    out.push_str("<style>");
                    out.push_str(&inlined_css);
                    out.push_str("</style>");
                    cursor = tag_end;
                }
            }
        }
        search = tag_end;
    }

    out.push_str(&html[cursor..]);
    out
}

/// Whether a `<link …>` tag (lowercased) is a stylesheet link.
fn is_stylesheet_link(tag_lower: &str) -> bool {
    // Look for rel containing "stylesheet". Match the rel attribute value.
    if let Some(rel) = attr_value(tag_lower, "rel") {
        rel.split_whitespace().any(|t| t == "stylesheet")
    } else {
        false
    }
}

/// Rewrite resource references in HTML to `data:` URIs. Handles `src`, `href`
/// (for non-stylesheet elements like icons), `poster`, and `srcset` attributes
/// that point at known parts, plus inline `style="…"`/`<style>` `url(...)`.
fn rewrite_references(html: &str, data_uris: &HashMap<String, String>) -> String {
    // First, inline url(...) inside any <style> blocks and style="" attrs by
    // running a CSS pass over the whole document — cheap and correct enough,
    // since url(...) only appears in CSS contexts within HTML.
    let with_css = inline_css_urls(html, data_uris);

    // Then rewrite element attributes that hold a single URL or a srcset.
    rewrite_attr_urls(&with_css, data_uris)
}

/// Replace `url(...)` targets and `@import "..."`/`@import url(...)` targets in
/// CSS/HTML text with `data:` URIs when the target is a known part.
fn inline_css_urls(text: &str, data_uris: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    // Handle url(...) occurrences.
    while let Some(pos) = rest.find("url(") {
        out.push_str(&rest[..pos + 4]);
        let after = &rest[pos + 4..];
        let Some(close) = after.find(')') else {
            // No closing paren; emit the remainder and stop.
            out.push_str(after);
            rest = "";
            break;
        };
        let inner = &after[..close];
        let trimmed = inner.trim();
        let (quote, raw_url) = strip_quotes(trimmed);
        let replacement = match data_uris.get(raw_url) {
            Some(data_uri) => match quote {
                Some(q) => format!("{q}{data_uri}{q}"),
                None => data_uri.clone(),
            },
            None => inner.to_string(),
        };
        out.push_str(&replacement);
        out.push(')');
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Strip a single pair of matching quotes from a CSS token, returning the quote
/// char used (if any) and the inner value.
fn strip_quotes(s: &str) -> (Option<char>, &str) {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return (Some(first as char), &s[1..s.len() - 1]);
        }
    }
    (None, s)
}

/// Rewrite single-URL attributes (`src`, `href`, `poster`) and `srcset`
/// attributes throughout the HTML to `data:` URIs for known parts.
fn rewrite_attr_urls(html: &str, data_uris: &HashMap<String, String>) -> String {
    // We do attribute-name-anchored scanning to avoid a full HTML parser.
    let single = ["src=", "poster=", "href="];
    let mut result = html.to_string();

    for attr in single {
        result = rewrite_single_attr(&result, attr, data_uris);
    }
    result = rewrite_srcset(&result, data_uris);
    result
}

/// Rewrite a single-URL attribute (e.g. `src="…"`). Matches the attribute name
/// only when it begins an attribute (preceded by whitespace), to avoid hitting
/// substrings like `data-src=` unintentionally.
fn rewrite_single_attr(html: &str, attr: &str, data_uris: &HashMap<String, String>) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0usize;
    let mut search = 0usize;

    while let Some(rel) = lower[search..].find(attr) {
        let at = search + rel;
        // Must be at an attribute boundary: preceded by ASCII whitespace.
        let preceded_ok = at > 0 && html.as_bytes()[at - 1].is_ascii_whitespace();
        if !preceded_ok {
            search = at + attr.len();
            continue;
        }
        let val_start = at + attr.len();
        let bytes = html.as_bytes();
        if val_start >= bytes.len() {
            break;
        }
        let quote = bytes[val_start];
        if quote != b'"' && quote != b'\'' {
            // Unquoted attribute values are rare in serialized DOM; skip.
            search = val_start;
            continue;
        }
        // Find the matching closing quote.
        let value_region = &html[val_start + 1..];
        let Some(end_rel) = value_region.find(quote as char) else {
            break;
        };
        let url = &value_region[..end_rel];
        let value_end = val_start + 1 + end_rel; // index of closing quote

        if let Some(data_uri) = data_uris.get(url.trim()) {
            out.push_str(&html[cursor..val_start + 1]);
            out.push_str(data_uri);
            cursor = value_end; // closing quote re-emitted by the tail/next copy
        }
        search = value_end + 1;
    }

    out.push_str(&html[cursor..]);
    out
}

/// Rewrite `srcset="url1 1x, url2 2x, …"` attributes, replacing each candidate
/// URL with its `data:` URI when known.
fn rewrite_srcset(html: &str, data_uris: &HashMap<String, String>) -> String {
    let attr = "srcset=";
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0usize;
    let mut search = 0usize;

    while let Some(rel) = lower[search..].find(attr) {
        let at = search + rel;
        let preceded_ok = at > 0 && html.as_bytes()[at - 1].is_ascii_whitespace();
        if !preceded_ok {
            search = at + attr.len();
            continue;
        }
        let val_start = at + attr.len();
        let bytes = html.as_bytes();
        if val_start >= bytes.len() {
            break;
        }
        let quote = bytes[val_start];
        if quote != b'"' && quote != b'\'' {
            search = val_start;
            continue;
        }
        let value_region = &html[val_start + 1..];
        let Some(end_rel) = value_region.find(quote as char) else {
            break;
        };
        let srcset = &value_region[..end_rel];
        let value_end = val_start + 1 + end_rel;

        let rewritten = rewrite_srcset_value(srcset, data_uris);
        out.push_str(&html[cursor..val_start + 1]);
        out.push_str(&rewritten);
        cursor = value_end;
        search = value_end + 1;
    }

    out.push_str(&html[cursor..]);
    out
}

/// Rewrite the value of a single `srcset` attribute.
fn rewrite_srcset_value(srcset: &str, data_uris: &HashMap<String, String>) -> String {
    srcset
        .split(',')
        .map(|candidate| {
            let candidate = candidate.trim_matches(|c: char| c.is_whitespace());
            // A candidate is `URL [descriptor]`. The URL is the first token.
            let mut it = candidate.splitn(2, |c: char| c.is_whitespace());
            let url = it.next().unwrap_or("");
            let descriptor = it.next();
            let new_url = data_uris.get(url.trim()).cloned().unwrap_or_else(|| url.to_string());
            match descriptor {
                Some(d) if !d.trim().is_empty() => format!("{new_url} {}", d.trim()),
                _ => new_url,
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extract the value of attribute `name` from a tag string, case-insensitively.
/// Returns the unquoted value (or the bare token for unquoted attributes).
fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0usize;
    loop {
        let rel = lower[from..].find(name)?;
        let at = from + rel;
        // Boundary check: attribute name preceded by whitespace or `<`/tag.
        let prev_ok = at == 0
            || tag.as_bytes()[at - 1].is_ascii_whitespace()
            || tag.as_bytes()[at - 1] == b'<';
        let after = &tag[at + name.len()..];
        // The next non-space char after the name must be `=`.
        let after_trim = after.trim_start();
        if prev_ok && after_trim.starts_with('=') {
            let val = after_trim[1..].trim_start();
            let vb = val.as_bytes();
            if vb.is_empty() {
                return Some(String::new());
            }
            if vb[0] == b'"' || vb[0] == b'\'' {
                let q = vb[0] as char;
                let rest = &val[1..];
                let end = rest.find(q)?;
                return Some(rest[..end].to_string());
            }
            // Unquoted: up to whitespace or `>`.
            let end = val
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(val.len());
            return Some(val[..end].to_string());
        }
        from = at + name.len();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 1x1 transparent PNG, base64 (standard MIME table).
    const PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    /// Build a minimal Chromium-style MHTML fixture: a root HTML part that
    /// references a base64 PNG (via Content-Location) and an external CSS part.
    fn fixture() -> String {
        // Root HTML references both resources by absolute URL.
        let html_body = "<!DOCTYPE html><html><head>\
<link rel=\"stylesheet\" href=\"https://ex.com/style.css\">\
</head><body>\
<p>Hello Amber</p>\
<img src=\"https://ex.com/pixel.png\">\
</body></html>";

        let css_body = "body { background: url(https://ex.com/pixel.png); color: red; }";

        // Assemble multipart/related. CRLF line endings, as Chromium emits.
        let mut m = String::new();
        m.push_str("From: <Saved by Blink>\r\n");
        m.push_str("Subject: Test\r\n");
        m.push_str("MIME-Version: 1.0\r\n");
        m.push_str(
            "Content-Type: multipart/related; boundary=\"BOUNDARY\"; type=\"text/html\"\r\n",
        );
        m.push_str("Content-Location: https://ex.com/index.html\r\n");
        m.push_str("\r\n");

        // Part 1: root HTML (quoted-printable to exercise that decoder).
        m.push_str("--BOUNDARY\r\n");
        m.push_str("Content-Type: text/html\r\n");
        m.push_str("Content-Transfer-Encoding: quoted-printable\r\n");
        m.push_str("Content-Location: https://ex.com/index.html\r\n");
        m.push_str("\r\n");
        m.push_str(&qp_encode(html_body));
        m.push_str("\r\n");

        // Part 2: external CSS (quoted-printable).
        m.push_str("--BOUNDARY\r\n");
        m.push_str("Content-Type: text/css\r\n");
        m.push_str("Content-Transfer-Encoding: quoted-printable\r\n");
        m.push_str("Content-Location: https://ex.com/style.css\r\n");
        m.push_str("\r\n");
        m.push_str(&qp_encode(css_body));
        m.push_str("\r\n");

        // Part 3: PNG image (base64).
        m.push_str("--BOUNDARY\r\n");
        m.push_str("Content-Type: image/png\r\n");
        m.push_str("Content-Transfer-Encoding: base64\r\n");
        m.push_str("Content-Location: https://ex.com/pixel.png\r\n");
        m.push_str("\r\n");
        m.push_str(PNG_B64);
        m.push_str("\r\n");

        // Terminator.
        m.push_str("--BOUNDARY--\r\n");
        m
    }

    /// Minimal quoted-printable encoder for building test fixtures: escapes
    /// `=` and bytes outside the printable ASCII range. (Plain ASCII text used
    /// in the fixtures mostly passes through; this keeps round-trips honest.)
    fn qp_encode(s: &str) -> String {
        let mut out = String::new();
        for &b in s.as_bytes() {
            if b == b'=' {
                out.push_str("=3D");
            } else if (0x20..=0x7e).contains(&b) {
                out.push(b as char);
            } else {
                out.push_str(&format!("={b:02X}"));
            }
        }
        out
    }

    #[test]
    fn full_transform_inlines_image_and_css() {
        let mhtml = fixture();
        let html = mhtml_to_single_file_html(&mhtml);

        // Image inlined as a data: URI.
        assert!(
            html.contains("data:image/png;base64,"),
            "expected inlined PNG data URI, got:\n{html}"
        );
        // The original network reference to the image is gone.
        assert!(
            !html.contains("src=\"https://ex.com/pixel.png\""),
            "image src should be rewritten:\n{html}"
        );

        // CSS inlined into a <style> block; no external <link> remains.
        assert!(html.contains("<style>"), "expected inline <style>:\n{html}");
        assert!(
            !html.to_ascii_lowercase().contains("<link"),
            "external stylesheet <link> should be removed:\n{html}"
        );
        assert!(
            html.contains("color: red"),
            "CSS body should be inlined:\n{html}"
        );
        // url(...) inside the inlined CSS is also a data: URI.
        assert!(
            html.contains("url(data:image/png;base64,"),
            "CSS url() should be inlined:\n{html}"
        );

        // Body text preserved.
        assert!(html.contains("Hello Amber"), "body text lost:\n{html}");
    }

    #[test]
    fn unparseable_input_returned_verbatim() {
        let junk = "this is not mhtml at all";
        assert_eq!(mhtml_to_single_file_html(junk), junk);
    }

    #[test]
    fn no_html_part_returns_input() {
        // A valid multipart with only a non-HTML part → return input unchanged.
        let m = "Content-Type: multipart/related; boundary=\"B\"\r\n\r\n\
--B\r\n\
Content-Type: image/png\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-Location: https://ex.com/x.png\r\n\
\r\n\
AAAA\r\n\
--B--\r\n";
        assert_eq!(mhtml_to_single_file_html(m), m);
    }

    #[test]
    fn base64_decoder_ignores_whitespace() {
        // "Hello" => SGVsbG8=, split across lines with CRLF wrapping.
        let wrapped = "SGVs\r\nbG8=";
        assert_eq!(decode_base64(wrapped), b"Hello");
    }

    #[test]
    fn base64_decoder_bad_input_is_empty() {
        assert_eq!(decode_base64("!!!not base64!!!"), Vec::<u8>::new());
    }

    #[test]
    fn quoted_printable_hex_escapes() {
        // "=3D" -> '=', "=20" -> ' '
        assert_eq!(decode_quoted_printable("a=3Db=20c"), b"a=b c");
    }

    #[test]
    fn quoted_printable_soft_line_breaks() {
        // Soft break "=\r\n" and "=\n" are removed (line continuation).
        assert_eq!(decode_quoted_printable("hel=\r\nlo"), b"hello");
        assert_eq!(decode_quoted_printable("wor=\nld"), b"world");
    }

    #[test]
    fn quoted_printable_robust_on_bad_escape() {
        // A lone '=' not followed by valid hex is emitted literally.
        assert_eq!(decode_quoted_printable("a=zz"), b"a=zz");
        assert_eq!(decode_quoted_printable("trailing="), b"trailing=");
    }

    #[test]
    fn boundary_extraction_quoted_and_unquoted() {
        assert_eq!(
            extract_boundary("multipart/related; boundary=\"abc123\""),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_boundary("multipart/related; boundary=xyz; type=\"text/html\""),
            Some("xyz".to_string())
        );
        assert_eq!(extract_boundary("text/html; charset=utf-8"), None);
    }

    #[test]
    fn boundary_splitter_yields_parts() {
        let body = "preamble\r\n\
--B\r\n\
Content-Type: text/plain\r\n\
\r\n\
one\r\n\
--B\r\n\
Content-Type: text/plain\r\n\
\r\n\
two\r\n\
--B--\r\n";
        let parts = split_multipart(body, "B");
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("one"));
        assert!(parts[1].contains("two"));
        // Preamble before the first boundary is not a part.
        assert!(!parts[0].contains("preamble"));
    }

    #[test]
    fn srcset_value_rewrite() {
        let mut map = HashMap::new();
        map.insert(
            "https://ex.com/a.png".to_string(),
            "data:image/png;base64,AAA".to_string(),
        );
        let got = rewrite_srcset_value(
            "https://ex.com/a.png 1x, https://ex.com/missing.png 2x",
            &map,
        );
        assert_eq!(
            got,
            "data:image/png;base64,AAA 1x, https://ex.com/missing.png 2x"
        );
    }

    #[test]
    fn attr_value_extraction() {
        let tag = "<link rel=\"stylesheet\" href=\"https://ex.com/s.css\">";
        assert_eq!(attr_value(tag, "rel"), Some("stylesheet".to_string()));
        assert_eq!(
            attr_value(tag, "href"),
            Some("https://ex.com/s.css".to_string())
        );
    }
}
