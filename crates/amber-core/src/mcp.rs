//! A minimal Model Context Protocol (MCP) server over newline-delimited
//! JSON-RPC 2.0 (the MCP stdio transport). See `Plans.md` (task 2.1).
//!
//! [`handle_request`] is the pure request→response core (`initialize`,
//! `tools/list`, `tools/call`); [`serve`] is the transport loop over any
//! `BufRead`/`Write` (stdin/stdout in production, in-memory buffers in tests).
//! The single tool, `snapshot`, captures a URL via a caller-supplied closure so
//! the protocol layer stays I/O-free and testable.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

/// The MCP protocol version this server implements.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

enum Outcome {
    Result(Value),
    Error(i64, String),
}

/// Handle one JSON-RPC request, returning the response — or `None` for a
/// notification (a message with no `id`). `capture(url, format)` performs the
/// actual page capture for `tools/call`.
pub fn handle_request<F>(req: &Value, capture: &F) -> Option<Value>
where
    F: Fn(&str, &str) -> Result<String, String>,
{
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    // Notifications (no/null id) get no response.
    let id = match req.get("id") {
        Some(id) if !id.is_null() => id.clone(),
        _ => return None,
    };

    let outcome = match method {
        "initialize" => Outcome::Result(initialize_result()),
        "tools/list" => Outcome::Result(tools_list_result()),
        "tools/call" => tools_call(req.get("params"), capture),
        other => Outcome::Error(-32601, format!("method not found: {other}")),
    };

    Some(match outcome {
        Outcome::Result(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Outcome::Error(code, message) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

/// Run the MCP server: read newline-delimited JSON-RPC requests from `reader`,
/// write responses to `writer`. Malformed lines get a `-32700` parse error.
pub fn serve<R, W, F>(reader: R, mut writer: W, capture: F) -> std::io::Result<()>
where
    R: BufRead,
    W: Write,
    F: Fn(&str, &str) -> Result<String, String>,
{
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(req) => {
                if let Some(resp) = handle_request(&req, &capture) {
                    writeln!(writer, "{resp}")?;
                    writer.flush()?;
                }
            }
            Err(_) => {
                let resp = json!({
                    "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32700, "message": "parse error" }
                });
                writeln!(writer, "{resp}")?;
                writer.flush()?;
            }
        }
    }
    Ok(())
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "amber-html", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn tools_list_result() -> Value {
    json!({
        "tools": [{
            "name": "snapshot",
            "description": "Capture a web page and return it as Markdown or readable text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The URL to capture." },
                    "format": {
                        "type": "string",
                        "enum": ["markdown", "readable"],
                        "description": "Output format (default: markdown)."
                    }
                },
                "required": ["url"]
            }
        }]
    })
}

fn tools_call<F>(params: Option<&Value>, capture: &F) -> Outcome
where
    F: Fn(&str, &str) -> Result<String, String>,
{
    let Some(params) = params else {
        return Outcome::Error(-32602, "missing params".to_string());
    };
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if name != "snapshot" {
        return Outcome::Error(-32602, format!("unknown tool: {name}"));
    }
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let Some(url) = args.get("url").and_then(Value::as_str) else {
        return Outcome::Error(-32602, "missing required argument: url".to_string());
    };
    let format = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("markdown");

    // Tool failures are reported as a result with isError:true (MCP convention),
    // not as a JSON-RPC protocol error.
    match capture(url, format) {
        Ok(text) => Outcome::Result(json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false
        })),
        Err(err) => Outcome::Result(json!({
            "content": [{ "type": "text", "text": err }],
            "isError": true
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A capture stub: errors for the URL "bad", echoes otherwise.
    fn stub(url: &str, format: &str) -> Result<String, String> {
        if url == "bad" {
            Err("capture failed".to_string())
        } else {
            Ok(format!("[{format}] {url}"))
        }
    }

    fn req(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[test]
    fn initialize_reports_server_info() {
        let r = handle_request(&req(1, "initialize", json!({})), &stub).unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(r["result"]["serverInfo"]["name"], "amber-html");
    }

    #[test]
    fn tools_list_exposes_snapshot() {
        let r = handle_request(&req(2, "tools/list", json!({})), &stub).unwrap();
        assert_eq!(r["result"]["tools"][0]["name"], "snapshot");
        assert_eq!(r["result"]["tools"][0]["inputSchema"]["required"][0], "url");
    }

    #[test]
    fn tools_call_returns_capture_text() {
        let params = json!({ "name": "snapshot", "arguments": { "url": "https://e.com", "format": "readable" } });
        let r = handle_request(&req(3, "tools/call", params), &stub).unwrap();
        assert_eq!(
            r["result"]["content"][0]["text"],
            "[readable] https://e.com"
        );
        assert_eq!(r["result"]["isError"], false);
    }

    #[test]
    fn tools_call_capture_error_is_iserror_result() {
        let params = json!({ "name": "snapshot", "arguments": { "url": "bad" } });
        let r = handle_request(&req(4, "tools/call", params), &stub).unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(r["result"]["content"][0]["text"], "capture failed");
    }

    #[test]
    fn tools_call_missing_url_is_invalid_params() {
        let params = json!({ "name": "snapshot", "arguments": {} });
        let r = handle_request(&req(5, "tools/call", params), &stub).unwrap();
        assert_eq!(r["error"]["code"], -32602);
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let r = handle_request(&req(6, "no/such", json!({})), &stub).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    #[test]
    fn notification_without_id_has_no_response() {
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle_request(&note, &stub).is_none());
    }

    #[test]
    fn serve_handles_a_request_stream() {
        let input = format!(
            "{}\n{}\nnot json\n",
            req(1, "initialize", json!({})),
            req(
                2,
                "tools/call",
                json!({ "name": "snapshot", "arguments": { "url": "u" } })
            ),
        );
        let mut out = Vec::new();
        serve(input.as_bytes(), &mut out, stub).unwrap();
        let out = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3); // initialize, tools/call, parse-error
        assert!(lines[0].contains("protocolVersion"));
        assert!(lines[1].contains("[markdown] u"));
        assert!(lines[2].contains("-32700")); // the "not json" line
    }
}
