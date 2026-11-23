//! The stdio probe.
//!
//! Spawns an MCP server process and walks its surface over
//! newline-delimited JSON-RPC, the same walk the HTTP probe makes,
//! and returns the one snapshot value the core compares. The walk
//! enters through server/discover, the current revision's entry
//! point, and falls back to the initialize handshake the earlier
//! revisions use when the server does not know discover. The
//! protocol version the surface answers with comes back in the
//! snapshot, because a server that renegotiates its protocol has
//! changed its surface.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use almanac_core::snapshot::{AuthSurface, ProbeFailure, SurfaceSnapshot, ToolSurface};
use serde_json::{Value, json};

pub const OFFERED_PROTOCOL_VERSION: &str = "2025-06-18";

/// The current MCP protocol revision, frozen until the next
/// revision lands. Under it the protocol is stateless: no
/// initialize handshake, no sessions, identity carried per
/// request, and server/discover is the mandatory entry point.
pub const CURRENT_PROTOCOL_VERSION: &str = "2026-07-28";

/// The _meta block every request carries under the current
/// revision: the protocol version, the client identity and the
/// client capabilities, keyed the way the revision names them.
/// Identity travels per request now, not in a handshake.
pub(crate) fn request_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": CURRENT_PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "almanac", "version": env!("CARGO_PKG_VERSION")},
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

/// The parameters of the discover call, the current revision's
/// entry point: the meta block and nothing else.
pub(crate) fn discover_params() -> Value {
    json!({"_meta": request_meta()})
}

/// The parameters of a list call under the current revision:
/// the meta block, plus the cursor when paging.
pub(crate) fn list_params(cursor: Option<&str>) -> Value {
    let mut params = json!({"_meta": request_meta()});
    if let Some(cursor) = cursor {
        params["cursor"] = json!(cursor);
    }
    params
}

/// Build the open snapshot from a handshake-shaped result. A
/// discover result under the current revision and an initialize
/// result under the earlier ones carry the same fields the
/// snapshot reads, serverInfo and protocolVersion, so one
/// construction serves both ladders.
pub(crate) fn handshake_snapshot(
    transport: &str,
    result: &Value,
    stateless_shape: bool,
) -> SurfaceSnapshot {
    let server_info = result.get("serverInfo").cloned().unwrap_or(Value::Null);
    SurfaceSnapshot {
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
        protocol_version: result
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        stateless_shape,
        tools: vec![],
        resources: vec![],
        prompts: vec![],
        auth: AuthSurface::Open,
        failure: None,
    }
}

/// The normative server card URL for an MCP endpoint, per
/// SEP-2127 Final: the endpoint URL with /server-card appended.
/// Trailing slashes are trimmed, and a query string or fragment
/// is cut first, because the card path appends to the path.
/// One constructor serves both sides, the fetch that reads the
/// card and the generation that publishes it, so the two can
/// never drift apart on the path's shape.
pub(crate) fn normative_card_url(endpoint_url: &str) -> String {
    let base = endpoint_url
        .split(['?', '#'])
        .next()
        .unwrap_or(endpoint_url)
        .trim_end_matches('/');
    format!("{base}/server-card")
}
const TIMEOUT: Duration = Duration::from_secs(10);

/// The most pages one list call may span before the probe
/// records a failure. A server that always answers with another
/// cursor would otherwise hold the gate open forever.
pub(crate) const MAX_PAGES: usize = 100;

/// The three list calls every MCP surface answers, and how a
/// page of each lands in the snapshot. Both transports walk the
/// same three lists, so the walk is written once, here.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ListKind {
    Tools,
    Resources,
    Prompts,
}

impl ListKind {
    pub(crate) fn method(&self) -> &'static str {
        match self {
            ListKind::Tools => "tools/list",
            ListKind::Resources => "resources/list",
            ListKind::Prompts => "prompts/list",
        }
    }

    pub(crate) fn apply(&self, snapshot: &mut SurfaceSnapshot, page: &Value) {
        match self {
            ListKind::Tools => snapshot.tools.extend(tool_surfaces(page)),
            ListKind::Resources => snapshot.resources.extend(resource_uris(page)),
            ListKind::Prompts => snapshot.prompts.extend(prompt_names(page)),
        }
    }
}

pub(crate) const LIST_KINDS: [ListKind; 3] =
    [ListKind::Tools, ListKind::Resources, ListKind::Prompts];

