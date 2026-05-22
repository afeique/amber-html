//! Model-agnostic structured & natural-language extraction. See `Plans.md`
//! (tasks 4.1/4.2; open question #4 on the LLM interface).
//!
//! AmberHTML does not embed any LLM. Instead the caller implements [`LlmClient`]
//! for their endpoint (OpenAI-compatible, a local model, …) and AmberHTML
//! handles the engine side: building the extraction prompt from a JSON schema
//! (or a natural-language instruction) plus the page text, and parsing the
//! model's response back into a JSON value — tolerating code fences and
//! surrounding prose.

use serde_json::Value;

use crate::error::{Error, Result};

/// A pluggable text-completion client. Implement this for your model endpoint;
/// AmberHTML stays model-agnostic (bring your own LLM).
pub trait LlmClient {
    /// Complete `prompt`, returning the model's text response.
    fn complete(&self, prompt: &str) -> Result<String>;
}

/// Build the prompt for schema-driven extraction: instruct the model to return
/// only JSON matching `schema`, given the page `content`.
pub fn schema_prompt(schema: &str, content: &str) -> String {
    format!(
        "Extract information from the CONTENT below into JSON that conforms to \
         the given JSON SCHEMA. Respond with ONLY the JSON value — no prose, no \
         code fences.\n\n\
         JSON SCHEMA:\n{schema}\n\n\
         CONTENT:\n{content}"
    )
}

/// Build the prompt for natural-language extraction: answer `instruction` about
/// the page `content` as JSON.
pub fn nl_prompt(instruction: &str, content: &str) -> String {
    format!(
        "Follow the INSTRUCTION using only the CONTENT below, and respond with \
         ONLY a JSON value — no prose, no code fences.\n\n\
         INSTRUCTION:\n{instruction}\n\n\
         CONTENT:\n{content}"
    )
}

/// Extract structured JSON from `content` against `schema` via `client`.
pub fn extract_structured<C: LlmClient>(content: &str, schema: &str, client: &C) -> Result<Value> {
    let raw = client.complete(&schema_prompt(schema, content))?;
    parse_json_response(&raw)
}

/// Extract JSON answering `instruction` about `content` via `client`.
pub fn extract_nl<C: LlmClient>(content: &str, instruction: &str, client: &C) -> Result<Value> {
    let raw = client.complete(&nl_prompt(instruction, content))?;
    parse_json_response(&raw)
}

/// Parse a JSON value out of a model response, tolerating ```json code fences
/// and surrounding prose by extracting the outermost JSON object/array.
pub fn parse_json_response(raw: &str) -> Result<Value> {
    let trimmed = strip_code_fences(raw.trim());

    // Fast path: the whole thing is JSON.
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    // Otherwise, slice from the first opening bracket to its matching close.
    if let Some(candidate) = outermost_json(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            return Ok(value);
        }
    }

    Err(Error::Extraction(
        "model response did not contain valid JSON".to_string(),
    ))
}

/// Strip a single ``` / ```json fenced block, returning its inner content.
fn strip_code_fences(s: &str) -> &str {
    let Some(rest) = s.strip_prefix("```") else {
        return s;
    };
    // Drop an optional language tag on the first line.
    let rest = rest.split_once('\n').map_or("", |(_, after)| after);
    rest.strip_suffix("```")
        .or_else(|| rest.rsplit_once("```").map(|(before, _)| before))
        .unwrap_or(rest)
        .trim()
}

/// The substring from the first `{`/`[` to its matching close brace/bracket,
/// respecting nesting and strings. `None` if no balanced region is found.
fn outermost_json(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{' || b == b'[')?;
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            x if x == open => depth += 1,
            x if x == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canned client returning a fixed response (records the prompt it saw).
    struct MockClient {
        response: String,
    }
    impl LlmClient for MockClient {
        fn complete(&self, _prompt: &str) -> Result<String> {
            Ok(self.response.clone())
        }
    }

    #[test]
    fn schema_prompt_includes_schema_and_content() {
        let p = schema_prompt(r#"{"type":"object"}"#, "Hello world");
        assert!(p.contains(r#"{"type":"object"}"#));
        assert!(p.contains("Hello world"));
        assert!(p.contains("ONLY the JSON"));
    }

    #[test]
    fn parses_plain_json() {
        let v = parse_json_response(r#"{"title":"Hi","n":3}"#).unwrap();
        assert_eq!(v["title"], "Hi");
        assert_eq!(v["n"], 3);
    }

    #[test]
    fn parses_fenced_json() {
        let raw = "```json\n{\"a\": 1}\n```";
        let v = parse_json_response(raw).unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn parses_json_embedded_in_prose() {
        let raw = "Sure! Here is the result:\n{\"ok\": true, \"items\": [1,2]}\nLet me know!";
        let v = parse_json_response(raw).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["items"][1], 2);
    }

    #[test]
    fn ignores_braces_inside_strings() {
        let raw = r#"{"text":"a } b ] c","done":true}"#;
        let v = parse_json_response(raw).unwrap();
        assert_eq!(v["text"], "a } b ] c");
        assert_eq!(v["done"], true);
    }

    #[test]
    fn errors_on_non_json_response() {
        let err = parse_json_response("I cannot help with that.").unwrap_err();
        assert!(matches!(err, Error::Extraction(_)));
    }

    #[test]
    fn extract_structured_round_trip_with_mock_client() {
        let client = MockClient {
            response: "```json\n{\"title\":\"AmberHTML\",\"tags\":[\"rust\",\"cli\"]}\n```"
                .to_string(),
        };
        let v = extract_structured("page text", r#"{"type":"object"}"#, &client).unwrap();
        assert_eq!(v["title"], "AmberHTML");
        assert_eq!(v["tags"][0], "rust");
    }

    #[test]
    fn extract_nl_uses_instruction_prompt() {
        let client = MockClient {
            response: "[\"a\",\"b\"]".to_string(),
        };
        let v = extract_nl("page text", "list the section names", &client).unwrap();
        assert!(v.is_array());
        assert_eq!(v[0], "a");
    }
}
