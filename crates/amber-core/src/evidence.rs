//! Tamper-evident evidence manifest. See `Plans.md` (task 9.1).
//!
//! An [`EvidenceManifest`] records, for one capture, the SHA-256 and byte length
//! of each requested output alongside the source URL and capture instant. Any
//! change to an output changes its digest, and [`EvidenceManifest::digest`]
//! covers the whole manifest — so the bundle is tamper-evident.
//! [`EvidenceManifest::sign`] adds an ed25519 signature over the canonical
//! manifest, yielding a self-contained, verifiable [`SignedEvidence`] bundle.

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
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
    /// output hash, the URL, or the timestamp changes this digest.
    pub fn digest(&self) -> String {
        content_hash(self.to_json().as_bytes())
    }

    /// Sign the manifest's canonical JSON with an ed25519 secret key (32-byte
    /// seed), producing a self-contained, verifiable [`SignedEvidence`] bundle
    /// (manifest + public key + signature). Signing is deterministic (RFC 8032).
    pub fn sign(&self, secret_seed: &[u8; 32]) -> SignedEvidence {
        let key = SigningKey::from_bytes(secret_seed);
        let signature = key.sign(self.to_json().as_bytes());
        SignedEvidence {
            manifest: self.clone(),
            algorithm: "ed25519".to_string(),
            public_key: to_hex(key.verifying_key().as_bytes()),
            signature: to_hex(&signature.to_bytes()),
        }
    }
}

/// An [`EvidenceManifest`] plus an ed25519 signature over its canonical JSON
/// and the signer's public key — a self-contained, verifiable evidence bundle
/// (Plans.md 9.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedEvidence {
    /// The signed manifest.
    pub manifest: EvidenceManifest,
    /// Signature algorithm identifier (`"ed25519"`).
    pub algorithm: String,
    /// The signer's ed25519 public key, lowercase hex (32 bytes).
    pub public_key: String,
    /// The signature over [`EvidenceManifest::to_json`], lowercase hex (64 bytes).
    pub signature: String,
}

impl SignedEvidence {
    /// Verify the signature over the manifest against the embedded public key.
    /// Returns `true` only if the manifest is untampered and was signed by the
    /// holder of the matching secret key. *Trusting the public key itself*
    /// (i.e. that it belongs to whom you expect) is the caller's responsibility.
    pub fn verify(&self) -> bool {
        let (Some(pk), Some(sig)) = (
            from_hex::<32>(&self.public_key),
            from_hex::<64>(&self.signature),
        ) else {
            return false;
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(&pk) else {
            return false;
        };
        let signature = ed25519_dalek::Signature::from_bytes(&sig);
        verifying_key
            .verify(self.manifest.to_json().as_bytes(), &signature)
            .is_ok()
    }

    /// Canonical JSON of the signed bundle.
    pub fn to_json(&self) -> String {
        json!({
            "manifest": serde_json::from_str::<serde_json::Value>(&self.manifest.to_json())
                .unwrap_or(serde_json::Value::Null),
            "algorithm": self.algorithm,
            "public_key": self.public_key,
            "signature": self.signature,
        })
        .to_string()
    }
}

/// Lowercase-hex encode bytes.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Decode exactly `N` bytes from lowercase/uppercase hex; `None` on bad length
/// or non-hex input.
fn from_hex<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
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

    #[test]
    fn hex_round_trips() {
        assert_eq!(to_hex(&[0x00, 0xab, 0xff]), "00abff");
        assert_eq!(from_hex::<3>("00abff"), Some([0x00, 0xab, 0xff]));
        assert_eq!(from_hex::<3>("00ab"), None); // wrong length
        assert_eq!(from_hex::<2>("zzzz"), None); // not hex
    }

    #[test]
    fn sign_then_verify_succeeds() {
        let signed = manifest().sign(&[7u8; 32]);
        assert_eq!(signed.algorithm, "ed25519");
        assert_eq!(signed.public_key.len(), 64); // 32 bytes, hex
        assert_eq!(signed.signature.len(), 128); // 64 bytes, hex
        assert!(signed.verify(), "a freshly signed bundle must verify");
    }

    #[test]
    fn verify_fails_when_manifest_is_tampered() {
        let mut signed = manifest().sign(&[7u8; 32]);
        signed.manifest.outputs[0].sha256 = content_hash(b"# Tampered");
        assert!(
            !signed.verify(),
            "tampering with the manifest must fail verify"
        );
    }

    #[test]
    fn verify_fails_on_corrupt_signature_or_key() {
        let good = manifest().sign(&[7u8; 32]);

        let mut bad_sig = good.clone();
        let flipped = if &good.signature[0..1] == "0" {
            "f"
        } else {
            "0"
        };
        bad_sig.signature.replace_range(0..1, flipped);
        assert!(!bad_sig.verify());

        let mut bad_key = good.clone();
        bad_key.public_key = "abcd".to_string(); // wrong length / not a key
        assert!(!bad_key.verify());
    }

    #[test]
    fn signed_json_carries_all_fields() {
        let signed = manifest().sign(&[7u8; 32]);
        let json = signed.to_json();
        assert!(json.contains("ed25519"));
        assert!(json.contains(&signed.public_key));
        assert!(json.contains(&signed.signature));
        assert!(json.contains("\"manifest\""));
    }
}
