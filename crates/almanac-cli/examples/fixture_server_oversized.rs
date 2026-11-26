//! Oversized stdio fixture server: one answer is a line
//! past the probe's response cap, the thirtieth slice's
//! test surface. The first argument names which answer
//! is oversized: "list" inflates a tool description in
//! tools/list, "read" inflates the contents text in
//! resources/read. Everything else is the smallest
//! surface that lets a walk reach the inflated answer.

use std::io::{BufRead, Write};

fn main() {
    let oversized = std::env::args().nth(1).unwrap_or_default();
    let giant = "x".repeat(17 * 1024 * 1024);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let result = match method {
            "initialize" => Some(serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "serverInfo": {"name": "telorama", "version": "0.1.0"}
            })),
            "tools/list" => {
                let description = if oversized == "list" {
                    giant.as_str()
                } else {
                    "Render a scene."
                };
                Some(serde_json::json!({
                    "tools": [{
                        "name": "render_scene",
                        "description": description,
                        "inputSchema": {"type": "object", "properties": {}}
                    }]
                }))
            }
            "resources/list" => Some(serde_json::json!({
                "resources": [{"uri": "ui://telorama/scene", "name": "Scene"}]
            })),
            "resources/read" => {
                let text = if oversized == "read" {
                    giant.as_str()
                } else {
                    "scene contents v1"
                };
                Some(serde_json::json!({
                    "contents": [{
                        "uri": "ui://telorama/scene",
                        "mimeType": "text/plain",
                        "text": text
                    }]
                }))
            }
            _ => None,
        };
        let response = match result {
            Some(result) => serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": result
            }),
            None => serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": "method not found"}
            }),
        };
        let _ = writeln!(stdout, "{response}");
        let _ = stdout.flush();
    }
}