/// A snapshot for a probe that stopped at a step.
pub fn failed_snapshot(transport: &str, step: &str, detail: String) -> SurfaceSnapshot {
    SurfaceSnapshot {
        transport: transport.to_string(),
        server_name: String::new(),
        server_version: String::new(),
        protocol_version: String::new(),
        stateless_shape: false,
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

    /// One JSON-RPC round trip. The timeout is a deadline for
    /// the whole request, not per line, so a server streaming
    /// unrelated lines cannot keep a request alive. When
    /// `allow_not_found` is set, a method-not-found error
    /// returns a sentinel the callers read two ways: the list
    /// calls treat it as an empty surface, and the discover
    /// call treats it as the signal to fall back to the
    /// handshake. The handshake itself passes false, because a
    /// missing initialize is a failure, not an empty surface.
    fn request(
        &mut self,
        method: &str,
        params: Value,
        allow_not_found: bool,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!("timed out waiting for the {method} response"));
            }
            let line = self
                .lines
                .recv_timeout(remaining)
                .map_err(|_| format!("timed out waiting for the {method} response"))?;
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if value.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(error) = value.get("error") {
                    if allow_not_found && is_method_not_found(&value) {
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

    /// Read one list to completion, following cursors. The walk
    /// is capped, and a server that repeats the cursor it was
    /// just given fails the call, so a paging loop can never
    /// hold the gate open.
    fn list_all(&mut self, method: &str, stateless: bool) -> Result<Vec<Value>, String> {
        let mut pages = Vec::new();
        let mut cursor: Option<String> = None;
        for _page in 0..MAX_PAGES {
            let params = if stateless {
                list_params(cursor.as_deref())
            } else {
                match &cursor {
                    Some(cursor) => json!({"cursor": cursor}),
                    None => json!({}),
                }
            };
            let result = self.request(method, params, true)?;
            if result.get("__method_not_found").is_some() {
                return Ok(pages);
            }
            let next = result
                .get("nextCursor")
                .and_then(|v| v.as_str())
                .map(String::from);
            pages.push(result);
            match next {
                None => return Ok(pages),
                Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
                Some(_) => return Err("the server repeated a pagination cursor".into()),
            }
        }
        Err(format!(
            "pagination did not terminate after {MAX_PAGES} pages"
        ))
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
    let _ = client.child.wait();
    snapshot
}

fn walk(client: &mut StdioClient, transport: &str) -> SurfaceSnapshot {
    // Discover first, the current revision's entry point. The
    // fallback is evidence based, as on the HTTP side: only the
    // method not found sentinel proves the server predates
    // discover. Any other discover failure (a different error,
    // a timeout, a result that is not an object or names no
    // server) is a failure at the server/discover step, because
    // falling back would let a broken discover hide behind a
    // healthy handshake and manufacture drift against the
    // legacy surface.
    match client.request("server/discover", discover_params(), true) {
        Ok(result) if result.get("__method_not_found").is_some() => {}
        Ok(result) => {
            if !result.is_object() {
                return failed_snapshot(
                    transport,
                    "server/discover",
                    "server answered server/discover without a usable result".into(),
                );
            }
            let mut snapshot = handshake_snapshot(transport, &result, true);
            if snapshot.server_name.is_empty() {
                return failed_snapshot(
                    transport,
                    "server/discover",
                    "the discover result named no server".into(),
                );
            }
            if snapshot.protocol_version.is_empty() {
                snapshot.protocol_version = CURRENT_PROTOCOL_VERSION.into();
            }
            gather_lists(client, &mut snapshot, true);
            return snapshot;
        }
        Err(detail) => return failed_snapshot(transport, "server/discover", detail),
    }
    // The server predates discover. The initialize handshake
    // below is the earlier revisions' entry, unchanged.
    let init = match client.request(
        "initialize",
        json!({
            "protocolVersion": OFFERED_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "almanac", "version": env!("CARGO_PKG_VERSION")}
        }),
        false,
    ) {
        Ok(result) => result,
        Err(detail) => return failed_snapshot(transport, "initialize", detail),
    };
    // The snapshot's stateless flag describes the walk's shape,
    // not the negotiated string, and this ladder walks statefully
    // whatever version the server names.
    let mut snapshot = handshake_snapshot(transport, &init, false);
    if let Err(detail) = client.notify("notifications/initialized") {
        snapshot.failure = Some(ProbeFailure {
            step: "notifications/initialized".into(),
            detail,
        });
        return snapshot;
    }
    gather_lists(client, &mut snapshot, false);
    snapshot
}

/// Gather the three lists into the snapshot, recording the
/// step that failed when one does. Both entry ladders end
/// here. Under the current revision the list requests carry
/// the _meta block like every other request; behind the
/// handshake they carry only the cursor.
fn gather_lists(client: &mut StdioClient, snapshot: &mut SurfaceSnapshot, stateless: bool) {
    for kind in LIST_KINDS {
        match client.list_all(kind.method(), stateless) {
            Ok(pages) => {
                for page in &pages {
                    kind.apply(snapshot, page);
                }
            }
            Err(detail) => {
                snapshot.failure = Some(ProbeFailure {
                    step: kind.method().into(),
                    detail,
                });
                return;
            }
        }
    }
    snapshot.resources.sort();
    snapshot.prompts.sort();
}
