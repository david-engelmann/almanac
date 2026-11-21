// The shared body of the stdio fixture servers.
//
// The fixture serves a surface modelled on telorama's, seven
// tools and one UI resource, over newline-delimited JSON-RPC. The
// including example file defines RENDER_TOOL, the name of the
// render tool in its own source, so the renamed fixture differs
// from the clean one by a source level rename followed by a
// rebuild, which is the stronger form of the rename test the
// design record owes. This file is included, never compiled
// alone.

use std::io::{BufRead, Write};

fn tools() -> serde_json::Value {
    let schema = |props: serde_json::Value| {
        serde_json::json!({
            "type": "object",
            "properties": props,
            "additionalProperties": false
        })
    };
    serde_json::json!([
        {"name": "validate_scene", "description": "Validate a scene spec.", "inputSchema": schema(serde_json::json!({"spec": {"type": "string"}}))},
        {"name": "inspect_traces", "description": "Inspect trace data.", "inputSchema": schema(serde_json::json!({"traces": {"type": "array"}}))},
        {"name": RENDER_TOOL, "description": "Render a scene.", "inputSchema": schema(serde_json::json!({"spec": {"type": "string"}}))},
        {"name": "session_run", "description": "Run a session.", "inputSchema": schema(serde_json::json!({"spec": {"type": "string"}}))},
        {"name": "session_frames", "description": "List session frames.", "inputSchema": schema(serde_json::json!({"session": {"type": "string"}}))},
        {"name": "scene_panel_detail", "description": "Detail one panel.", "inputSchema": schema(serde_json::json!({"panel": {"type": "string"}}))},
        {"name": "incident_scene", "description": "Build an incident scene.", "inputSchema": schema(serde_json::json!({"incident": {"type": "string"}}))}
    ])
}

fn main() {
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
        let id = request.get("id").cloned();
        let result = match method {
            "initialize" => Some(serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "serverInfo": {"name": "telorama", "version": "0.1.0"}
            })),
            "tools/list" => Some(serde_json::json!({"tools": tools()})),
            "resources/list" => Some(serde_json::json!({
                "resources": [{"uri": "ui://telorama/scene", "name": "Scene"}]
            })),
            "prompts/list" => None,
            _ => None,
        };
        let Some(id) = id else { continue };
        let response = match (method, result) {
            ("prompts/list", _) => serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": "method not found"}
            }),
            (_, Some(result)) => serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": result
            }),
            _ => serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": "method not found"}
            }),
        };
        let _ = writeln!(stdout, "{response}");
        let _ = stdout.flush();
    }
}
