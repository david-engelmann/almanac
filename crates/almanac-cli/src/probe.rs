//! The stdio probe.
//!
//! Spawns an MCP server process and walks its surface over
//! newline-delimited JSON-RPC, the same walk the HTTP probe makes,
//! and returns the one snapshot value the core compares. The
//! protocol version offered is the newest this build speaks. The
//! negotiated version comes back in the snapshot, because a server
//! that renegotiates its protocol has changed its surface.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use almanac_core::snapshot::{AuthSurface, ProbeFailure, SurfaceSnapshot, ToolSurface};
use serde_json::{Value, json};

pub const OFFERED_PROTOCOL_VERSION: &str = "2025-06-18";
const TIMEOUT: Duration = Duration::from_secs(10);

/// A snapshot for a probe that stopped at a step.
pub fn failed_snapshot(transport: &str, step: &str, detail: String) -> SurfaceSnapshot {
    SurfaceSnapshot {
        transport: transport.to_string(),
        server_name: String::new(),
        server_version: String::new(),
        protocol_version: String::new(),
        tools: vec![],
        resources: vec![],
        prompts: vec![],
        auth: AuthSurface::Open,
        failure: Some(ProbeFailure {
            step: step.to_string(),
            detail,
        }),
    }
}

/// Reduce a tools/list result page to tool surfaces.
pub fn tool_surfaces(result: &Value) -> Vec<ToolSurface> {
    result
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .map(|item| ToolSurface {
                    name: item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    description: item
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    input_schema: item
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Reduce a resources/list result page to sorted URIs.
pub fn resource_uris(result: &Value) -> Vec<String> {
    let mut uris: Vec<String> = result
        .get("resources")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("uri").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    uris.sort();
    uris
}

/// Reduce a prompts/list result page to sorted names.
pub fn prompt_names(result: &Value) -> Vec<String> {
    let mut names: Vec<String> = result
        .get("prompts")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// True when a JSON-RPC error is method-not-found, which for the
/// optional list methods means an empty set, not a failure.
pub fn is_method_not_found(response: &Value) -> bool {
    response
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64())
        == Some(-32601)
}

struct StdioClient {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    next_id: i64,
}

impl StdioClient {
    fn spawn(command: &[String]) -> Result<Self, String> {
        let mut child = Command::new(&command[0])
            .args(&command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not spawn {} with {e}", command[0]))?;
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(StdioClient {
            child,
            stdin,
            lines: rx,
            next_id: 1,
        })
    }

    fn send(&mut self, payload: &Value) -> Result<(), String> {
        let mut text = serde_json::to_string(payload).map_err(|e| e.to_string())?;
        text.push('\n');
        self.stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("write to server stdin failed with {e}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let line = self
                .lines
                .recv_timeout(TIMEOUT)
                .map_err(|_| format!("timed out waiting for the {method} response"))?;
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if value.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(error) = value.get("error") {
                    if is_method_not_found(&value) {
                        return Ok(json!({"__method_not_found": true}));
                    }
                    return Err(format!("server answered {method} with error {error}"));
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    fn notify(&mut self, method: &str) -> Result<(), String> {
        self.send(&json!({"jsonrpc": "2.0", "method": method}))
    }

    fn list_all(&mut self, method: &str) -> Result<Vec<Value>, String> {
        let mut pages = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(cursor) => json!({"cursor": cursor}),
                None => json!({}),
            };
            let result = self.request(method, params)?;
            if result.get("__method_not_found").is_some() {
                return Ok(vec![]);
            }
            cursor = result
                .get("nextCursor")
                .and_then(|v| v.as_str())
                .map(String::from);
            pages.push(result);
            if cursor.is_none() {
                return Ok(pages);
            }
        }
    }
}

/// Probe an MCP server over stdio.
pub fn probe_stdio(command: &[String]) -> SurfaceSnapshot {
    let transport = "stdio";
    let mut client = match StdioClient::spawn(command) {
        Ok(client) => client,
        Err(detail) => return failed_snapshot(transport, "spawn", detail),
    };
    let snapshot = walk(&mut client, transport);
    let _ = client.child.kill();
    snapshot
}

fn walk(client: &mut StdioClient, transport: &str) -> SurfaceSnapshot {
    let init = match client.request(
        "initialize",
        json!({
            "protocolVersion": OFFERED_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "almanac", "version": env!("CARGO_PKG_VERSION")}
        }),
    ) {
        Ok(result) => result,
        Err(detail) => return failed_snapshot(transport, "initialize", detail),
    };
    let server_info = init.get("serverInfo").cloned().unwrap_or(Value::Null);
    let mut snapshot = SurfaceSnapshot {
        transport: transport.to_string(),
        server_name: server_info
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        server_version: server_info
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        protocol_version: init
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        tools: vec![],
        resources: vec![],
        prompts: vec![],
        auth: AuthSurface::Open,
        failure: None,
    };
    if let Err(detail) = client.notify("notifications/initialized") {
        snapshot.failure = Some(ProbeFailure {
            step: "notifications/initialized".into(),
            detail,
        });
        return snapshot;
    }
    for (method, apply) in [
        ("tools/list", 0),
        ("resources/list", 1),
        ("prompts/list", 2),
    ] {
        match client.list_all(method) {
            Ok(pages) => {
                for page in &pages {
                    match apply {
                        0 => snapshot.tools.extend(tool_surfaces(page)),
                        1 => snapshot.resources.extend(resource_uris(page)),
                        _ => snapshot.prompts.extend(prompt_names(page)),
                    }
                }
                if apply == 1 {
                    snapshot.resources.sort();
                }
                if apply == 2 {
                    snapshot.prompts.sort();
                }
            }
            Err(detail) => {
                snapshot.failure = Some(ProbeFailure {
                    step: method.into(),
                    detail,
                });
                return snapshot;
            }
        }
    }
    snapshot
}
