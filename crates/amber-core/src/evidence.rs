//! Tamper-evident evidence manifest. See `Plans.md` (task 9.1).
//!
//! An [`EvidenceManifest`] records, for one capture, the SHA-256 and byte length
//! of each requested output alongside the source URL and capture instant. Any
//! change to an output changes its digest, and [`EvidenceManifest::digest`]
//! covers the whole manifest — so the bundle is tamper-evident. A cryptographic
//! signature over [`digest`](EvidenceManifest::digest) (e.g. ed25519) is the
//! remaining 9.1 work; the manifest is the value such a signature would cover.

use serde_json::json;

use crate::cache::content_hash;

/// One captured output's integrity record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceEntry {
    /// The output format's extension (e.g. `md`, `warc`).
    pub format: String,
    /// SHA-256 of the rendered bytes, lowercase hex.
    pub sha256: String,
    /// Length of the rendered output in bytes.
    pub bytes: usize,
}

/// A tamper-evident record of one capture's outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceManifest {
    /// The capture's source URL.
    pub url: String,
    /// Capture instant (ISO 8601), fixed at capture time (reproducible, 8.5).
    pub captured_at: String,
    /// One entry per requested output, in request order.
    pub outputs: Vec<EvidenceEntry>,
}

impl EvidenceManifest {
    /// Canonical JSON encoding. `serde_json` orders object keys deterministically,
    /// so the encoding (and thus [`digest`](Self::digest)) is stable.
    pub fn to_json(&self) -> String {
        let outputs: Vec<_> = self
            .outputs
            .iter()
            .map(|e| json!({ "format": e.format, "sha256": e.sha256, "bytes": e.bytes }))
            .collect();
        json!({
            "url": self.url,
            "captured_at": self.captured_at,
            "outputs": outputs,
        })
        .to_string()
    }

    /// SHA-256 of the canonical JSON — the tamper-evident root. Changing any
    /// output hash, the URL, or the timestamp changes this digest. This is the
    /// value a cryptographic signature would cover (remaining 9.1 work).
    pub fn digest(&self) -> String {
        content_hash(self.to_json().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> EvidenceManifest {
        EvidenceManifest {
            url: "https://ex.com/".to_string(),
            captured_at: "2026-01-02T03:04:05Z".to_string(),
            outputs: vec![EvidenceEntry {
                format: "md".to_string(),
                sha256: content_hash(b"# Hello"),
                bytes: 7,
            }],
        }
    }

    #[test]
    fn json_carries_fields_and_output_hash() {
        let json = manifest().to_json();
        assert!(json.contains("\"url\""));
        assert!(json.contains("\"captured_at\""));
        assert!(json.contains("\"outputs\""));
        assert!(json.contains(&content_hash(b"# Hello")));
        // Encoding is stable across calls (basis for a stable digest).
        assert_eq!(json, manifest().to_json());
    }

    #[test]
    fn digest_is_deterministic() {
        assert_eq!(manifest().digest(), manifest().digest());
    }

    #[test]
    fn digest_changes_when_any_output_is_tampered() {
        let original = manifest();
        let mut tampered = original.clone();
        tampered.outputs[0].sha256 = content_hash(b"# Tampered");
        assert_ne!(original.digest(), tampered.digest());
    }

    #[test]
    fn digest_changes_when_url_or_timestamp_changes() {
        let original = manifest();
        let mut other = original.clone();
        other.captured_at = "2099-12-31T23:59:59Z".to_string();
        assert_ne!(original.digest(), other.digest());
    }
}
