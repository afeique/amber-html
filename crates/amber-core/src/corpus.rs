//! Provenance-tagged corpus builder. See `Plans.md` (task 9.2).
//!
//! Bulk captures produce a dataset where every extracted field carries its
//! provenance — the DOM-node CSS path and source URL it came from (task 4.4) —
//! emitted as JSONL (one record per line, the dataset-export shape of 7.6). A
//! [`ProvenanceRecord`] is one capture's structured extraction plus its anchors;
//! [`to_jsonl`] serializes a batch of them.

use serde_json::{json, Value};

use crate::provenance::FieldProvenance;

/// One capture's provenance-tagged extraction: the structured value plus, per
/// field, where on the page it came from.
#[derive(Debug, Clone)]
pub struct ProvenanceRecord {
    /// Source URL of the capture.
    pub url: String,
    /// Capture instant (ISO 8601).
    pub captured_at: String,
    /// The extracted structured value.
    pub value: Value,
    /// One anchor per scalar field of `value` (see [`FieldProvenance`]).
    pub anchors: Vec<FieldProvenance>,
}

impl ProvenanceRecord {
    /// JSON object: `{ url, captured_at, value, provenance: [{path, value,
    /// css_path, source_url}] }`. `css_path`/`source_url` are `null` for fields
    /// not located on the page.
    pub fn to_json(&self) -> Value {
        let provenance: Vec<Value> = self
            .anchors
            .iter()
            .map(|field| {
                json!({
                    "path": field.path,
                    "value": field.value,
                    "css_path": field.anchor.as_ref().map(|a| a.css_path.clone()),
                    "source_url": field.anchor.as_ref().map(|a| a.url.clone()),
                })
            })
            .collect();
        json!({
            "url": self.url,
            "captured_at": self.captured_at,
            "value": self.value,
            "provenance": provenance,
        })
    }
}

/// Serialize a batch of records to JSONL — one record's JSON per line.
pub fn to_jsonl(records: &[ProvenanceRecord]) -> String {
    records
        .iter()
        .map(|r| r.to_json().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::Provenance;

    fn record(url: &str) -> ProvenanceRecord {
        ProvenanceRecord {
            url: url.to_string(),
            captured_at: "2026-01-02T03:04:05Z".to_string(),
            value: json!({ "price": "42 USD" }),
            anchors: vec![FieldProvenance {
                path: "/price".to_string(),
                value: "42 USD".to_string(),
                anchor: Some(Provenance {
                    css_path: "div#main > span.amount".to_string(),
                    text: "42 USD".to_string(),
                    url: url.to_string(),
                }),
            }],
        }
    }

    #[test]
    fn record_json_tags_fields_with_provenance() {
        let json = record("https://ex.com/").to_json();
        assert_eq!(json["url"], "https://ex.com/");
        assert_eq!(json["captured_at"], "2026-01-02T03:04:05Z");
        assert_eq!(json["value"]["price"], "42 USD");
        let prov = &json["provenance"][0];
        assert_eq!(prov["path"], "/price");
        assert_eq!(prov["css_path"], "div#main > span.amount");
        assert_eq!(prov["source_url"], "https://ex.com/");
    }

    #[test]
    fn unanchored_field_has_null_css_path() {
        let mut rec = record("https://ex.com/");
        rec.anchors[0].anchor = None;
        let json = rec.to_json();
        assert!(json["provenance"][0]["css_path"].is_null());
    }

    #[test]
    fn jsonl_has_one_valid_json_object_per_line() {
        let jsonl = to_jsonl(&[record("https://a.com/"), record("https://b.com/")]);
        let lines: Vec<&str> = jsonl.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            // Each line parses as a standalone JSON object (valid JSONL).
            let v: Value = serde_json::from_str(line).expect("each line is JSON");
            assert!(v["url"].is_string());
            assert!(v["provenance"].is_array());
        }
    }
}
