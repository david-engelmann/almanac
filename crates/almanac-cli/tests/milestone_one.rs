//! Milestone one end to end tests.
//!
//! Three fixture families. A stdio fixture server and its source
//! level renamed twin, exercising the stdio probe and the rename
//! test the design record owes. An HTTP fixture serving the same
//! surface over Streamable HTTP with planted breaks. An auth
//! fixture answering 401 with OAuth metadata, exercising the
//! ladder and the declared evidence degradation.
//!
//! The auth baseline is patched into the generated baseline by the
//! test, because generation for protected surfaces lands in a
//! later milestone. The metadata records in it are built from the
//! fixture's real documents through the core crate's own
//! reduction, so the comparison under test is the real one.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use almanac_core::baseline::{AuthBaseline, Baseline};
use almanac_core::snapshot::MetadataRecord;

fn almanac() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_almanac"))
}

fn fixture_binary(name: &str) -> PathBuf {
    almanac()
        .parent()
        .expect("binary has a parent")
        .join("examples")
        .join(name)
}

fn run(args: &[&str]) -> (i32, String, String) {
    run_with_env(args, &[])
}

/// Run the binary with extra environment variables, the
/// credential tests' shape: the token reaches the probe
/// through the environment and nowhere else.
fn run_with_env(args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new(almanac())
        .args(args)
        .envs(env.iter().copied())
        .output()
        .expect("almanac runs");
    (
        status.code().unwrap_or(-1),
        String::from_utf8_lossy(&stdout).to_string(),
        String::from_utf8_lossy(&stderr).to_string(),
    )
}

fn temp_site(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almanac-m1-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp site dir");
    dir
}

fn site_arg(site: &Path) -> String {
    site.to_string_lossy().to_string()
}

fn generate_stdio(site: &Path, fixture: &Path) -> (i32, String, String) {
    run(&[
        "generate",
        "--site",
        &site_arg(site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--command",
        &fixture.to_string_lossy(),
    ])
}

fn check_stdio(site: &Path, fixture: &Path) -> (i32, String, String) {
    run(&[
        "check",
        "--site",
        &site_arg(site),
        "--command",
        &fixture.to_string_lossy(),
    ])
}

fn generate_stdio_with_skill(
    site: &Path,
    fixture: &Path,
    skill_file: &Path,
) -> (i32, String, String) {
    run(&[
        "generate",
        "--site",
        &site_arg(site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--skill",
        &skill_file.to_string_lossy(),
        "--command",
        &fixture.to_string_lossy(),
    ])
}

#[test]
fn stdio_clean_then_source_level_rename() {
    let site = temp_site("stdio");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &clean);
    assert_eq!(code, 0, "generate failed with {stderr}");
    assert!(site.join(".well-known/ard.json").exists());
    assert!(site.join(".well-known/ai-catalog.json").exists());
    assert!(site.join("baseline.json").exists());

    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");
    assert!(stdout.contains("\"CRITICAL\": 0"), "report was {stdout}");

    let renamed = fixture_binary("fixture_server_renamed");
    let (code, stdout, _) = check_stdio(&site, &renamed);
    assert_eq!(code, 1, "rename must fail the gate, report was {stdout}");
    assert!(
        stdout.contains("announced-tool-missing"),
        "report was {stdout}"
    );
    assert!(
        stdout.contains("unannounced-tool-served"),
        "report was {stdout}"
    );
}

#[test]
fn skill_digest_and_path_breaks() {
    let site = temp_site("breaks");
    let skill_dir = temp_site("skill-src");
    let skill_file = skill_dir.join("SKILL.md");
    std::fs::write(
        &skill_file,
        "---\nname: demo-skill\ndescription: A demo skill.\n---\n\nDo the demo thing.\n",
    )
    .expect("skill file");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio_with_skill(&site, &clean, &skill_file);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let (code, _, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0);

    // A skill edit without regeneration is major drift.
    let site_skill = site.join("skills/demo-skill/SKILL.md");
    let mut text = std::fs::read_to_string(&site_skill).expect("site skill");
    text.push_str("\nAn unannounced instruction.\n");
    std::fs::write(&site_skill, text).expect("edit site skill");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "skill edit must fail, report was {stdout}");
    assert!(
        stdout.contains("skill-content-changed"),
        "report was {stdout}"
    );
    let (code, _, _) = generate_stdio(&site, &clean);
    assert_eq!(code, 0);
    let (code, _, _) = generate_stdio_with_skill(&site, &clean, &skill_file);
    assert_eq!(code, 0);

    // A served artifact that no longer matches its entry digest is
    // critical.
    let card_path = site.join(".well-known/mcp/server-card.json");
    let mut card = std::fs::read_to_string(&card_path).expect("card");
    card.push('\n');
    std::fs::write(&card_path, card).expect("edit card");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "digest break must fail, report was {stdout}");
    assert!(stdout.contains("digest-mismatch"), "report was {stdout}");

    // A missing legacy path is a broken pair.
    let (code, _, _) = generate_stdio(&site, &clean);
    assert_eq!(code, 0);
    let (code, _, _) = generate_stdio_with_skill(&site, &clean, &skill_file);
    assert_eq!(code, 0);
    std::fs::remove_file(site.join(".well-known/ai-catalog.json")).expect("remove legacy");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(
        code, 1,
        "missing legacy path must fail, report was {stdout}"
    );
    assert!(stdout.contains("path-divergence"), "report was {stdout}");
}

// ----------------------------------------------------------------
// The HTTP fixture.

type Mode = Arc<Mutex<String>>;

/// The JSON-RPC methods the HTTP fixture has received, in
/// order, for the tests that pin the probe's entry point.
type Log = Arc<Mutex<Vec<String>>>;

fn fixture_tools(mode: &str) -> serde_json::Value {
    let schema = |extra: serde_json::Value| {
        let mut base =
            serde_json::json!({"type": "object", "properties": {"spec": {"type": "string"}}});
        if let (Some(props), Some(extra)) = (base.get_mut("properties"), extra.as_object()) {
            for (key, value) in extra {
                props[key] = value.clone();
            }
        }
        base
    };
    let render = if mode == "open-rename" {
        "render_scenes"
    } else {
        "render_scene"
    };
    let validate_schema = if mode == "open-schema" {
        schema(serde_json::json!({"strict": {"type": "boolean"}}))
    } else {
        schema(serde_json::json!({}))
    };
    let mut tools = vec![
        serde_json::json!({"name": "validate_scene", "description": "Validate a scene spec.", "inputSchema": validate_schema}),
        serde_json::json!({"name": "inspect_traces", "description": "Inspect trace data.", "inputSchema": schema(serde_json::json!({}))}),
        serde_json::json!({"name": render, "description": "Render a scene.", "inputSchema": schema(serde_json::json!({}))}),
        serde_json::json!({"name": "session_run", "description": "Run a session.", "inputSchema": schema(serde_json::json!({}))}),
        serde_json::json!({"name": "session_frames", "description": "List session frames.", "inputSchema": schema(serde_json::json!({}))}),
        serde_json::json!({"name": "scene_panel_detail", "description": "Detail one panel.", "inputSchema": schema(serde_json::json!({}))}),
        serde_json::json!({"name": "incident_scene", "description": "Build an incident scene.", "inputSchema": schema(serde_json::json!({}))}),
    ];
    if mode == "open-add" {
        tools.push(serde_json::json!({"name": "extra_tool", "description": "Never announced.", "inputSchema": schema(serde_json::json!({}))}));
    }
    serde_json::Value::Array(tools)
}

fn start_http_fixture() -> (String, Mode) {
    let (url, mode, _log) = start_http_fixture_logged();
    (url, mode)
}

fn start_http_fixture_logged() -> (String, Mode, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let port = listener.local_addr().expect("addr").port();
    let mode: Mode = Arc::new(Mutex::new("open-clean".to_string()));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let server_mode = mode.clone();
    let server_log = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mode = server_mode.clone();
            let log = server_log.clone();
            std::thread::spawn(move || {
                handle_http(&mut stream, &mode, &log, port);
            });
        }
    });
    (format!("http://127.0.0.1:{port}/mcp"), mode, log)
}

fn read_request(stream: &mut std::net::TcpStream) -> (String, String, String, String) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            return (String::new(), String::new(), String::new(), String::new());
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
    }
    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let first_line = headers.lines().next().unwrap_or("").to_string();
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    (
        method,
        path,
        headers.to_lowercase(),
        String::from_utf8_lossy(&body).to_string(),
    )
}

fn respond(
    stream: &mut std::net::TcpStream,
    status: u16,
    content_type: &str,
    extra_headers: &str,
    body: &str,
) {
    let text = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(text.as_bytes());
    let _ = stream.flush();
}

fn fixture_card() -> serde_json::Value {
    serde_json::json!({
        "name": "telorama",
        "version": "0.1.0",
        "protocolVersion": "2025-06-18",
        "transport": "streamable-http",
        "description": "The telorama MCP server, from its card.",
        "tools": fixture_tools("open-clean"),
        "resources": [{"uri": "ui://telorama/scene"}],
        "prompts": []
    })
}

/// The same server described by a card in the SEP-2127 Final
/// shape, for the auth-card-final mode. The endpoint is filled
/// in by the handler, which knows the port.
fn fixture_card_final(endpoint: &str) -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json",
        "name": "com.example/telorama",
        "title": "telorama",
        "version": "0.1.0",
        "description": "The telorama MCP server, from its card.",
        "remotes": [{
            "type": "streamable-http",
            "url": endpoint,
            "supportedProtocolVersions": ["2025-06-18"]
        }]
    })
}

fn handle_http(stream: &mut std::net::TcpStream, mode: &Mode, log: &Log, port: u16) {
    let (method, path, headers, body) = read_request(stream);
    let mode = mode.lock().expect("mode lock").clone();
    let origin = format!("http://127.0.0.1:{port}");
    match (method.as_str(), path.as_str()) {
        ("GET", "/.well-known/oauth-protected-resource") => {
            if mode == "auth-metadata-gone" {
                respond(stream, 404, "text/plain", "", "not found");
            } else {
                let doc = serde_json::json!({
                    "resource": format!("{origin}/mcp"),
                    "authorization_servers": [origin]
                });
                respond(stream, 200, "application/json", "", &doc.to_string());
            }
        }
        ("GET", "/.well-known/oauth-authorization-server") => {
            let scopes = if mode == "auth-scope-removed" {
                serde_json::json!(["read"])
            } else {
                serde_json::json!(["read", "write"])
            };
            let issuer = if mode == "auth-issuer-changed" {
                "https://issuer.example/other".to_string()
            } else {
                origin.clone()
            };
            let doc = serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{origin}/authorize"),
                "token_endpoint": format!("{origin}/token"),
                "jwks_uri": format!("{origin}/jwks"),
                "scopes_supported": scopes
            });
            respond(stream, 200, "application/json", "", &doc.to_string());
        }
        ("GET", "/.well-known/mcp/server-card.json") => {
            if mode == "auth-card-normative" || mode == "auth-card-final" {
                respond(stream, 404, "text/plain", "", "not found");
            } else if mode == "auth-card-both" {
                // Both locations serve, with different cards, so
                // the suite can pin which one the fetch prefers.
                let mut card = fixture_card();
                card["version"] = serde_json::json!("0.0.9");
                respond(stream, 200, "application/json", "", &card.to_string());
            } else {
                respond(
                    stream,
                    200,
                    "application/json",
                    "",
                    &fixture_card().to_string(),
                );
            }
        }
        ("GET", "/mcp/server-card") => {
            if mode == "auth-card-final" {
                respond(
                    stream,
                    200,
                    "application/json",
                    "",
                    &fixture_card_final(&format!("{origin}/mcp")).to_string(),
                );
            } else if mode == "auth-card-normative" || mode == "auth-card-both" {
                respond(
                    stream,
                    200,
                    "application/json",
                    "",
                    &fixture_card().to_string(),
                );
            } else {
                respond(stream, 404, "text/plain", "", "not found");
            }
        }
        ("POST", "/mcp") => {
            // The credential mode serves the walk only to the
            // request carrying the fixture's token, and
            // challenges everything else.
            if mode == "auth-token" && headers.contains("authorization: bearer test-token") {
                // Fall through to the open serving path below.
            } else if mode.starts_with("auth-") {
                let header = format!(
                    "WWW-Authenticate: Bearer resource_metadata=\"{origin}/.well-known/oauth-protected-resource\"\r\n"
                );
                respond(stream, 401, "text/plain", &header, "unauthorized");
                return;
            }
            let request: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let method_name = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
            log.lock().expect("log lock").push(method_name.to_string());
            // The current revision's mode is strict the other
            // way: a stateless request carries no session id and
            // no protocol version header, and identity rides in
            // the _meta block on every call, so the fixture
            // refuses the headers and requires the meta.
            if mode == "open-2026" {
                let request_id = request
                    .get("id")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                if headers.contains("mcp-session-id:") || headers.contains("mcp-protocol-version:")
                {
                    respond(
                        stream,
                        400,
                        "application/json",
                        "",
                        &serde_json::json!({
                            "jsonrpc": "2.0", "id": request_id,
                            "error": {"code": -32602, "message": "stateless requests carry no session or version header"}
                        })
                        .to_string(),
                    );
                    return;
                }
                let carries_meta = request
                    .get("params")
                    .and_then(|p| p.get("_meta"))
                    .and_then(|m| m.get("io.modelcontextprotocol/protocolVersion"))
                    .is_some();
                if matches!(
                    method_name,
                    "server/discover" | "tools/list" | "resources/list" | "prompts/list"
                ) && !carries_meta
                {
                    respond(
                        stream,
                        200,
                        "application/json",
                        "",
                        &serde_json::json!({
                            "jsonrpc": "2.0", "id": request_id,
                            "error": {"code": -32602, "message": "missing _meta"}
                        })
                        .to_string(),
                    );
                    return;
                }
            }
            // Session strict. The fixture issues a session id at
            // initialize and refuses later calls that do not echo
            // it, so the suite catches a probe that stops echoing.
            // The check guards the methods the fixture serves,
            // because a method it does not implement is answered
            // method not found below whatever the session state,
            // the way a real framework dispatches. The current
            // revision's mode is stateless and exempt, because
            // its requests carry no session by design.
            if mode != "open-2026"
                && matches!(
                    method_name,
                    "tools/list" | "resources/list" | "prompts/list"
                )
                && !headers.contains("mcp-session-id: fixture-session")
            {
                respond(
                    stream,
                    400,
                    "application/json",
                    "",
                    "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32000,\"message\":\"missing session id\"}}",
                );
                return;
            }
            let id = request.get("id").cloned();
            let Some(id) = id else {
                respond(stream, 202, "text/plain", "", "");
                return;
            };
            let cursor = request
                .get("params")
                .and_then(|p| p.get("cursor"))
                .and_then(|c| c.as_str())
                .map(String::from);
            let payload = match method_name {
                "server/discover" if mode == "open-2026" => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "protocolVersion": "2026-07-28",
                        "capabilities": {},
                        "serverInfo": {"name": "telorama", "version": "0.1.0"}
                    }
                }),
                "initialize" if mode == "open-2026" => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": {"code": -32601, "message": "method not found"}
                }),
                "initialize" => {
                    let version = if mode == "open-version" {
                        "0.2.0"
                    } else {
                        "0.1.0"
                    };
                    serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {
                            "protocolVersion": "2025-06-18",
                            "capabilities": {},
                            "serverInfo": {"name": "telorama", "version": version}
                        }
                    })
                }
                "tools/list" if mode == "open-paged" => {
                    let all = fixture_tools("open-clean");
                    let items = all.as_array().expect("tools array");
                    match cursor.as_deref() {
                        None => serde_json::json!({
                            "jsonrpc": "2.0", "id": id,
                            "result": {"tools": items[..4].to_vec(), "nextCursor": "page2"}
                        }),
                        Some("page2") => serde_json::json!({
                            "jsonrpc": "2.0", "id": id,
                            "result": {"tools": items[4..].to_vec()}
                        }),
                        _ => serde_json::json!({
                            "jsonrpc": "2.0", "id": id,
                            "error": {"code": -32602, "message": "bad cursor"}
                        }),
                    }
                }
                "tools/list" if mode == "open-loop" => {
                    let page: usize = cursor
                        .as_deref()
                        .and_then(|c| c.strip_prefix('p'))
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0);
                    let all = fixture_tools("open-clean");
                    let items = all.as_array().expect("tools array");
                    serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {
                            "tools": [items[0].clone()],
                            "nextCursor": format!("p{}", page + 1)
                        }
                    })
                }
                "tools/list" => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"tools": fixture_tools(&mode)}
                }),
                "resources/list" if mode == "open-resources" => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"resources": []}
                }),
                "resources/list" => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"resources": [{"uri": "ui://telorama/scene", "name": "Scene"}]}
                }),
                _ => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": {"code": -32601, "message": "method not found"}
                }),
            };
            if method_name == "tools/list" && mode == "open-sse" {
                // The event stream framing, with the format's
                // canonical CRLF line endings.
                let frame = format!("event: message\r\ndata: {payload}\r\n\r\n");
                respond(
                    stream,
                    200,
                    "text/event-stream",
                    "Mcp-Session-Id: fixture-session\r\n",
                    &frame,
                );
                return;
            }
            respond(
                stream,
                200,
                "application/json",
                "Mcp-Session-Id: fixture-session\r\n",
                &payload.to_string(),
            );
        }
        _ => respond(stream, 404, "text/plain", "", "not found"),
    }
}

fn generate_http(site: &Path, url: &str) -> (i32, String, String) {
    run(&[
        "generate",
        "--site",
        &site_arg(site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--url",
        url,
    ])
}

fn check_http(site: &Path, url: &str) -> (i32, String, String) {
    run(&["check", "--site", &site_arg(site), "--url", url])
}

#[test]
fn http_clean_then_planted_breaks() {
    let (url, mode) = start_http_fixture();
    let site = temp_site("http");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    *mode.lock().expect("mode") = "open-rename".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "rename must fail, report was {stdout}");
    assert!(
        stdout.contains("announced-tool-missing"),
        "report was {stdout}"
    );

    *mode.lock().expect("mode") = "open-add".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "added tool must fail, report was {stdout}");
    assert!(
        stdout.contains("unannounced-tool-served"),
        "report was {stdout}"
    );

    *mode.lock().expect("mode") = "open-schema".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "schema change must fail, report was {stdout}");
    assert!(
        stdout.contains("tool-schema-changed"),
        "report was {stdout}"
    );
}

/// Patch a generated baseline's auth record from the fixture's
/// real metadata documents, through the core reduction, the way
/// protected generation will do it in a later milestone.
fn patch_auth_baseline(site: &Path, origin: &str) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let prm_url = format!("{origin}/.well-known/oauth-protected-resource");
    let prm: serde_json::Value = agent
        .get(&prm_url)
        .call()
        .expect("prm fetch")
        .body_mut()
        .read_json()
        .expect("prm json");
    let as_url = format!("{origin}/.well-known/oauth-authorization-server");
    let as_doc: serde_json::Value = agent
        .get(&as_url)
        .call()
        .expect("as fetch")
        .body_mut()
        .read_json()
        .expect("as json");
    let baseline_path = site.join("baseline.json");
    let mut baseline: Baseline =
        serde_json::from_slice(&std::fs::read(&baseline_path).expect("baseline"))
            .expect("baseline parses");
    baseline.mcp.as_mut().expect("mcp baseline").auth =
        AuthBaseline::Protected(Box::new(almanac_core::snapshot::ProtectedMetadata {
            resource_metadata: Some(MetadataRecord::from_document(&prm_url, &prm)),
            authorization_servers: vec![MetadataRecord::from_document(&as_url, &as_doc)],
        }));
    std::fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&baseline).expect("serialize"),
    )
    .expect("write baseline");
}

#[test]
fn auth_ladder_and_metadata_breaks() {
    let (url, mode) = start_http_fixture();
    let origin = url.strip_suffix("/mcp").expect("origin").to_string();
    let site = temp_site("auth");

    // Baseline from the open surface first.
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // The open baseline against the now protected surface is a
    // posture flip, major authentication drift.
    *mode.lock().expect("mode") = "auth-clean".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "posture flip must fail, report was {stdout}");
    assert!(
        stdout.contains("authentication-changed"),
        "report was {stdout}"
    );

    patch_auth_baseline(&site, &origin);

    // Clean protected run, declared evidence only, exit 0.
    let (code, stdout, stderr) = check_http(&site, &url);
    assert_eq!(code, 0, "clean protected check failed with {stdout}");
    assert!(
        stdout.contains("declared-evidence-only"),
        "report was {stdout}"
    );
    assert!(
        stderr.contains("declared evidence only"),
        "stderr was {stderr}"
    );

    // A removed scope is major authentication drift.
    *mode.lock().expect("mode") = "auth-scope-removed".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "scope removal must fail, report was {stdout}");
    assert!(
        stdout.contains("authentication-changed"),
        "report was {stdout}"
    );
    assert!(stdout.contains("scopes_supported"), "report was {stdout}");

    // Metadata that vanishes is major authentication drift.
    *mode.lock().expect("mode") = "auth-metadata-gone".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "vanished metadata must fail, report was {stdout}");
    assert!(
        stdout.contains("authentication-changed"),
        "report was {stdout}"
    );

    // A changed issuer in the metadata is major drift, named by
    // field.
    *mode.lock().expect("mode") = "auth-issuer-changed".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "issuer change must fail, report was {stdout}");
    assert!(
        stdout.contains("authentication-changed"),
        "report was {stdout}"
    );
    assert!(stdout.contains("issuer"), "report was {stdout}");
}

// ----------------------------------------------------------------
// The checkpoint fix pass. Each test below answers one review
// finding, and each can fail for the reason its finding named.

#[test]
fn http_sse_pagination_version_and_resource_breaks() {
    let (url, mode) = start_http_fixture();
    let site = temp_site("http2");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // The event stream framing, CRLF and all, reads clean.
    *mode.lock().expect("mode") = "open-sse".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "SSE framed check failed with {stdout}");

    // The same surface over two pages reads clean.
    *mode.lock().expect("mode") = "open-paged".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "paged check failed with {stdout}");

    // A server that pages forever fails the step, never hangs.
    *mode.lock().expect("mode") = "open-loop".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "endless paging must fail, report was {stdout}");
    assert!(
        stdout.contains("pagination did not terminate"),
        "report was {stdout}"
    );

    // A version change is major drift.
    *mode.lock().expect("mode") = "open-version".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "version change must fail, report was {stdout}");
    assert!(
        stdout.contains("server-version-changed"),
        "report was {stdout}"
    );

    // A resource set change is major drift.
    *mode.lock().expect("mode") = "open-resources".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "resource change must fail, report was {stdout}");
    assert!(
        stdout.contains("resource-set-changed"),
        "report was {stdout}"
    );
}

#[test]
fn unreachable_is_exit_1_and_unparseable_is_exit_2() {
    let site = temp_site("exits");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &clean);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // An announced server that does not answer is drift at
    // critical severity, exit 1, never a could-not-run.
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--url",
        "http://127.0.0.1:1/mcp",
    ]);
    assert_eq!(code, 1, "unreachable must exit 1, report was {stdout}");
    assert!(stdout.contains("server-unreachable"), "report was {stdout}");

    // A catalog that does not parse is a check that could not
    // run, exit 2.
    std::fs::write(site.join(".well-known/ard.json"), "not json").expect("corrupt catalog");
    let (code, _, _) = check_stdio(&site, &clean);
    assert_eq!(code, 2, "unparseable catalog must exit 2");
}

#[test]
fn repo_mode_phantom_tool_and_legacy_only() {
    let site = temp_site("repomode");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &clean);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // Repo mode, no target named, runs the offline comparisons
    // and passes a coherent site.
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "repo mode check failed with {stdout}");
    assert!(stdout.contains("\"mode\": \"repo\""), "report was {stdout}");

    // A phantom tool announced by the catalog pair, in repo mode
    // and in live mode alike, is critical divergence. This is
    // the red team's lying catalog, and it must not pass.
    for path in [".well-known/ard.json", ".well-known/ai-catalog.json"] {
        let full = site.join(path);
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&full).expect("catalog")).expect("parses");
        catalog["entries"][0]["capabilities"]
            .as_array_mut()
            .expect("capabilities")
            .push(serde_json::json!("evil_tool"));
        std::fs::write(
            &full,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&catalog).expect("serialize")
            ),
        )
        .expect("write catalog");
    }
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 1, "phantom must fail repo mode, report was {stdout}");
    assert!(
        stdout.contains("catalog-baseline-divergence"),
        "report was {stdout}"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "phantom must fail live mode, report was {stdout}");
    assert!(
        stdout.contains("catalog-baseline-divergence"),
        "report was {stdout}"
    );

    // A site on the legacy path alone gets the major class the
    // design record defines for it, exit 1, not a silent exit 2.
    let (code, _, _) = generate_stdio(&site, &clean);
    assert_eq!(code, 0);
    std::fs::remove_file(site.join(".well-known/ard.json")).expect("remove current path");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "legacy only must exit 1, report was {stdout}");
    assert!(
        stdout.contains("legacy-only-catalog"),
        "report was {stdout}"
    );
}

#[test]
fn suppression_holds_then_expires() {
    let (url, mode) = start_http_fixture();
    let site = temp_site("suppress");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");
    *mode.lock().expect("mode") = "open-schema".to_string();
    let (code, _, _) = check_http(&site, &url);
    assert_eq!(code, 1, "schema change fails before any suppression");

    std::fs::write(
        site.join("almanac.toml"),
        "[[suppress]]\nkind = \"tool-schema-changed\"\nreason = \"known and accepted for this test\"\nexpires = \"2999-01-01\"\n",
    )
    .expect("write config");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "suppressed check must pass, report was {stdout}");

    std::fs::write(
        site.join("almanac.toml"),
        "[[suppress]]\nkind = \"tool-schema-changed\"\nreason = \"known and accepted for this test\"\nexpires = \"2000-01-01\"\n",
    )
    .expect("write expired config");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(
        code, 1,
        "expired suppression must fail, report was {stdout}"
    );
    assert!(
        stdout.contains("suppression-expired"),
        "report was {stdout}"
    );
    assert!(
        stdout.contains("tool-schema-changed"),
        "report was {stdout}"
    );

    // A suppression without a reason is a configuration error,
    // and the check does not run.
    std::fs::write(
        site.join("almanac.toml"),
        "[[suppress]]\nkind = \"tool-schema-changed\"\nreason = \"\"\nexpires = \"2999-01-01\"\n",
    )
    .expect("write reasonless config");
    let (code, _, _) = check_http(&site, &url);
    assert_eq!(code, 2, "a reasonless suppression must exit 2");
}

#[test]
fn unannounced_skill_file_is_critical() {
    let site = temp_site("sneaky");
    let skill_dir = temp_site("sneaky-src");
    let skill_file = skill_dir.join("SKILL.md");
    std::fs::write(
        &skill_file,
        "---\nname: demo-skill\ndescription: A demo skill.\n---\n\nDo the demo thing.\n",
    )
    .expect("skill file");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio_with_skill(&site, &clean, &skill_file);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let (code, _, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0);

    let sneaky = site.join("skills/sneaky-skill");
    std::fs::create_dir_all(&sneaky).expect("sneaky dir");
    std::fs::write(sneaky.join("SKILL.md"), "Do something unannounced.\n").expect("sneaky skill");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "unannounced skill must fail, report was {stdout}");
    assert!(
        stdout.contains("unannounced-skill-served"),
        "report was {stdout}"
    );
}

#[test]
fn card_divergence_in_declared_mode() {
    let (url, mode) = start_http_fixture();
    let origin = url.strip_suffix("/mcp").expect("origin").to_string();
    let site = temp_site("card");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");
    *mode.lock().expect("mode") = "auth-clean".to_string();
    patch_auth_baseline(&site, &origin);
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "clean declared check failed with {stdout}");

    // A card that stops naming one of the baseline's tools is
    // critical divergence in declared evidence mode. The
    // published card is Final shaped and lists no tools, so
    // the diverging card is an enumerating one in the earlier
    // shape, missing its last tool.
    let card_path = site.join(".well-known/mcp/server-card.json");
    let mut card = fixture_card();
    card["tools"].as_array_mut().expect("tools").pop();
    std::fs::write(
        &card_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&card).expect("serialize")
        ),
    )
    .expect("write card");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "card divergence must fail, report was {stdout}");
    assert!(stdout.contains("card-divergence"), "report was {stdout}");
}

// ----------------------------------------------------------------
// Milestone two. Protected generation from the server card, and
// the validate command.

#[test]
fn protected_generation_from_the_card() {
    let (url, mode) = start_http_fixture();
    let site = temp_site("protgen");
    *mode.lock().expect("mode") = "auth-clean".to_string();

    // Generation against the protected surface succeeds from the
    // card the fixture serves, no hand patched baseline anywhere.
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "protected generate failed with {stderr}");
    let baseline: Baseline =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert!(
        matches!(
            baseline.mcp.as_ref().expect("mcp baseline").auth,
            AuthBaseline::Protected(_)
        ),
        "the baseline must record the protected posture from the ladder"
    );
    let catalog_text = std::fs::read_to_string(site.join(".well-known/ard.json")).expect("catalog");
    assert!(
        catalog_text.contains("The telorama MCP server, from its card."),
        "the entry description must come from the card, catalog was {catalog_text}"
    );

    // The declared evidence check over what generation recorded
    // is clean.
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "clean declared check failed with {stdout}");
    assert!(
        stdout.contains("declared-evidence-only"),
        "report was {stdout}"
    );

    // And the auth ladder still catches a removed scope against
    // the card generated baseline.
    *mode.lock().expect("mode") = "auth-scope-removed".to_string();
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "scope removal must fail, report was {stdout}");
    assert!(
        stdout.contains("authentication-changed"),
        "report was {stdout}"
    );
}

#[test]
fn migrate_carries_tool_claims_unverified_from_a_final_card() {
    let (url, mode) = start_http_fixture();
    let site = temp_site("mig-final");
    *mode.lock().expect("mode") = "auth-card-final".to_string();
    let entry = serde_json::json!({
        "identifier": "urn:air:example.com:mcp:telorama",
        "displayName": "telorama",
        "type": "application/mcp-server-card+json",
        "url": "https://example.com/.well-known/mcp/server-card.json",
        "description": "The telorama MCP server.",
        "capabilities": ["render_scene", "validate_scene"],
        "representativeQueries": ["What can telorama do", "Use telorama to complete a task"],
        "version": "0.1.0"
    });
    let catalog = serde_json::json!({
        "host": {"displayName": "Example", "domain": "example.com"},
        "entries": [entry]
    });
    std::fs::create_dir_all(site.join(".well-known")).expect("well known");
    std::fs::write(
        site.join(".well-known/ai-catalog.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&catalog).expect("serialize")
        ),
    )
    .expect("legacy catalog");
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, stderr) = run(&[
        "migrate",
        "--site",
        &site_arg(&site),
        "--ci",
        &ci.to_string_lossy(),
        "--url",
        &url,
    ]);
    // The only card is a Final one, which enumerates no tools.
    // The entry's tool claims cannot be judged, so they carry
    // unverified and the migration succeeds, saying so. Judged
    // against an empty list, every claim would fail and the
    // entry would drop.
    assert_eq!(code, 0, "migrate failed with {stdout} {stderr}");
    assert!(
        stdout.contains("carried unverified"),
        "the report must say the claims carried unverified, report was {stdout}"
    );
    let migrated: serde_json::Value = serde_json::from_slice(
        &std::fs::read(site.join(".well-known/ard.json")).expect("migrated catalog"),
    )
    .expect("migrated catalog parses");
    assert_eq!(
        migrated["entries"][0]["capabilities"],
        serde_json::json!(["render_scene", "validate_scene"]),
        "the entry keeps the claims it announced, catalog was {migrated}"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert_eq!(
        baseline["mcp"]["server_name"].as_str(),
        Some("telorama"),
        "baseline was {baseline}"
    );
    assert!(
        baseline["mcp"]["tools"]
            .as_object()
            .expect("tools")
            .is_empty(),
        "the baseline records no tools from an unenumerating card, baseline was {baseline}"
    );
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "check after migrate failed with {stdout}");
}

#[test]
fn generated_card_is_final_shaped() {
    let (url, _mode) = start_http_fixture();
    let site = temp_site("cardshape");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let card: serde_json::Value = serde_json::from_slice(
        &std::fs::read(site.join(".well-known/mcp/server-card.json")).expect("card"),
    )
    .expect("card parses");
    assert_eq!(
        card["$schema"].as_str(),
        Some("https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json"),
        "card was {card}"
    );
    assert_eq!(
        card["name"].as_str(),
        Some("com.example/telorama"),
        "card was {card}"
    );
    assert_eq!(card["title"].as_str(), Some("telorama"), "card was {card}");
    assert_eq!(card["version"].as_str(), Some("0.1.0"), "card was {card}");
    let description = card["description"].as_str().expect("description");
    assert!(
        description.chars().count() <= 100,
        "description was {description}"
    );
    assert!(
        card.get("tools").is_none(),
        "a Final card lists no tools, card was {card}"
    );
    assert!(
        card.get("endpoint").is_none(),
        "the endpoint lives in remotes, card was {card}"
    );
    assert_eq!(
        card["remotes"][0]["type"].as_str(),
        Some("streamable-http"),
        "card was {card}"
    );
    assert_eq!(
        card["remotes"][0]["url"].as_str(),
        Some(url.as_str()),
        "card was {card}"
    );
    assert_eq!(
        card["remotes"][0]["supportedProtocolVersions"][0].as_str(),
        Some("2025-06-18"),
        "card was {card}"
    );

    // A check with no target named resolves the endpoint from
    // the Final card's remotes and runs clean.
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "check with a resolved target failed with {stdout}");
}

#[test]
fn protected_generation_from_a_final_card() {
    let (url, mode) = start_http_fixture();
    let site = temp_site("protgenfinal");
    *mode.lock().expect("mode") = "auth-card-final".to_string();

    // Generation succeeds from a card that declares identity
    // and endpoints alone, served only at the normative
    // location.
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(
        code, 0,
        "protected generate from a Final card failed with {stderr}"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert_eq!(
        baseline["mcp"]["server_name"].as_str(),
        Some("telorama"),
        "baseline was {baseline}"
    );
    assert_eq!(
        baseline["mcp"]["server_version"].as_str(),
        Some("0.1.0"),
        "baseline was {baseline}"
    );
    assert_eq!(
        baseline["mcp"]["protocol_version"].as_str(),
        Some("2025-06-18"),
        "baseline was {baseline}"
    );
    assert!(
        baseline["mcp"]["tools"]
            .as_object()
            .expect("tools")
            .is_empty(),
        "a Final card enumerates no tools, baseline was {baseline}"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog parses");
    let entry = &catalog["entries"][0];
    assert!(
        entry
            .get("capabilities")
            .and_then(|v| v.as_array())
            .is_none_or(|items| items.is_empty()),
        "the entry announces no capabilities from an unenumerating card, catalog was {catalog}"
    );

    // The declared check is clean and, above all, does not read
    // the card's silence about tools as a divergence.
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "clean declared check failed with {stdout}");
    assert!(
        stdout.contains("declared-evidence-only"),
        "report was {stdout}"
    );
    assert!(!stdout.contains("card-divergence"), "report was {stdout}");

    // The same silence against a baseline and a catalog that
    // DO record a tool, as a site generated while the card
    // still enumerated would hold: the unenumerated card must
    // still say nothing about tools. Read as an empty list,
    // it would diverge from both records at critical severity.
    let baseline_path = site.join("baseline.json");
    let mut baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline_path).expect("baseline"))
            .expect("baseline parses");
    baseline["mcp"]["tools"] = serde_json::json!({
        "render_scene": {"description_sha256": "x", "schema_sha256": "y"}
    });
    std::fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&baseline).expect("baseline text"),
    )
    .expect("edit baseline");
    for name in [".well-known/ard.json", ".well-known/ai-catalog.json"] {
        let path = site.join(name);
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("catalog"))
                .expect("catalog parses");
        catalog["entries"][0]["capabilities"] = serde_json::json!(["render_scene"]);
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&catalog).expect("catalog text"),
        )
        .expect("edit catalog");
    }
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(
        code, 0,
        "an unenumerated card must not diverge on tools, report was {stdout}"
    );
    assert!(!stdout.contains("card-divergence"), "report was {stdout}");

    // Identity is still compared: a card that changes its
    // version diverges.
    let card_path = site.join(".well-known/mcp/server-card.json");
    let mut card: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&card_path).expect("card")).expect("card parses");
    card["version"] = serde_json::json!("0.0.9");
    std::fs::write(
        &card_path,
        serde_json::to_string_pretty(&card).expect("card text"),
    )
    .expect("tamper card");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "identity change must fail, report was {stdout}");
    assert!(stdout.contains("card-divergence"), "report was {stdout}");
}

#[test]
fn validate_clean_then_broken() {
    let site = temp_site("validate");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &clean);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // A generated catalog validates clean.
    let (code, stdout, _) = run(&["validate", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "validate of a generated site failed with {stdout}");

    let edit_pair = |site: &Path, edit: &dyn Fn(&mut serde_json::Value)| {
        for path in [".well-known/ard.json", ".well-known/ai-catalog.json"] {
            let full = site.join(path);
            let mut catalog: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&full).expect("catalog")).expect("parses");
            edit(&mut catalog);
            std::fs::write(
                &full,
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&catalog).expect("serialize")
                ),
            )
            .expect("write catalog");
        }
    };

    // An entry anchored to another publisher's domain fails.
    edit_pair(&site, &|catalog| {
        catalog["entries"][0]["identifier"] = serde_json::json!("urn:air:other.example:mcp:demo");
    });
    let (code, stdout, _) = run(&["validate", "--site", &site_arg(&site)]);
    assert_eq!(code, 1, "unanchored entry must fail, report was {stdout}");
    assert!(stdout.contains("urn-not-anchored"), "report was {stdout}");

    // A warning alone does not fail validate.
    let (code, _, _) = generate_stdio(&site, &clean);
    assert_eq!(code, 0);
    edit_pair(&site, &|catalog| {
        catalog["entries"][0]["representativeQueries"] = serde_json::json!(["only one query"]);
    });
    let (code, stdout, _) = run(&["validate", "--site", &site_arg(&site)]);
    assert_eq!(
        code, 0,
        "a warning must not fail validate, report was {stdout}"
    );
    assert!(
        stdout.contains("representative-queries"),
        "report was {stdout}"
    );

    // Single file mode reads one catalog on its own.
    let ard = site.join(".well-known/ard.json");
    let (code, _, _) = run(&["validate", "--file", &ard.to_string_lossy()]);
    assert_eq!(code, 0);

    // A catalog that does not parse is a could not run, exit 2.
    let garbage = temp_site("validate-garbage").join("catalog.json");
    std::fs::write(&garbage, "not json").expect("garbage");
    let (code, _, _) = run(&["validate", "--file", &garbage.to_string_lossy()]);
    assert_eq!(code, 2, "unparseable catalog must exit 2");
}

// ----------------------------------------------------------------
// Migrate. The legacy catalog is a claim, verified entry by
// entry. What verifies carries, what fails is reported.

const SEVEN_TOOLS: [&str; 7] = [
    "validate_scene",
    "inspect_traces",
    "render_scene",
    "session_run",
    "session_frames",
    "scene_panel_detail",
    "incident_scene",
];

fn legacy_site(name: &str, entries: serde_json::Value) -> PathBuf {
    let site = temp_site(name);
    let catalog = serde_json::json!({
        "host": {"displayName": "Example", "domain": "example.com"},
        "entries": entries
    });
    std::fs::create_dir_all(site.join(".well-known")).expect("well known");
    std::fs::write(
        site.join(".well-known/ai-catalog.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&catalog).expect("serialize")
        ),
    )
    .expect("legacy catalog");
    site
}

fn legacy_mcp_entry(capabilities: Vec<&str>) -> serde_json::Value {
    serde_json::json!({
        "identifier": "urn:air:example.com:mcp:telorama",
        "displayName": "telorama",
        "type": "application/mcp-server-card+json",
        "url": "https://example.com/.well-known/mcp/server-card.json",
        "description": "The telorama MCP server.",
        "capabilities": capabilities,
        "representativeQueries": ["What can telorama do", "Use telorama to complete a task"],
        "version": "0.1.0"
    })
}

fn legacy_skill_entry() -> serde_json::Value {
    serde_json::json!({
        "identifier": "urn:air:example.com:skill:demo-review",
        "displayName": "demo-review",
        "type": "text/markdown",
        "url": "https://example.com/skills/demo-review/SKILL.md",
        "description": "Reviews a demo.",
        "representativeQueries": ["Review this demo", "Review that demo"]
    })
}

fn write_demo_skill(site: &Path) {
    let path = site.join("skills/demo-review/SKILL.md");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(
        path,
        "---\nname: demo-review\ndescription: Reviews a demo.\n---\n\nSteps.\n",
    )
    .expect("skill");
}

fn migrate_stdio(site: &Path, server: &Path, ci: &Path) -> (i32, String, String) {
    run(&[
        "migrate",
        "--site",
        &site_arg(site),
        "--ci",
        &ci.to_string_lossy(),
        "--command",
        &server.to_string_lossy(),
    ])
}

#[test]
fn migrate_clean_legacy_site_then_check_passes() {
    let site = legacy_site(
        "mig-clean",
        serde_json::json!([legacy_mcp_entry(SEVEN_TOOLS.to_vec()), legacy_skill_entry()]),
    );
    write_demo_skill(&site);
    let server = fixture_binary("fixture_server");
    let ci = site.join("ci/almanac.yml");

    let (code, stdout, stderr) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 0, "migrate failed with {stdout} {stderr}");
    assert!(
        stdout.contains("urn:air:example.com:mcp:telorama"),
        "report was {stdout}"
    );
    assert!(site.join(".well-known/ard.json").exists());
    assert!(site.join("baseline.json").exists());
    assert!(ci.exists(), "CI wiring must be written");

    // The migrated site passes the gate in both modes.
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "repo check after migrate failed with {stdout}");
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--command",
        &server.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "live check after migrate failed with {stdout}");

    // A second migration refuses. The site is no longer legacy.
    let (code, _, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 2, "a second migrate must refuse");
}

#[test]
fn migrate_reconciles_a_dead_tool_and_reports_it() {
    let mut capabilities = SEVEN_TOOLS.to_vec();
    capabilities.push("vanished_tool");
    let site = legacy_site(
        "mig-dead-tool",
        serde_json::json!([legacy_mcp_entry(capabilities)]),
    );
    let server = fixture_binary("fixture_server");
    let ci = site.join("ci/almanac.yml");

    let (code, stdout, stderr) = migrate_stdio(&site, &server, &ci);
    assert_eq!(
        code, 1,
        "a false capability claim must exit 1, report was {stdout} {stderr}"
    );
    assert!(
        stdout.contains("vanished_tool"),
        "the dead tool must be named in the report, which was {stdout}"
    );
    assert!(
        stdout.contains("no longer serves"),
        "the claim failure must be stated, report was {stdout}"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("ard"))
            .expect("parses");
    let carried = catalog["entries"][0]["capabilities"]
        .as_array()
        .expect("capabilities");
    assert_eq!(carried.len(), 7, "the dead tool must not carry over");
    assert!(
        !carried.iter().any(|c| c == "vanished_tool"),
        "capabilities were {carried:?}"
    );

    // And the migrated site gates clean against the live surface.
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--command",
        &server.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "live check after migrate failed with {stdout}");
}

#[test]
fn migrate_reports_an_unverifiable_entry_and_still_lands() {
    let ghost = serde_json::json!({
        "identifier": "urn:air:example.com:openapi:ghost",
        "displayName": "ghost",
        "type": "application/openapi+json",
        "url": "https://example.com/openapi.json",
        "description": "An API that is not in the site.",
        "representativeQueries": ["Call the ghost API", "List ghost operations"]
    });
    let site = legacy_site(
        "mig-ghost",
        serde_json::json!([legacy_mcp_entry(SEVEN_TOOLS.to_vec()), ghost]),
    );
    let server = fixture_binary("fixture_server");
    let ci = site.join("ci/almanac.yml");

    let (code, stdout, stderr) = migrate_stdio(&site, &server, &ci);
    assert_eq!(
        code, 1,
        "a failed entry must exit 1, report was {stdout} {stderr}"
    );
    assert!(
        stdout.contains("urn:air:example.com:openapi:ghost"),
        "the failed entry must be named, report was {stdout}"
    );
    assert!(
        stdout.contains("not in the site"),
        "the reason must be named, report was {stdout}"
    );
    // The verified entry still landed as a pair.
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("ard"))
            .expect("parses");
    assert_eq!(catalog["entries"].as_array().expect("entries").len(), 1);

    // A site with nothing to migrate is a could not run.
    let empty = temp_site("mig-empty");
    let (code, _, _) = migrate_stdio(&empty, &server, &ci);
    assert_eq!(code, 2, "no legacy catalog must exit 2");
}

#[test]
fn migrate_fails_false_claims() {
    let server = fixture_binary("fixture_server");

    // A skill whose file no longer matches the announced digest.
    let mut skill = legacy_skill_entry();
    skill["digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let site = legacy_site("mig-digest", serde_json::json!([skill]));
    write_demo_skill(&site);
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 1, "digest mismatch must exit 1, report was {stdout}");
    assert!(
        stdout.contains("no longer matches the digest"),
        "report was {stdout}"
    );
    assert!(
        !site.join(".well-known/ard.json").exists(),
        "nothing verified, so nothing may be written"
    );

    // A skill file declaring a different name than its entry.
    let site = legacy_site("mig-name", serde_json::json!([legacy_skill_entry()]));
    let path = site.join("skills/demo-review/SKILL.md");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(
        path,
        "---\nname: other-skill\ndescription: Reviews a demo.\n---\n\nSteps.\n",
    )
    .expect("skill");
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 1, "name mismatch must exit 1, report was {stdout}");
    assert!(stdout.contains("other-skill"), "report was {stdout}");

    // An MCP entry naming a different server than the surface.
    let mut wrong = legacy_mcp_entry(SEVEN_TOOLS.to_vec());
    wrong["identifier"] = serde_json::json!("urn:air:example.com:mcp:payments-api");
    let site = legacy_site("mig-identity", serde_json::json!([wrong]));
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(
        code, 1,
        "identity mismatch must exit 1, report was {stdout}"
    );
    assert!(stdout.contains("calls itself"), "report was {stdout}");
    assert!(!site.join(".well-known/ard.json").exists());

    // A second MCP entry is reported, the first still migrates.
    let mut second = legacy_mcp_entry(SEVEN_TOOLS.to_vec());
    second["identifier"] = serde_json::json!("urn:air:example.com:mcp:telorama-two");
    let site = legacy_site(
        "mig-two-servers",
        serde_json::json!([legacy_mcp_entry(SEVEN_TOOLS.to_vec()), second]),
    );
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 1, "a second server must exit 1, report was {stdout}");
    assert!(
        stdout.contains("more than one MCP server"),
        "report was {stdout}"
    );
    assert!(site.join(".well-known/ard.json").exists());

    // A duplicated identifier and an inlined entry both fail,
    // each with its reason, beside an entry that verifies.
    let inlined = serde_json::json!({
        "identifier": "urn:air:example.com:notes:inline",
        "displayName": "inline",
        "type": "application/json",
        "data": {"hello": "world"},
        "description": "Inlined notes.",
        "representativeQueries": ["Show the notes", "Read the notes"]
    });
    let site = legacy_site(
        "mig-dup-inline",
        serde_json::json!([legacy_skill_entry(), legacy_skill_entry(), inlined]),
    );
    write_demo_skill(&site);
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 1, "report was {stdout}");
    assert!(stdout.contains("more than once"), "report was {stdout}");
    assert!(stdout.contains("inlined entry"), "report was {stdout}");

    // One malformed entry is reported, not a dead end.
    let malformed = serde_json::json!({
        "identifier": "urn:air:example.com:skill:broken",
        "displayName": "broken",
        "type": "text/markdown",
        "url": "https://example.com/skills/broken/SKILL.md"
    });
    let site = legacy_site(
        "mig-malformed",
        serde_json::json!([legacy_skill_entry(), malformed]),
    );
    write_demo_skill(&site);
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 1, "report was {stdout}");
    assert!(stdout.contains("does not parse"), "report was {stdout}");
    assert!(site.join(".well-known/ard.json").exists());
}

#[test]
fn migrate_probe_failure_still_judges_the_site() {
    let site = legacy_site(
        "mig-probe-fails",
        serde_json::json!([legacy_mcp_entry(SEVEN_TOOLS.to_vec()), legacy_skill_entry()]),
    );
    write_demo_skill(&site);
    let ci = site.join("ci/almanac.yml");

    // The server command does not exist. The server claim fails
    // with the probe's reason, the skill still verifies, and
    // the pair still lands for what verified.
    let (code, stdout, stderr) = run(&[
        "migrate",
        "--site",
        &site_arg(&site),
        "--ci",
        &ci.to_string_lossy(),
        "--command",
        "/nonexistent-server-binary",
    ]);
    assert_eq!(
        code, 1,
        "a failed probe must exit 1, report was {stdout} {stderr}"
    );
    assert!(stdout.contains("the probe failed"), "report was {stdout}");
    assert!(site.join(".well-known/ard.json").exists());

    // A server claim with no target named is a could not run.
    let site = legacy_site(
        "mig-no-target",
        serde_json::json!([legacy_mcp_entry(SEVEN_TOOLS.to_vec())]),
    );
    let ci = site.join("ci/almanac.yml");
    let (code, _, stderr) = run(&[
        "migrate",
        "--site",
        &site_arg(&site),
        "--ci",
        &ci.to_string_lossy(),
    ]);
    assert_eq!(code, 2, "no target must exit 2, stderr was {stderr}");

    // An existing CI file is never overwritten.
    let site = legacy_site("mig-ci-kept", serde_json::json!([legacy_skill_entry()]));
    write_demo_skill(&site);
    let ci = site.join("ci/almanac.yml");
    std::fs::create_dir_all(ci.parent().expect("parent")).expect("dirs");
    std::fs::write(&ci, "sentinel\n").expect("sentinel");
    let server = fixture_binary("fixture_server");
    let (code, stdout, _) = migrate_stdio(&site, &server, &ci);
    assert_eq!(code, 0, "migrate failed with {stdout}");
    assert_eq!(
        std::fs::read_to_string(&ci).expect("ci"),
        "sentinel\n",
        "the existing CI file must stand"
    );
    assert!(stdout.contains("did not overwrite"), "report was {stdout}");
}

#[test]
fn migrate_protected_surface_from_the_card() {
    let (url, mode) = start_http_fixture();
    *mode.lock().expect("mode") = "auth-clean".to_string();
    let site = legacy_site(
        "mig-protected",
        serde_json::json!([legacy_mcp_entry(SEVEN_TOOLS.to_vec())]),
    );
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, stderr) = run(&[
        "migrate",
        "--site",
        &site_arg(&site),
        "--ci",
        &ci.to_string_lossy(),
        "--url",
        &url,
    ]);
    assert_eq!(code, 0, "protected migrate failed with {stdout} {stderr}");
    assert!(
        stderr.contains("declared evidence only"),
        "stderr was {stderr}"
    );
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "declared check after migrate failed with {stdout}");
}

// ----------------------------------------------------------------
// Amendment C. The agents.json cross check. agents.json names
// MCP endpoints as a corroborating surface; disagreement with
// the catalog the site publishes is minor drift, at the default
// gate a warning, never a failure.

#[test]
fn agents_json_cross_check() {
    let (url, _mode) = start_http_fixture();
    let site = temp_site("agentsjson");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // No agents.json, the cross check does not run.
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "clean check failed with {stdout}");
    assert!(!stdout.contains("agents-json"), "report was {stdout}");

    // agents.json naming the endpoint the catalog announces
    // through its card, agreement.
    std::fs::write(
        site.join("agents.json"),
        format!("{{\"mcp\": [{{\"url\": \"{url}\", \"type\": \"streamable-http\"}}]}}"),
    )
    .expect("write agents.json");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "agreeing check failed with {stdout}");
    assert!(!stdout.contains("agents-json"), "report was {stdout}");

    // agents.json naming a different endpoint disagrees in both
    // directions, at minor severity, so the run still passes.
    std::fs::write(
        site.join("agents.json"),
        "{\"mcp\": [{\"url\": \"http://127.0.0.1:9/elsewhere\"}]}",
    )
    .expect("write agents.json");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(
        code, 0,
        "mismatch is minor and must pass, report was {stdout}"
    );
    assert_eq!(
        stdout.matches("agents-json-mismatch").count(),
        2,
        "both directions must be reported, report was {stdout}"
    );

    // An agents.json with no mcp block names no endpoints, so
    // the announced endpoint stands uncorroborated, one finding.
    std::fs::write(site.join("agents.json"), "{}").expect("write agents.json");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "report was {stdout}");
    assert_eq!(
        stdout.matches("agents-json-mismatch").count(),
        1,
        "exactly one direction can fire, report was {stdout}"
    );

    // A present but unreadable agents.json is reported as its
    // own finding, never silently compared against.
    std::fs::write(site.join("agents.json"), "not json").expect("write agents.json");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "report was {stdout}");
    assert!(
        stdout.contains("agents-json-unreadable"),
        "report was {stdout}"
    );
}

// ----------------------------------------------------------------
// The generation roadmap's first slice. An OpenAPI document is
// a generation input and a drift surface, compared operation
// by operation.

fn billing_doc() -> serde_json::Value {
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "Billing", "version": "1.0.0", "description": "Charges cards."},
        "paths": {
            "/charges": {
                "post": {
                    "operationId": "createCharge",
                    "description": "Create a charge.",
                    "parameters": [{"name": "key", "in": "header", "required": true}]
                },
                "get": {"operationId": "listCharges", "description": "List charges."}
            }
        }
    })
}

/// The billing document in YAML, carrying the two shapes
/// real YAML OpenAPI documents have and JSON ones cannot:
/// an unquoted numeric response key, and a shared
/// parameter written once as an anchor and aliased into
/// the second operation.
fn billing_yaml() -> &'static str {
    "openapi: 3.1.0\n\
     info:\n  title: Billing\n  version: 1.0.0\n  description: Charges cards.\n\
     paths:\n  /charges:\n    post:\n      operationId: createCharge\n      description: Create a charge.\n      parameters:\n        - &key\n          name: key\n          in: header\n          required: true\n      responses:\n        200:\n          description: Charged.\n    get:\n      operationId: listCharges\n      description: List charges.\n      parameters:\n        - *key\n"
}

fn write_doc(path: &Path, doc: &serde_json::Value) {
    std::fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(doc).expect("serialize")
        ),
    )
    .expect("write document");
}

fn generate_stdio_with_openapi(
    site: &Path,
    fixture: &Path,
    openapi: &Path,
) -> (i32, String, String) {
    run(&[
        "generate",
        "--site",
        &site_arg(site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--openapi",
        &openapi.to_string_lossy(),
        "--command",
        &fixture.to_string_lossy(),
    ])
}

#[test]
fn openapi_input_and_operation_drift() {
    let src = temp_site("openapi-src");
    let doc_path = src.join("billing.json");
    write_doc(&doc_path, &billing_doc());
    let site = temp_site("openapi");
    let clean = fixture_binary("fixture_server");

    // Generation announces the API beside the MCP server, and
    // the document is copied into the site.
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let site_doc = site.join("billing.json");
    assert!(site_doc.exists(), "the document must be copied in");
    let catalog_text = std::fs::read_to_string(site.join(".well-known/ard.json")).expect("catalog");
    assert!(
        catalog_text.contains("application/openapi+json"),
        "catalog was {catalog_text}"
    );
    assert!(
        catalog_text.contains("createCharge"),
        "catalog was {catalog_text}"
    );
    let baseline_text = std::fs::read_to_string(site.join("baseline.json")).expect("baseline");
    assert!(
        baseline_text.contains("\"openapi\""),
        "baseline was {baseline_text}"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    // A formatting only rewrite of the served document is not
    // drift. The comparison is operation by operation, never
    // byte by byte.
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&site_doc).expect("doc")).expect("parses");
    std::fs::write(&site_doc, serde_json::to_string(&doc).expect("serialize"))
        .expect("rewrite compact");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "formatting rewrite must pass, report was {stdout}");

    // An operation the document stops serving is major drift.
    let mut broken = billing_doc();
    broken["paths"]["/charges"]
        .as_object_mut()
        .expect("path item")
        .remove("get");
    write_doc(&site_doc, &broken);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "removed operation must fail, report was {stdout}");
    assert!(
        stdout.contains("openapi-operation-removed"),
        "report was {stdout}"
    );

    // An operation the catalog never announced is critical.
    let (code, _, _) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0);
    let mut broken = billing_doc();
    broken["paths"]["/refunds"] = serde_json::json!({
        "post": {"operationId": "createRefund", "description": "Refund a charge."}
    });
    write_doc(&site_doc, &broken);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(
        code, 1,
        "unannounced operation must fail, report was {stdout}"
    );
    assert!(
        stdout.contains("openapi-operation-unannounced"),
        "report was {stdout}"
    );

    // A new required parameter on a surviving operation is
    // major drift.
    let (code, _, _) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0);
    let mut broken = billing_doc();
    broken["paths"]["/charges"]["post"]["parameters"]
        .as_array_mut()
        .expect("params")
        .push(serde_json::json!({"name": "tenant", "in": "header", "required": true}));
    write_doc(&site_doc, &broken);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(
        code, 1,
        "new required parameter must fail, report was {stdout}"
    );
    assert!(
        stdout.contains("openapi-required-parameter-added"),
        "report was {stdout}"
    );

    // A description only change warns and passes.
    let (code, _, _) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0);
    let mut edited = billing_doc();
    edited["paths"]["/charges"]["get"]["description"] = serde_json::json!("List every charge.");
    write_doc(&site_doc, &edited);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "description change must pass, report was {stdout}");
    assert!(
        stdout.contains("openapi-operation-description-changed"),
        "report was {stdout}"
    );

    // A missing document is critical, with the reason named.
    let (code, _, _) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0);
    std::fs::remove_file(&site_doc).expect("remove document");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "missing document must fail, report was {stdout}");
    assert!(
        stdout.contains("openapi-document-unreadable"),
        "report was {stdout}"
    );

    // A catalog edited to announce an operation generation
    // never recorded is divergence, the phantom rule.
    let (code, _, _) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0);
    for path in [".well-known/ard.json", ".well-known/ai-catalog.json"] {
        let full = site.join(path);
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&full).expect("catalog")).expect("parses");
        let entry = catalog["entries"]
            .as_array_mut()
            .expect("entries")
            .iter_mut()
            .find(|e| e["type"] == "application/openapi+json")
            .expect("api entry");
        entry["capabilities"]
            .as_array_mut()
            .expect("capabilities")
            .push(serde_json::json!("deleteEverything"));
        std::fs::write(
            &full,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&catalog).expect("serialize")
            ),
        )
        .expect("write catalog");
    }
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "phantom operation must fail, report was {stdout}");
    assert!(
        stdout.contains("catalog-baseline-divergence"),
        "report was {stdout}"
    );

    // A document in neither format is refused at
    // generation with both reasons named, and a YAML
    // document that parses but is not an OpenAPI document
    // is refused by the reduction, exit 2 either way.
    let neither = src.join("api.yaml");
    std::fs::write(&neither, b"\x80\x81 not a document").expect("bytes");
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &neither);
    assert_eq!(
        code, 2,
        "an unreadable document must refuse, stderr was {stderr}"
    );
    assert!(
        stderr.contains("not JSON") && stderr.contains("not YAML"),
        "stderr was {stderr}"
    );
    let yaml = src.join("partial.yaml");
    std::fs::write(&yaml, "openapi: 3.1.0\ninfo:\n  title: Billing\n").expect("yaml");
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &yaml);
    assert_eq!(
        code, 2,
        "a partial document must refuse, stderr was {stderr}"
    );
    assert!(
        stderr.contains("cannot be announced"),
        "stderr was {stderr}"
    );
    let not_api = src.join("notes.json");
    write_doc(&not_api, &serde_json::json!({"hello": "world"}));
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &not_api);
    assert_eq!(
        code, 2,
        "a non OpenAPI document must refuse, stderr was {stderr}"
    );
    assert!(stderr.contains("openapi"), "stderr was {stderr}");
}

/// A one purpose HTTP server for the remote artifact
/// tests: it answers every request with the bytes it
/// currently holds, so a test can change what the remote
/// side serves between checks.
fn serve_bytes(bytes: Vec<u8>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let shared = std::sync::Arc::new(std::sync::Mutex::new(bytes));
    let held = shared.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let held = held.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let body = held.lock().expect("lock").clone();
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), shared)
}

fn check_fetch_remote(site: &Path) -> (i32, String, String) {
    run(&["check", "--site", &site_arg(site), "--fetch-remote"])
}

/// The (body, media type) pair serve_typed holds, in
/// the shape its caller flips between runs.
type HeldTyped = std::sync::Arc<std::sync::Mutex<(Vec<u8>, String)>>;

/// Serve bytes under a media type the test can change,
/// for the catalog media type observation. The held
/// value is the (body, media type) pair.
fn serve_typed(bytes: Vec<u8>, media_type: &str) -> (String, HeldTyped) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let shared = std::sync::Arc::new(std::sync::Mutex::new((bytes, media_type.to_string())));
    let held = shared.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let held = held.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let (body, media_type) = held.lock().expect("lock").clone();
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {media_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), shared)
}

#[test]
fn check_fetches_remote_artifacts() {
    // A document and a card the catalog announces at
    // remote URLs: offline check cannot read the document
    // and says so; a fetching run reads both where they
    // live, and drift on the remote side is caught the
    // same way local drift is.
    let clean = fixture_binary("fixture_server");
    let src = temp_site("remote-artifacts-src");
    let doc_path = src.join("billing.json");
    write_doc(&doc_path, &billing_doc());
    let site = temp_site("remote-artifacts");
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0, "generate failed with {stderr}");

    let doc_bytes = std::fs::read(&doc_path).expect("doc bytes");
    let (doc_base, doc_held) = serve_bytes(doc_bytes);
    let card_bytes =
        std::fs::read(site.join(".well-known/mcp/server-card.json")).expect("card bytes");
    let (card_base, card_held) = serve_bytes(card_bytes);

    // Repoint the document in the baseline and in both
    // catalog files, and the card artifact in the
    // baseline, at the remote URLs.
    let mut baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    assert_eq!(
        baseline["artifacts"].as_array().expect("artifacts").len(),
        1,
        "a stdio generate records exactly one artifact, the card copy"
    );
    baseline["openapi"][0]["url"] = serde_json::Value::String(format!("{doc_base}/billing.json"));
    baseline["artifacts"][0]["url"] =
        serde_json::Value::String(format!("{card_base}/server-card.json"));
    std::fs::write(
        site.join("baseline.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&baseline).expect("serialize")
        ),
    )
    .expect("write baseline");
    for name in ["ard.json", "ai-catalog.json"] {
        let path = site.join(".well-known").join(name);
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("catalog")).expect("catalog json");
        for entry in catalog["entries"].as_array_mut().expect("entries") {
            if entry["type"] == "application/openapi+json" {
                entry["url"] = serde_json::Value::String(format!("{doc_base}/billing.json"));
            }
        }
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&catalog).expect("serialize")
            ),
        )
        .expect("write catalog");
    }
    std::fs::remove_file(site.join("billing.json")).expect("remove local doc");

    // Offline, the document cannot be read and the run
    // says exactly that.
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 1, "offline check must fail, report was {stdout}");
    assert!(
        stdout.contains("openapi-document-unreadable")
            && stdout.contains("does not map into the site"),
        "report was {stdout}"
    );

    // Fetching, the same site is clean.
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(code, 0, "fetching check failed with {stdout}");

    // An operation removed on the remote side is drift.
    let mut shrunk = billing_doc();
    shrunk["paths"]["/charges"]
        .as_object_mut()
        .expect("paths")
        .remove("get");
    *doc_held.lock().expect("lock") = format!(
        "{}\n",
        serde_json::to_string_pretty(&shrunk).expect("serialize")
    )
    .into_bytes();
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(code, 1, "removed operation must fail, report was {stdout}");
    assert!(
        stdout.contains("openapi-operation-removed"),
        "report was {stdout}"
    );
    *doc_held.lock().expect("lock") = std::fs::read(&doc_path).expect("doc bytes restored");

    // A card changed on the remote side breaks its digest.
    *card_held.lock().expect("lock") = b"{\"tampered\":true}\n".to_vec();
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(code, 1, "tampered card must fail, report was {stdout}");
    assert!(stdout.contains("digest-mismatch"), "report was {stdout}");
    *card_held.lock().expect("lock") =
        std::fs::read(site.join(".well-known/mcp/server-card.json")).expect("card restored");

    // A document whose host is gone fails as unreadable
    // with the fetch's reason named, not as a missing
    // file: repoint it at a port nothing serves.
    let dead = std::net::TcpListener::bind("127.0.0.1:0").expect("bind dead");
    let dead_port = dead.local_addr().expect("addr").port();
    drop(dead);
    let mut baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    baseline["openapi"][0]["url"] =
        serde_json::Value::String(format!("http://127.0.0.1:{dead_port}/billing.json"));
    std::fs::write(
        site.join("baseline.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&baseline).expect("serialize")
        ),
    )
    .expect("write baseline");
    for name in ["ard.json", "ai-catalog.json"] {
        let path = site.join(".well-known").join(name);
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("catalog")).expect("catalog json");
        for entry in catalog["entries"].as_array_mut().expect("entries") {
            if entry["type"] == "application/openapi+json" {
                entry["url"] =
                    serde_json::Value::String(format!("http://127.0.0.1:{dead_port}/billing.json"));
            }
        }
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&catalog).expect("serialize")
            ),
        )
        .expect("write catalog");
    }
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(
        code, 1,
        "unreachable document must fail, report was {stdout}"
    );
    assert!(
        stdout.contains("openapi-document-unreadable") && stdout.contains("could not be fetched"),
        "report was {stdout}"
    );
}

#[test]
fn check_with_an_http_target_fetches_remote_artifacts() {
    // The second fetch trigger: a run with an HTTP target
    // fetches without the flag. The site is generated from
    // the HTTP fixture itself, so the live comparison is
    // against the surface the baseline recorded. The
    // served card file is then removed from the tree, so
    // a plain run resolves no target from the catalog and
    // stays in repo mode; the only difference the named
    // run makes is the target itself.
    let (url, _mode) = start_http_fixture();
    let src = temp_site("remote-target-src");
    let doc_path = src.join("billing.json");
    write_doc(&doc_path, &billing_doc());
    let site = temp_site("remote-target");
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--openapi",
        &doc_path.to_string_lossy(),
        "--url",
        &url,
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");

    let doc_bytes = std::fs::read(&doc_path).expect("doc bytes");
    let (doc_base, _doc_held) = serve_bytes(doc_bytes);
    let card_bytes =
        std::fs::read(site.join(".well-known/mcp/server-card.json")).expect("card bytes");
    let (card_base, _card_held) = serve_bytes(card_bytes);
    let mut baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    assert_eq!(
        baseline["artifacts"].as_array().expect("artifacts").len(),
        1,
        "an off-origin endpoint records exactly one artifact, the card copy"
    );
    baseline["openapi"][0]["url"] = serde_json::Value::String(format!("{doc_base}/billing.json"));
    baseline["artifacts"][0]["url"] =
        serde_json::Value::String(format!("{card_base}/server-card.json"));
    std::fs::write(
        site.join("baseline.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&baseline).expect("serialize")
        ),
    )
    .expect("write baseline");
    for name in ["ard.json", "ai-catalog.json"] {
        let path = site.join(".well-known").join(name);
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("catalog")).expect("catalog json");
        for entry in catalog["entries"].as_array_mut().expect("entries") {
            if entry["type"] == "application/openapi+json" {
                entry["url"] = serde_json::Value::String(format!("{doc_base}/billing.json"));
            }
        }
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&catalog).expect("serialize")
            ),
        )
        .expect("write catalog");
    }

    // With the card file gone, a plain run resolves no
    // target and stays in repo mode, where the remote
    // document cannot be read.
    std::fs::remove_file(site.join(".well-known/mcp/server-card.json")).expect("remove card");
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 1, "repo mode check must fail, report was {stdout}");
    assert!(
        stdout.contains("openapi-document-unreadable"),
        "report was {stdout}"
    );

    // The targeted run fetches both and is clean.
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site), "--url", &url]);
    assert_eq!(code, 0, "targeted check failed with {stdout}");
}

#[test]
fn catalog_served_with_a_generic_media_type_warns() {
    // A fetching run observes the media type the
    // catalog itself is served with. The site's domain
    // is the serving address's authority, so the
    // observation reads what this server answers; the
    // https attempt cannot be made against a plain
    // HTTP server, and the run falls back to http.
    let clean = fixture_binary("fixture_server");
    let catalog_bytes = b"{}".to_vec();
    let (base, held) = serve_typed(catalog_bytes, "application/json");
    let authority = base.trim_start_matches("http://").to_string();
    let site = temp_site("media-type");
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        &authority,
        "--name",
        "Example",
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // Repo mode observes nothing and reports nothing
    // for the class.
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "repo mode check failed with {stdout}");
    assert!(
        !stdout.contains("catalog-media-type"),
        "report was {stdout}"
    );

    // A fetching run observes the generic type: one
    // Minor, and the run still passes at Major.
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(
        code, 0,
        "a Minor must not fail the run, report was {stdout}"
    );
    assert!(
        stdout.contains("catalog-media-type") && stdout.contains("application/json"),
        "report was {stdout}"
    );

    // Served under its own media type, the observation
    // is clean.
    held.lock().expect("lock").1 = "application/ard+json".to_string();
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(code, 0, "check failed with {stdout}");
    assert!(
        !stdout.contains("catalog-media-type"),
        "report was {stdout}"
    );
}

#[test]
fn card_display_changes_warn_instead_of_failing() {
    // A card edited only in its display fields warns at
    // Minor and passes; a card edited in substance fails
    // the digest at Critical, as it always has.
    let clean = fixture_binary("fixture_server");
    let site = temp_site("card-display");
    let (code, _, stderr) = generate_stdio(&site, &clean);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let baseline_text = std::fs::read_to_string(site.join("baseline.json")).expect("baseline");
    assert!(
        baseline_text.contains("\"card_display\""),
        "the baseline records the card's display fields"
    );
    let card_path = site.join(".well-known/mcp/server-card.json");
    let mut card: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&card_path).expect("card")).expect("card json");
    card["description"] = serde_json::Value::String("A better description.".into());
    std::fs::write(
        &card_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&card).expect("serialize")
        ),
    )
    .expect("write card");
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "a display change must pass, report was {stdout}");
    assert!(
        stdout.contains("declared-display-changed") && stdout.contains("description"),
        "report was {stdout}"
    );
    assert!(
        !stdout.contains("digest-mismatch"),
        "no digest finding may fire, report was {stdout}"
    );

    card["version"] = serde_json::Value::String("9.9.9".into());
    std::fs::write(
        &card_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&card).expect("serialize")
        ),
    )
    .expect("write card");
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(
        code, 1,
        "a substantive change must fail, report was {stdout}"
    );
    assert!(stdout.contains("digest-mismatch"), "report was {stdout}");
}

#[test]
fn yaml_openapi_documents_generate_and_drift() {
    // A YAML OpenAPI document generates exactly as its
    // JSON twin would: same entry, same capabilities,
    // and the drift pass reads the YAML surface back.
    let clean = fixture_binary("fixture_server");
    let src = temp_site("yaml-openapi-src");
    let yaml = src.join("billing.yaml");
    std::fs::write(&yaml, billing_yaml()).expect("yaml written");
    let site = temp_site("yaml-openapi");
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &yaml);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog json");
    let api = catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["type"] == "application/openapi+json")
        .expect("api entry");
    assert_eq!(api["identifier"], "urn:air:example.com:openapi:billing");
    assert_eq!(
        api["capabilities"],
        serde_json::json!(["createCharge", "listCharges"]),
        "catalog was {catalog}"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    // Removing an operation from the served YAML is
    // drift at Major, read back through the YAML parse.
    let without_get = billing_yaml().replace(
        "    get:\n      operationId: listCharges\n      description: List charges.\n      parameters:\n        - *key\n",
        "",
    );
    assert_ne!(without_get, billing_yaml(), "the get block must be cut");
    std::fs::write(site.join("billing.yaml"), without_get).expect("yaml rewritten");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "removed operation must fail, report was {stdout}");
    assert!(
        stdout.contains("openapi-operation-removed"),
        "report was {stdout}"
    );
}

#[test]
fn openapi_generate_variants() {
    let clean = fixture_binary("fixture_server");

    // Two documents in one run, the repeatable flag's whole
    // point, plus one document with no info description, which
    // generates with its title as the description and a
    // warning.
    let src = temp_site("openapi-variants-src");
    let billing = src.join("billing.json");
    write_doc(&billing, &billing_doc());
    let mut shipping_doc = billing_doc();
    shipping_doc["info"] = serde_json::json!({"title": "Shipping", "version": "0.3.0"});
    let shipping = src.join("shipping.json");
    write_doc(&shipping, &shipping_doc);
    let site = temp_site("openapi-variants");
    let (code, stdout, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--openapi",
        &billing.to_string_lossy(),
        "--openapi",
        &shipping.to_string_lossy(),
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w
                .as_str()
                .is_some_and(|w| w.contains("no info description"))),
        "report was {report}"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("parses");
    let apis: Vec<&serde_json::Value> = catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|e| e["type"] == "application/openapi+json")
        .collect();
    assert_eq!(apis.len(), 2, "catalog was {catalog}");
    let shipping_entry = apis
        .iter()
        .find(|e| e["identifier"] == "urn:air:example.com:openapi:shipping")
        .expect("shipping entry");
    assert_eq!(shipping_entry["description"], "Shipping");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    // A document already inside the site is used in place, at
    // its real served path, and no copy appears at the root.
    let site = temp_site("openapi-insite");
    let docs = site.join("docs");
    std::fs::create_dir_all(&docs).expect("docs dir");
    let inside = docs.join("api.json");
    write_doc(&inside, &billing_doc());
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &inside);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let catalog_text = std::fs::read_to_string(site.join(".well-known/ard.json")).expect("catalog");
    assert!(
        catalog_text.contains("https://example.com/docs/api.json"),
        "catalog was {catalog_text}"
    );
    assert!(
        !site.join("api.json").exists(),
        "an in place document must not be copied to the root"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    // Two documents with the same stem are refused before
    // anything is written, and a document named baseline.json
    // is refused for the name itself.
    let other = temp_site("openapi-dup-src");
    let dup = other.join("billing.json");
    write_doc(&dup, &billing_doc());
    let site = temp_site("openapi-dup");
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--openapi",
        &billing.to_string_lossy(),
        "--openapi",
        &dup.to_string_lossy(),
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 2, "duplicate stems must refuse, stderr was {stderr}");
    assert!(
        stderr.contains("two OpenAPI documents"),
        "stderr was {stderr}"
    );
    assert!(
        !site.join("billing.json").exists(),
        "a refused run writes no documents"
    );
    let named = other.join("baseline.json");
    write_doc(&named, &billing_doc());
    let (code, _, stderr) = generate_stdio_with_openapi(&site, &clean, &named);
    assert_eq!(
        code, 2,
        "a baseline.json document must refuse, stderr was {stderr}"
    );
    assert!(stderr.contains("baseline.json"), "stderr was {stderr}");
}

fn check_repo(site: &Path) -> (i32, String, String) {
    run(&["check", "--site", &site_arg(site)])
}

fn agent_card() -> serde_json::Value {
    serde_json::json!({
        "name": "Code Review Agent",
        "description": "Reviews code changes.",
        "version": "1.2.0",
        "supportedInterfaces": [
            {"url": "https://example.com/review", "protocolBinding": "JSONRPC", "protocolVersion": "1.0"}
        ],
        "capabilities": {"streaming": true},
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": [
            {"id": "review-diff", "name": "Review a diff", "description": "Reviews a diff.", "tags": ["code"], "examples": ["Review PR 12"]},
            {"id": "summarize-repo", "name": "Summarize", "description": "Summarizes a repo.", "tags": ["code"]}
        ]
    })
}

fn generate_stdio_with_agent_card(
    site: &Path,
    fixture: &Path,
    card: &Path,
) -> (i32, String, String) {
    run(&[
        "generate",
        "--site",
        &site_arg(site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--agent-card",
        &card.to_string_lossy(),
        "--command",
        &fixture.to_string_lossy(),
    ])
}

#[test]
fn a2a_input_and_skill_drift() {
    let src = temp_site("a2a-src");
    let card_path = src.join("review-agent.json");
    write_doc(&card_path, &agent_card());
    let site = temp_site("a2a");
    let clean = fixture_binary("fixture_server");

    // Generation announces the agent beside the MCP server,
    // the card is copied into the site, and the entry's
    // representative queries lead with the skill's own
    // example, the publisher's words.
    let (code, _, stderr) = generate_stdio_with_agent_card(&site, &clean, &card_path);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let site_card = site.join("review-agent.json");
    assert!(site_card.exists(), "the card must be copied in");
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog parses");
    let entry = catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["type"] == "application/a2a-agent-card+json")
        .expect("a2a entry");
    assert_eq!(
        entry["identifier"].as_str(),
        Some("urn:air:example.com:a2a:review-agent")
    );
    assert_eq!(
        entry["capabilities"],
        serde_json::json!(["review-diff", "summarize-repo"])
    );
    assert_eq!(
        entry["representativeQueries"][0].as_str(),
        Some("Review PR 12"),
        "entry was {entry}"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert_eq!(
        baseline["a2a"][0]["entry_id"].as_str(),
        Some("urn:air:example.com:a2a:review-agent"),
        "baseline was {baseline}"
    );
    assert_eq!(baseline["a2a"][0]["version"].as_str(), Some("1.2.0"));
    assert_eq!(
        baseline["a2a"][0]["skills"]
            .as_array()
            .expect("recorded skills")
            .len(),
        2
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    // A formatting only rewrite of the served card is not
    // drift. The comparison is fact by fact, never byte by
    // byte.
    let card: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&site_card).expect("card")).expect("parses");
    std::fs::write(&site_card, serde_json::to_string(&card).expect("serialize"))
        .expect("rewrite compact");
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "formatting rewrite must pass, report was {stdout}");

    // A skill the card stops serving is major drift.
    let mut broken = agent_card();
    broken["skills"]
        .as_array_mut()
        .expect("skills")
        .retain(|s| s["id"] != "summarize-repo");
    write_doc(&site_card, &broken);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "removed skill must fail, report was {stdout}");
    assert!(stdout.contains("a2a-skill-removed"), "report was {stdout}");

    // A skill the catalog never announced is critical.
    let (code, _, _) = generate_stdio_with_agent_card(&site, &clean, &card_path);
    assert_eq!(code, 0);
    let mut broken = agent_card();
    broken["skills"].as_array_mut().expect("skills").push(
        serde_json::json!({"id": "delete-repo", "name": "Delete", "description": "Deletes.", "tags": []}),
    );
    write_doc(&site_card, &broken);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "unannounced skill must fail, report was {stdout}");
    assert!(
        stdout.contains("a2a-skill-unannounced"),
        "report was {stdout}"
    );

    // An interface change is major drift.
    let (code, _, _) = generate_stdio_with_agent_card(&site, &clean, &card_path);
    assert_eq!(code, 0);
    let mut broken = agent_card();
    broken["supportedInterfaces"][0]["url"] = serde_json::json!("https://example.com/review-v2");
    write_doc(&site_card, &broken);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "interface change must fail, report was {stdout}");
    assert!(
        stdout.contains("a2a-interfaces-changed"),
        "report was {stdout}"
    );

    // A default modes change is major drift too.
    let (code, _, _) = generate_stdio_with_agent_card(&site, &clean, &card_path);
    assert_eq!(code, 0);
    let mut broken = agent_card();
    broken["defaultInputModes"] = serde_json::json!(["application/json"]);
    write_doc(&site_card, &broken);
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 1, "modes change must fail, report was {stdout}");
    assert!(stdout.contains("a2a-modes-changed"), "report was {stdout}");
}

#[test]
fn generate_refuses_an_openapi_and_agent_card_name_collision() {
    // An OpenAPI document and an agent card with the same
    // file name would land on the same site path. Planning
    // refuses the run before any write, so neither input
    // overwrites the other and the site is left untouched.
    let src = temp_site("collision-src");
    std::fs::create_dir_all(src.join("ext")).expect("ext dir");
    std::fs::create_dir_all(src.join("ext2")).expect("ext2 dir");
    write_doc(&src.join("ext/api.json"), &billing_doc());
    write_doc(&src.join("ext2/api.json"), &agent_card());
    let site = temp_site("collision");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--openapi",
        &src.join("ext/api.json").to_string_lossy(),
        "--agent-card",
        &src.join("ext2/api.json").to_string_lossy(),
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 2, "the collision must refuse, stderr was {stderr}");
    assert!(stderr.contains("already claims"), "stderr was {stderr}");
    assert!(
        !site.join("api.json").exists(),
        "no copy may land on a refused run"
    );
}

#[test]
fn repo_metadata_supplies_the_editorial_fallbacks() {
    // The site is a child of the repo root, which carries a
    // package.json. With no --name flag and no stronger
    // editorial source, the host takes its display name and
    // the MCP entry its description from the repo metadata,
    // each flagged low confidence in the run's notes.
    let root = temp_site("metadata-root");
    write_doc(
        &root.join("package.json"),
        &serde_json::json!({
            "name": "Parent Project",
            "description": "Words from the repo itself.",
            "version": "9.9.9"
        }),
    );
    let site = root.join("public");
    std::fs::create_dir_all(&site).expect("site dir");
    let clean = fixture_binary("fixture_server");
    let (code, stdout, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|n| n.as_str().is_some_and(|n| n.contains("lowest confidence"))),
        "report was {report}"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog parses");
    assert_eq!(
        catalog["host"]["displayName"].as_str(),
        Some("Parent Project"),
        "catalog was {catalog}"
    );
    assert_eq!(
        catalog["entries"][0]["description"].as_str(),
        Some("Words from the repo itself."),
        "catalog was {catalog}"
    );
    // The manifest's version is not read: the entry keeps
    // the probed server's own version.
    assert_eq!(
        catalog["entries"][0]["version"].as_str(),
        Some("0.1.0"),
        "catalog was {catalog}"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");
}

#[test]
fn repo_metadata_reads_a_cargo_manifest() {
    // The TOML path end to end: the parent directory carries
    // a Cargo.toml, and its package words fill the host name
    // and the MCP entry description through the toml crate's
    // conversion, not only through the core reduction.
    let root = temp_site("metadata-cargo-root");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"cargo-project\"\ndescription = \"Words from the crate.\"\nversion = \"3.2.1\"\n",
    )
    .expect("manifest written");
    let site = root.join("public");
    std::fs::create_dir_all(&site).expect("site dir");
    let clean = fixture_binary("fixture_server");
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog parses");
    assert_eq!(
        catalog["host"]["displayName"].as_str(),
        Some("cargo-project"),
        "catalog was {catalog}"
    );
    assert_eq!(
        catalog["entries"][0]["description"].as_str(),
        Some("Words from the crate."),
        "catalog was {catalog}"
    );
    // The generated server card carries the same words as
    // the entry, not the standing template.
    let card: serde_json::Value = serde_json::from_slice(
        &std::fs::read(site.join(".well-known/mcp/server-card.json")).expect("card"),
    )
    .expect("card parses");
    assert_eq!(
        card["description"].as_str(),
        Some("Words from the crate."),
        "card was {card}"
    );
}

#[test]
fn repo_metadata_fills_an_openapi_description() {
    // An OpenAPI document with no info description takes
    // the repo metadata description before the title
    // fallback, so the title warning never fires.
    let root = temp_site("metadata-openapi-root");
    write_doc(
        &root.join("package.json"),
        &serde_json::json!({"description": "The repo describes the API."}),
    );
    let site = root.join("public");
    std::fs::create_dir_all(&site).expect("site dir");
    let doc = serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "Billing", "version": "1.0.0"},
        "paths": {"/charges": {"get": {"operationId": "listCharges"}}}
    });
    write_doc(&site.join("billing.json"), &doc);
    let clean = fixture_binary("fixture_server");
    let (code, stdout, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--openapi",
        &site.join("billing.json").to_string_lossy(),
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .all(|w| !w
                .as_str()
                .is_some_and(|w| w.contains("carries no info description"))),
        "report was {report}"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog parses");
    let api = catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["type"] == "application/openapi+json")
        .expect("api entry");
    assert_eq!(
        api["description"].as_str(),
        Some("The repo describes the API."),
        "catalog was {catalog}"
    );
}

#[test]
fn publisher_config_overrides_editorial_fields() {
    // Rule 4's first source: the host name, the MCP entry's
    // display name, description, tags, and queries all come
    // from almanac.toml when it names them, the generated
    // card carries the configured title and description, a
    // document entry takes its configured description and
    // display name over its own document's words, and a
    // block for an entry the run does not generate is warned
    // on by name.
    let src = temp_site("config-overrides-src");
    let doc_path = src.join("billing.json");
    write_doc(&doc_path, &billing_doc());
    let site = temp_site("config-overrides");
    std::fs::write(
        site.join("almanac.toml"),
        "[host]\ndisplay_name = \"Configured Host\"\n\n[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\ndisplay_name = \"Configured Server\"\ndescription = \"Configured words.\"\ntags = [\"music\", \"booking\"]\nqueries = [\"Configured question one\", \"Configured question two\"]\n\n[[entries]]\nidentifier = \"urn:air:example.com:openapi:billing\"\ndisplay_name = \"Configured API\"\ndescription = \"Configured API words.\"\n\n[[entries]]\nidentifier = \"urn:air:example.com:mcp:absent\"\ndescription = \"Never applied.\"\n",
    )
    .expect("config written");
    let clean = fixture_binary("fixture_server");
    // No --name flag on this run: the flag outranks the
    // config file in rule 4's order, so the shared helper,
    // which passes one, would mask the behavior under test.
    let (code, stdout, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--openapi",
        &doc_path.to_string_lossy(),
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains("had no effect"))),
        "report was {report}"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog parses");
    assert_eq!(
        catalog["host"]["displayName"].as_str(),
        Some("Configured Host"),
        "catalog was {catalog}"
    );
    let entry = &catalog["entries"][0];
    assert_eq!(
        entry["displayName"].as_str(),
        Some("Configured Server"),
        "catalog was {catalog}"
    );
    assert_eq!(
        entry["description"].as_str(),
        Some("Configured words."),
        "catalog was {catalog}"
    );
    assert_eq!(
        entry["tags"],
        serde_json::json!(["music", "booking"]),
        "catalog was {catalog}"
    );
    assert_eq!(
        entry["representativeQueries"],
        serde_json::json!(["Configured question one", "Configured question two"]),
        "catalog was {catalog}"
    );
    let api = catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["type"] == "application/openapi+json")
        .expect("api entry");
    assert_eq!(
        api["displayName"].as_str(),
        Some("Configured API"),
        "catalog was {catalog}"
    );
    assert_eq!(
        api["description"].as_str(),
        Some("Configured API words."),
        "catalog was {catalog}"
    );
    let card: serde_json::Value = serde_json::from_slice(
        &std::fs::read(site.join(".well-known/mcp/server-card.json")).expect("card"),
    )
    .expect("card parses");
    assert_eq!(
        card["description"].as_str(),
        Some("Configured words."),
        "card was {card}"
    );
    assert_eq!(
        card["title"].as_str(),
        Some("Configured Server"),
        "card was {card}"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");
}

#[test]
fn publisher_config_suppresses_entries() {
    // Rule 1's recorded suppression: a suppressed entry is
    // not announced and leaves no baseline records, the
    // generation report records it with its reason, and the
    // gate stays clean.
    let src = temp_site("config-suppress-src");
    let doc_path = src.join("billing.json");
    write_doc(&doc_path, &billing_doc());
    let site = temp_site("config-suppress");
    std::fs::write(
        site.join("almanac.toml"),
        "[[entries]]\nidentifier = \"urn:air:example.com:openapi:billing\"\nsuppress = \"The billing API is internal only.\"\n",
    )
    .expect("config written");
    let clean = fixture_binary("fixture_server");
    let (code, stdout, stderr) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["suppressed"]
            .as_array()
            .expect("suppressed")
            .iter()
            .any(|s| s["entry"] == "urn:air:example.com:openapi:billing"
                && s["reason"] == "The billing API is internal only."),
        "report was {report}"
    );
    assert!(
        !site.join("billing.json").exists(),
        "a suppressed document must not be copied into the site"
    );
    let catalog_text = std::fs::read_to_string(site.join(".well-known/ard.json")).expect("catalog");
    assert!(
        !catalog_text.contains("application/openapi+json"),
        "catalog was {catalog_text}"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert_eq!(
        baseline["openapi"],
        serde_json::json!([]),
        "baseline was {baseline}"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    // Suppressing the MCP entry announces no server and
    // writes no server card.
    let site = temp_site("config-suppress-mcp");
    std::fs::write(
        site.join("almanac.toml"),
        "[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\nsuppress = \"The server is being renamed.\"\n",
    )
    .expect("config written");
    let (code, stdout, stderr) = generate_stdio_with_openapi(&site, &clean, &doc_path);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["suppressed"]
            .as_array()
            .expect("suppressed")
            .iter()
            .any(|s| s["entry"] == "urn:air:example.com:mcp:telorama"
                && s["reason"] == "The server is being renamed."),
        "report was {report}"
    );
    let catalog_text = std::fs::read_to_string(site.join(".well-known/ard.json")).expect("catalog");
    assert!(
        !catalog_text.contains("mcp-server-card"),
        "catalog was {catalog_text}"
    );
    assert!(
        !site.join(".well-known/mcp/server-card.json").exists(),
        "a suppressed server must not be published a card"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert!(baseline["mcp"].is_null(), "baseline was {baseline}");

    // Suppressing a skill entry leaves no served file
    // behind: check walks the site's skills directory, so a
    // SKILL.md with no baseline record would read as an
    // unannounced skill at Critical.
    let skill_dir = temp_site("config-suppress-skill-src");
    let skill_src = skill_dir.join("SKILL.md");
    std::fs::write(
        &skill_src,
        "---\nname: guide\ndescription: A guide skill.\n---\n\nGuide the thing.\n",
    )
    .expect("skill file");
    let site = temp_site("config-suppress-skill");
    std::fs::write(
        site.join("almanac.toml"),
        "[[entries]]\nidentifier = \"urn:air:example.com:skill:guide\"\nsuppress = \"The guide is withdrawn.\"\n",
    )
    .expect("config written");
    let (code, stdout, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--skill",
        &skill_src.to_string_lossy(),
        "--command",
        &clean.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["suppressed"]
            .as_array()
            .expect("suppressed")
            .iter()
            .any(|s| s["entry"] == "urn:air:example.com:skill:guide"
                && s["reason"] == "The guide is withdrawn."),
        "report was {report}"
    );
    assert!(
        !site.join("skills/guide/SKILL.md").exists(),
        "a suppressed skill must not be served from the site"
    );
    let (code, stdout, _) = check_stdio(&site, &clean);
    assert_eq!(code, 0, "clean check failed with {stdout}");
}

#[test]
fn publisher_config_errors_refuse_the_run() {
    // Each malformed editorial block is a configuration
    // error: the run does not start and no catalog is
    // written, because every one of these shapes can only
    // be a mistake about what the run would do.
    let clean = fixture_binary("fixture_server");
    for (name, config) in [
        (
            "duplicate",
            "[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\ndescription = \"One.\"\n\n[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\ndescription = \"Two.\"\n",
        ),
        (
            "reasonless",
            "[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\nsuppress = \"  \"\n",
        ),
        (
            "both",
            "[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\nsuppress = \"A reason.\"\ndescription = \"Also an override.\"\n",
        ),
        (
            "no-identifier",
            "[[entries]]\ndescription = \"No identifier at all.\"\n",
        ),
        (
            "empty-queries",
            "[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\nqueries = []\n",
        ),
    ] {
        let site = temp_site(&format!("config-error-{name}"));
        std::fs::write(site.join("almanac.toml"), config).expect("config written");
        let (code, _, stderr) = run(&[
            "generate",
            "--site",
            &site_arg(&site),
            "--domain",
            "example.com",
            "--command",
            &clean.to_string_lossy(),
        ]);
        assert_eq!(
            code, 2,
            "{name} config must refuse the run, stderr was {stderr}"
        );
        assert!(
            !site.join(".well-known/ard.json").exists(),
            "{name} config must not write a catalog"
        );
    }
}

#[test]
fn migrate_records_a2a_baselines_and_judges_claims() {
    let site = temp_site("mig-a2a");
    std::fs::create_dir_all(site.join(".well-known")).expect("well known");
    write_doc(&site.join("review-agent.json"), &agent_card());
    let entry = serde_json::json!({
        "identifier": "urn:air:example.com:a2a:review-agent",
        "displayName": "Code Review Agent",
        "type": "application/a2a-agent-card+json",
        "url": "https://example.com/review-agent.json",
        "description": "Reviews code changes.",
        "capabilities": ["review-diff", "summarize-repo", "invented-skill"],
        "representativeQueries": ["Review PR 12"],
        "version": "1.2.0"
    });
    let catalog = serde_json::json!({
        "host": {"displayName": "Example", "domain": "example.com"},
        "entries": [entry]
    });
    std::fs::write(
        site.join(".well-known/ai-catalog.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&catalog).expect("serialize")
        ),
    )
    .expect("legacy catalog");
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, stderr) = run(&[
        "migrate",
        "--site",
        &site_arg(&site),
        "--ci",
        &ci.to_string_lossy(),
    ]);
    // The invented skill is a false claim: the migration
    // reports it and exits 1, and the entry still carries,
    // reconciled to the card's real skills.
    assert_eq!(
        code, 1,
        "false claims must fail, report was {stdout} {stderr}"
    );
    assert!(stdout.contains("invented-skill"), "report was {stdout}");
    let migrated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog parses");
    assert_eq!(
        migrated["entries"][0]["capabilities"],
        serde_json::json!(["review-diff", "summarize-repo"]),
        "catalog was {migrated}"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert_eq!(
        baseline["a2a"][0]["version"].as_str(),
        Some("1.2.0"),
        "baseline was {baseline}"
    );
}

#[test]
fn migrate_records_openapi_baselines() {
    let clean = fixture_binary("fixture_server");
    let api_entry = |capabilities: Vec<&str>| {
        serde_json::json!({
            "identifier": "urn:air:example.com:openapi:billing",
            "displayName": "Billing",
            "type": "application/openapi+json",
            "url": "https://example.com/billing.json",
            "description": "Charges cards.",
            "capabilities": capabilities,
            "representativeQueries": ["What can the Billing API do", "Use the Billing API"],
            "version": "1.0.0"
        })
    };
    let site_with_doc = |name: &str, capabilities: Vec<&str>| {
        let site = legacy_site(
            name,
            serde_json::json!([
                legacy_mcp_entry(SEVEN_TOOLS.to_vec()),
                api_entry(capabilities)
            ]),
        );
        write_doc(&site.join("billing.json"), &billing_doc());
        site
    };

    // A truthful legacy entry migrates into an OpenAPI
    // baseline, and the migrated site checks clean, the same
    // gate a generated document lives under.
    let site = site_with_doc("migrate-openapi", vec!["createCharge", "listCharges"]);
    let ci = site.join("workflow.yml");
    let (code, stdout, _) = migrate_stdio(&site, &clean, &ci);
    assert_eq!(code, 0, "migrate failed with {stdout}");
    let baseline_text = std::fs::read_to_string(site.join("baseline.json")).expect("baseline");
    assert!(
        baseline_text.contains("\"openapi\""),
        "baseline was {baseline_text}"
    );
    let (code, stdout, _) = check_repo(&site);
    assert_eq!(code, 0, "migrated check failed with {stdout}");

    // A false capability claim is judged the way the MCP
    // entry's claims are judged: reported, exit 1, and the
    // entry carries reconciled to what the document serves.
    let site = site_with_doc(
        "migrate-openapi-lying",
        vec!["createCharge", "listCharges", "deleteEverything"],
    );
    let ci = site.join("workflow.yml");
    let (code, stdout, _) = migrate_stdio(&site, &clean, &ci);
    assert_eq!(code, 1, "a false claim must fail, report was {stdout}");
    assert!(stdout.contains("deleteEverything"), "report was {stdout}");
    let catalog_text = std::fs::read_to_string(site.join(".well-known/ard.json")).expect("catalog");
    assert!(
        !catalog_text.contains("deleteEverything"),
        "the carried entry is reconciled, catalog was {catalog_text}"
    );
    let (code, stdout, _) = check_repo(&site);
    assert_eq!(code, 0, "migrated check failed with {stdout}");
}

// ---------------------------------------------------------------------
// Applicability audit charge 1 (2026-11-23): the server card at both
// locations, the endpoint's /server-card path normative per SEP-2127
// Final and the well known path beside it.
// ---------------------------------------------------------------------

#[test]
fn card_published_at_both_locations() {
    let (url, _mode) = start_http_fixture();
    let site = temp_site("card-dual");
    // The site's domain is the fixture's own authority, so the
    // endpoint lives on the site's origin and the normative
    // copy belongs in the site.
    let authority = url
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .expect("authority")
        .to_string();
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        &authority,
        "--name",
        "Example",
        "--url",
        &url,
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let legacy_path = site.join(".well-known/mcp/server-card.json");
    let normative_path = site.join("mcp/server-card");
    let legacy = std::fs::read(&legacy_path).expect("legacy card");
    let normative = std::fs::read(&normative_path).expect("normative card");
    assert_eq!(legacy, normative, "the two card copies must be identical");
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    let artifacts = baseline["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 2, "both copies are baseline artifacts");
    let legacy_url = format!("https://{authority}/.well-known/mcp/server-card.json");
    assert!(
        artifacts
            .iter()
            .any(|a| a["url"].as_str() == Some(legacy_url.as_str())),
        "the well known copy is a baseline artifact"
    );
    let normative_url = format!("{url}/server-card");
    assert!(
        artifacts
            .iter()
            .any(|a| a["url"].as_str() == Some(normative_url.as_str())),
        "the normative copy is a baseline artifact"
    );
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "clean check failed with {stdout}");

    // Tampering with either copy alone is drift.
    std::fs::write(&normative_path, b"{}").expect("tamper normative");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "a tampered normative copy must fail, {stdout}");
    std::fs::write(&normative_path, &normative).expect("restore normative");
    std::fs::write(&legacy_path, b"{}").expect("tamper legacy");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 1, "a tampered legacy copy must fail, {stdout}");
}

#[test]
fn protected_generation_reads_the_normative_card() {
    let (url, mode) = start_http_fixture();
    *mode.lock().expect("mode") = "auth-card-normative".to_string();
    let site = temp_site("card-normative");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(
        code, 0,
        "protected generation must read the card at the normative location, {stderr}"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("ard"))
            .expect("ard json");
    assert_eq!(catalog["entries"][0]["displayName"], "telorama");
}

#[test]
fn protected_generation_prefers_the_normative_card() {
    let (url, mode) = start_http_fixture();
    *mode.lock().expect("mode") = "auth-card-both".to_string();
    let site = temp_site("card-both");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "protected generation failed with {stderr}");
    // The normative location serves version 0.1.0 and the
    // legacy path serves 0.0.9 in this mode, so the recorded
    // version says which card the fetch read.
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    assert_eq!(
        baseline["mcp"]["server_version"], "0.1.0",
        "the normative card must win when both locations serve"
    );
}

#[test]
fn migrate_publishes_the_card_at_both_locations() {
    let (url, _mode) = start_http_fixture();
    let site = temp_site("mig-card-dual");
    let authority = url
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .expect("authority")
        .to_string();
    let entry = serde_json::json!({
        "identifier": format!("urn:air:{authority}:mcp:telorama"),
        "displayName": "telorama",
        "type": "application/mcp-server-card+json",
        "url": format!("https://{authority}/.well-known/mcp/server-card.json"),
        "description": "The telorama MCP server.",
        "capabilities": SEVEN_TOOLS,
        "representativeQueries": ["What can telorama do", "Use telorama to complete a task"],
        "version": "0.1.0"
    });
    let catalog = serde_json::json!({
        "host": {"displayName": "Example", "domain": authority},
        "entries": [entry]
    });
    std::fs::create_dir_all(site.join(".well-known")).expect("well known");
    std::fs::write(
        site.join(".well-known/ai-catalog.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&catalog).expect("serialize")
        ),
    )
    .expect("legacy catalog");
    let ci = site.join("ci/almanac.yml");
    let (code, stdout, stderr) = run(&[
        "migrate",
        "--site",
        &site_arg(&site),
        "--ci",
        &ci.to_string_lossy(),
        "--url",
        &url,
    ]);
    assert_eq!(code, 0, "migrate failed with {stdout} {stderr}");
    let legacy =
        std::fs::read(site.join(".well-known/mcp/server-card.json")).expect("legacy card written");
    let normative = std::fs::read(site.join("mcp/server-card")).expect("normative card written");
    assert_eq!(legacy, normative, "the two card copies must be identical");
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    let artifacts = baseline["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 2, "both copies are baseline artifacts");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "check after migrate failed with {stdout}");
    // Tampering with the migrated normative copy alone fails.
    std::fs::write(site.join("mcp/server-card"), b"{}").expect("tamper normative");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(
        code, 1,
        "a tampered normative copy must fail, report was {stdout}"
    );
}

// ---------------------------------------------------------------------
// Applicability audit charge 2 (2026-11-23): the probe ladder is
// re-baselined to MCP core 2026-07-28. Discover first, stateless;
// the initialize handshake is the fallback for older servers.
// ---------------------------------------------------------------------

#[test]
fn http_discover_first_stateless() {
    let (url, mode, log) = start_http_fixture_logged();
    *mode.lock().expect("mode") = "open-2026".to_string();
    let site = temp_site("http2026");
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let seen = log.lock().expect("log").clone();
    assert_eq!(
        seen.first().map(String::as_str),
        Some("server/discover"),
        "discover must be the first request, saw {seen:?}"
    );
    assert!(
        !seen.iter().any(|m| m == "initialize"),
        "a current-revision surface must never see initialize, saw {seen:?}"
    );
    assert!(
        !seen.iter().any(|m| m == "notifications/initialized"),
        "the stateless walk sends no initialized notification, saw {seen:?}"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    assert_eq!(baseline["mcp"]["protocol_version"], "2026-07-28");
    let (code, stdout, _) = check_http(&site, &url);
    assert_eq!(code, 0, "clean check failed with {stdout}");
}

#[test]
fn stdio_discover_first_stateless() {
    let site = temp_site("stdio2026");
    let server = fixture_binary("fixture_server_2026");
    let (code, _, stderr) = generate_stdio(&site, &server);
    assert_eq!(
        code, 0,
        "generate against a discover-only server failed with {stderr}"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    assert_eq!(baseline["mcp"]["protocol_version"], "2026-07-28");
    let (code, stdout, _) = check_stdio(&site, &server);
    assert_eq!(code, 0, "clean check failed with {stdout}");
}

#[test]
fn generate_prints_the_full_report() {
    // The full generation report: one JSON document on
    // stdout naming the entries with their provenance,
    // the files written, and the run's notes, with
    // nothing scattered across stderr.
    let site = temp_site("generation-report");
    let skill_dir = temp_site("generation-report-skill");
    let skill_file = skill_dir.join("SKILL.md");
    std::fs::write(
        &skill_file,
        "---\nname: guide\ndescription: A guide skill.\n---\n\nGuide the thing.\n",
    )
    .expect("skill file");
    let clean = fixture_binary("fixture_server");
    let (code, stdout, stderr) = generate_stdio_with_skill(&site, &clean, &skill_file);
    assert_eq!(code, 0, "generate failed with {stderr}");
    assert!(stderr.is_empty(), "stderr was {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert_eq!(report["mode"], "generate", "report was {report}");
    assert_eq!(report["host"], "Example", "report was {report}");
    assert_eq!(report["domain"], "example.com", "report was {report}");
    let entries = report["entries"].as_array().expect("entries");
    assert!(
        entries
            .iter()
            .any(|e| e["entry"] == "urn:air:example.com:mcp:telorama"
                && e["type"] == "application/mcp-server-card+json"
                && e["provenance"] == "live-probe"),
        "report was {report}"
    );
    assert!(
        entries
            .iter()
            .any(|e| e["entry"] == "urn:air:example.com:skill:guide"
                && e["type"] == "text/markdown"
                && e["provenance"] == "skill-file"),
        "report was {report}"
    );
    let files: Vec<&str> = report["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f.as_str().expect("file name"))
        .collect();
    assert_eq!(
        files,
        [
            ".well-known/mcp/server-card.json",
            "skills/guide/SKILL.md",
            ".well-known/ard.json",
            ".well-known/ai-catalog.json",
            "baseline.json",
        ],
        "files must be exact and in write order, report was {report}"
    );
    assert_eq!(
        report["removed"],
        serde_json::json!([]),
        "report was {report}"
    );
    assert_eq!(
        report["suppressed"],
        serde_json::json!([]),
        "report was {report}"
    );
    assert!(
        report["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|n| n.as_str().is_some_and(|n| n.contains("legacy path"))),
        "report was {report}"
    );
}

#[test]
fn generate_rest_only_from_documents() {
    // A REST only site: no target at all, and the named
    // documents are the catalog. No MCP entry, no server
    // card, no MCP baseline, and check reads the result
    // clean.
    let site = temp_site("rest-only");
    // A config block naming an MCP entry this run cannot
    // generate is warned on by name, never matched and
    // never invented.
    std::fs::write(
        site.join("almanac.toml"),
        "[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\ndescription = \"No server here.\"\n",
    )
    .expect("config written");
    let docs = temp_site("rest-only-docs");
    let doc_path = docs.join("billing.json");
    write_doc(&doc_path, &billing_doc());
    let card_path = docs.join("agent.json");
    write_doc(&card_path, &agent_card());
    let (code, stdout, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--openapi",
        &doc_path.to_string_lossy(),
        "--agent-card",
        &card_path.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog json");
    let types: Vec<&str> = catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["type"].as_str().expect("type"))
        .collect();
    assert_eq!(
        types,
        [
            "application/openapi+json",
            "application/a2a-agent-card+json"
        ],
        "catalog was {catalog}"
    );
    assert!(
        !site.join(".well-known/mcp/server-card.json").exists(),
        "a REST only run writes no server card"
    );
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline json");
    assert_eq!(
        baseline["mcp"],
        serde_json::Value::Null,
        "baseline was {baseline}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|f| !f.as_str().expect("file").contains("server-card")),
        "report was {report}"
    );
    assert!(
        report["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w
                .as_str()
                .is_some_and(|w| w.contains("mcp:telorama") && w.contains("had no effect"))),
        "report was {report}"
    );
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "check of the REST only site failed with {stdout}");
}

#[test]
fn generate_rest_only_with_a_skill() {
    // The skill input alone, with no target, is a complete
    // REST only run: the refusal's skill arm accepts it, the
    // skill is announced and served, and check reads the
    // site clean. The --queries flag, which names the MCP
    // entry's queries, warns that it had no effect.
    let site = temp_site("rest-only-skill");
    let skill_dir = temp_site("rest-only-skill-src");
    let skill_file = skill_dir.join("SKILL.md");
    std::fs::write(
        &skill_file,
        "---\nname: guide\ndescription: A guide skill.\n---\n\nGuide the thing.\n",
    )
    .expect("skill file");
    let (code, stdout, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--skill",
        &skill_file.to_string_lossy(),
        "--queries",
        "Ask the guide",
    ]);
    assert_eq!(code, 0, "generate failed with {stderr}");
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join(".well-known/ard.json")).expect("catalog"))
            .expect("catalog json");
    assert_eq!(
        catalog["entries"][0]["type"], "text/markdown",
        "catalog was {catalog}"
    );
    assert!(site.join("skills/guide/SKILL.md").exists());
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains("--queries"))),
        "report was {report}"
    );
    let (code, stdout, _) = run(&["check", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "check of the skill only site failed with {stdout}");
}

#[test]
fn generate_with_nothing_named_refuses() {
    // No target and no inputs: an empty catalog is a
    // mistake rather than a publication, so the run
    // refuses and writes nothing.
    let site = temp_site("rest-only-empty");
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
    ]);
    assert_eq!(code, 2, "stderr was {stderr}");
    assert!(
        stderr.contains("no surface to probe and no inputs"),
        "stderr was {stderr}"
    );
    assert!(!site.join(".well-known/ard.json").exists());
}

#[test]
fn generate_report_collects_the_config_warnings() {
    // The config validator's own warnings (a query count
    // outside the draft's two to five, here) are generation
    // warnings like any other: they land in the report and
    // nothing prints to stderr.
    let site = temp_site("generation-report-config");
    std::fs::write(
        site.join("almanac.toml"),
        "[[entries]]\nidentifier = \"urn:air:example.com:mcp:telorama\"\nqueries = [\"Ask once\"]\n",
    )
    .expect("config written");
    let clean = fixture_binary("fixture_server");
    let (code, stdout, stderr) = generate_stdio(&site, &clean);
    assert_eq!(code, 0, "generate failed with {stderr}");
    assert!(stderr.is_empty(), "stderr was {stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("report parses");
    assert!(
        report["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w
                .as_str()
                .is_some_and(|w| w.contains("configures 1 representative queries"))),
        "report was {report}"
    );
}

#[test]
fn credentialed_probe_walks_a_protected_surface() {
    // The credential-aware probe. The fixture's auth-token
    // mode challenges every request that does not carry the
    // token and serves the walk to the one that does. A
    // credentialed generate records the live surface, tools
    // and all, where an unauthenticated one would record
    // the card. A credentialed check is a live pass. A
    // refused credential degrades to declared evidence with
    // the refusal named, and no credential is the declared
    // run it always was.
    let (url, mode) = start_http_fixture();
    *mode.lock().expect("mode") = "auth-token".to_string();
    let site = temp_site("credentialed");
    let (code, _, stderr) = run_with_env(
        &[
            "generate",
            "--site",
            &site_arg(&site),
            "--domain",
            "example.com",
            "--url",
            &url,
        ],
        &[("ALMANAC_TOKEN", "test-token")],
    );
    assert_eq!(code, 0, "credentialed generate failed with {stderr}");
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(site.join("baseline.json")).expect("baseline"))
            .expect("baseline parses");
    assert_eq!(
        baseline["mcp"]["tools"]
            .as_object()
            .expect("tools recorded")
            .len(),
        7,
        "the credentialed generate records the walked tools, baseline was {baseline}"
    );
    let provenance = baseline["provenance"]
        .as_array()
        .expect("provenance records")
        .iter()
        .find(|record| record["entry_id"] == "urn:air:example.com:mcp:telorama")
        .expect("the MCP entry's provenance");
    assert_eq!(
        provenance["produced_by"].as_str(),
        Some("live-probe"),
        "baseline was {baseline}"
    );

    // The credentialed check is a live pass, not declared
    // evidence.
    let (code, stdout, _) = run_with_env(
        &["check", "--site", &site_arg(&site), "--url", &url],
        &[("ALMANAC_TOKEN", "test-token")],
    );
    assert_eq!(code, 0, "credentialed check failed with {stdout}");
    assert!(stdout.contains("\"mode\": \"live\""), "report was {stdout}");

    // No credential: the declared run, unchanged.
    let (code, stdout, stderr) = run(&["check", "--site", &site_arg(&site), "--url", &url]);
    assert_eq!(code, 0, "declared check failed with {stdout}");
    assert!(
        stdout.contains("declared-evidence-only"),
        "report was {stdout}"
    );
    assert!(
        stderr.contains("was not probed live"),
        "stderr was {stderr}"
    );
    assert!(
        !stderr.contains("refused"),
        "a tokenless run must not report a refusal, stderr was {stderr}"
    );

    // A refused credential degrades the same way, with the
    // refusal named rather than silent.
    let (code, stdout, stderr) = run_with_env(
        &["check", "--site", &site_arg(&site), "--url", &url],
        &[("ALMANAC_TOKEN", "wrong-token")],
    );
    assert_eq!(code, 0, "refused check failed with {stdout}");
    assert!(
        stdout.contains("declared-evidence-only"),
        "report was {stdout}"
    );
    assert!(
        stderr.contains("the presented credential was refused"),
        "stderr was {stderr}"
    );
}

#[test]
fn validate_verifies_a_trust_manifest_signature_with_supplied_keys() {
    // The core suite's ES256 fixture, end to end through
    // the binary: the same JWK, the same signed manifest.
    let dir = temp_site("trust-validate");
    let digest = format!("sha256:{}", "a".repeat(64));
    let signature = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImRpZDp3ZWI6ZXhhbXBsZS5jb20ja2V5LTEifQ.._JNWqS8t8CWplUrJOvIPW_UiNQ_6pwTC8NTbZyEJEHivmugANeHTwTDP3UYw7SU6Yirz5DknxMsRln6MIdFdXw";
    let catalog = format!(
        r#"{{
  "host": {{"displayName": "Example", "domain": "example.com"}},
  "entries": [{{
    "identifier": "urn:air:example.com:mcp:demo",
    "displayName": "Demo",
    "type": "application/mcp-server-card+json",
    "url": "https://example.com/mcp",
    "description": "A demo server.",
    "representativeQueries": ["one", "two"],
    "digest": "{digest}",
    "trustManifest": {{
      "identity": "did:web:example.com",
      "issuedAt": "2026-11-20T00:00:00Z",
      "expiresAt": "2027-06-01",
      "subject": {{
        "identifier": "urn:air:example.com:mcp:demo",
        "type": "application/mcp-server-card+json",
        "digest": "{digest}"
      }},
      "signature": "{signature}"
    }}
  }}]
}}"#
    );
    let catalog_path = dir.join("catalog.json");
    std::fs::write(&catalog_path, &catalog).expect("write catalog");
    let keys_path = dir.join("keys.json");
    std::fs::write(
        &keys_path,
        r#"{"keys":[{"kty":"EC","crv":"P-256","kid":"did:web:example.com#key-1","x":"KFvtK60fJVnMb4stS4hJSzf4U37o4zyW2oy0fuOL_Ow","y":"C43rXNNfvh4McIabWEirso0xH2eDtSDCVfupwMHs7mE"}]}"#,
    )
    .expect("write keys");

    // With the key, the envelope verifies and validate
    // is clean.
    let (code, stdout, _) = run(&[
        "validate",
        "--file",
        &catalog_path.to_string_lossy(),
        "--trust-keys",
        &keys_path.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "signed validate failed with {stdout}");
    assert!(!stdout.contains("trust-manifest"), "report was {stdout}");

    // Without a key the signature reports unverified, a
    // warning, and validate still passes.
    let (code, stdout, _) = run(&["validate", "--file", &catalog_path.to_string_lossy()]);
    assert_eq!(code, 0, "keyless validate failed with {stdout}");
    assert!(
        stdout.contains("trust-manifest-signature-unverified"),
        "report was {stdout}"
    );

    // A tampered subject digest breaks both the binding
    // and the signature, and validate fails.
    let tampered = catalog.replace(
        &format!("\"digest\": \"{digest}\"\n      }},"),
        &format!("\"digest\": \"sha256:{}\"\n      }},", "b".repeat(64)),
    );
    assert_ne!(tampered, catalog, "the tamper must change the file");
    std::fs::write(&catalog_path, &tampered).expect("write tampered catalog");
    let (code, stdout, _) = run(&[
        "validate",
        "--file",
        &catalog_path.to_string_lossy(),
        "--trust-keys",
        &keys_path.to_string_lossy(),
    ]);
    assert_eq!(code, 1, "tampered validate passed with {stdout}");
    assert!(
        stdout.contains("trust-manifest-subject-mismatch"),
        "report was {stdout}"
    );
    assert!(
        stdout.contains("trust-manifest-signature-invalid"),
        "report was {stdout}"
    );
}

#[test]
fn check_judges_trust_manifests_and_remembers_a_removal() {
    // The fifteenth slice end to end. A manifest is
    // added to the generated catalog by hand, as a
    // publisher's signer would leave it. Check without
    // keys judges the envelope and is clean; check with
    // keys engages the signature layer and the planted
    // junk signature fails; regeneration carries the
    // envelope forward and the baseline records it; a
    // catalog stripped of the envelope then fails check
    // with the removal at critical.
    let site = temp_site("check-trust");
    let fixture = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &fixture);
    assert_eq!(code, 0, "generate failed: {stderr}");

    let ard_path = site.join(".well-known/ard.json");
    let legacy_path = site.join(".well-known/ai-catalog.json");
    let mut catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ard_path).expect("ard.json reads"))
            .expect("catalog parses");
    let entry = catalog["entries"]
        .as_array_mut()
        .expect("entries")
        .iter_mut()
        .find(|e| e["type"] == "application/mcp-server-card+json")
        .expect("the MCP entry");
    let identifier = entry["identifier"].as_str().unwrap().to_string();
    let digest = entry["digest"].as_str().unwrap().to_string();
    let junk_signature = format!(
        "eyJhbGciOiJFUzI1NiIsImtpZCI6ImRpZDp3ZWI6ZXhhbXBsZS5jb20ja2V5LTEifQ..{}",
        "A".repeat(86)
    );
    entry["trustManifest"] = serde_json::json!({
        "identity": "did:web:example.com",
        "expiresAt": "2028-06-01",
        "subject": {
            "identifier": identifier,
            "type": "application/mcp-server-card+json",
            "digest": digest,
        },
        "signature": junk_signature,
    });
    let bytes = serde_json::to_vec_pretty(&catalog).expect("catalog serializes");
    std::fs::write(&ard_path, &bytes).expect("ard.json written");
    std::fs::write(&legacy_path, &bytes).expect("legacy written");

    // No keys: the envelope is sound and no signature
    // claim is made in either direction.
    let (code, stdout, stderr) = check_stdio(&site, &fixture);
    assert_eq!(code, 0, "check failed: {stdout} {stderr}");
    assert!(!stdout.contains("trust-manifest"), "report was {stdout}");

    // Keys named: the signature layer engages and the
    // junk signature is invalid.
    let keys_path = site.join("keys.json");
    std::fs::write(
        &keys_path,
        r#"{"kty":"EC","crv":"P-256","kid":"did:web:example.com#key-1","x":"KFvtK60fJVnMb4stS4hJSzf4U37o4zyW2oy0fuOL_Ow","y":"C43rXNNfvh4McIabWEirso0xH2eDtSDCVfupwMHs7mE"}"#,
    )
    .expect("keys written");
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--trust-keys",
        &keys_path.to_string_lossy(),
        "--command",
        &fixture.to_string_lossy(),
    ]);
    assert_eq!(code, 1, "keyed check passed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-signature-invalid"),
        "report was {stdout}"
    );

    // Regeneration carries the envelope and the new
    // baseline records it.
    let (code, stdout, stderr) = generate_stdio(&site, &fixture);
    assert_eq!(code, 0, "regenerate failed: {stderr}");
    assert!(
        stdout.contains("carried from the catalog this run replaces"),
        "report was {stdout}"
    );
    let regenerated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ard_path).expect("ard.json reads"))
            .expect("catalog parses");
    assert!(
        regenerated["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.get("trustManifest").is_some()),
        "the envelope did not carry: {regenerated}"
    );
    let baseline_text =
        std::fs::read_to_string(site.join("baseline.json")).expect("baseline reads");
    assert!(
        baseline_text.contains("\"trust_manifests\""),
        "baseline was {baseline_text}"
    );

    // Stripped from both paths, the recorded envelope's
    // absence fails the gate at critical.
    let mut stripped = regenerated;
    for entry in stripped["entries"].as_array_mut().unwrap() {
        entry.as_object_mut().unwrap().remove("trustManifest");
    }
    let bytes = serde_json::to_vec_pretty(&stripped).expect("catalog serializes");
    std::fs::write(&ard_path, &bytes).expect("ard.json written");
    std::fs::write(&legacy_path, &bytes).expect("legacy written");
    let (code, stdout, _) = check_stdio(&site, &fixture);
    assert_eq!(code, 1, "stripped check passed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-removed"),
        "report was {stdout}"
    );
}

#[test]
fn check_verifies_attestation_pins_when_the_run_fetches() {
    // The sixteenth slice end to end. A bound
    // envelope is planted on the generated catalog,
    // as a publisher's signer would leave it, its
    // attestations pinning a document a local server
    // holds. Runs that do not fetch are silent about
    // the pin in both directions, before and after
    // the server swaps its bytes; an engaged run is
    // clean while the pinned bytes are served, and
    // the substitution fails at critical in check
    // and validate alike. A fetching check run
    // engages the pin layer without the flag.
    let site = temp_site("attestation-pins");
    let fixture = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &fixture);
    assert_eq!(code, 0, "generate failed: {stderr}");
    let document = b"the attestation document, as served".to_vec();
    let (att_uri, held) = serve_bytes(document.clone());
    let pin = almanac_core::hash::sha256_hex(&document);

    // Plant a bound envelope whose attestations pin
    // the served document, on both catalog paths.
    let ard_path = site.join(".well-known/ard.json");
    let legacy_path = site.join(".well-known/ai-catalog.json");
    let mut catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ard_path).expect("ard read")).expect("ard parses");
    let entry = catalog["entries"]
        .as_array_mut()
        .expect("entries")
        .iter_mut()
        .find(|e| e["type"] == "application/mcp-server-card+json")
        .expect("the MCP entry");
    let digest = entry["digest"]
        .as_str()
        .expect("the entry digest")
        .to_string();
    let id = entry["identifier"]
        .as_str()
        .expect("the entry id")
        .to_string();
    let junk = format!(
        "eyJhbGciOiJFUzI1NiIsImtpZCI6ImRpZDp3ZWI6ZXhhbXBsZS5jb20ja2V5LTEifQ..{}",
        "A".repeat(86)
    );
    entry["trustManifest"] = serde_json::json!({
        "identity": "did:web:example.com",
        "expiresAt": "2028-06-01",
        "subject": {
            "identifier": id,
            "type": "application/mcp-server-card+json",
            "digest": digest,
        },
        "attestations": [{
            "type": "https://slsa.dev/provenance/v1",
            "uri": att_uri,
            "digest": format!("sha256:{pin}"),
        }],
        "signature": junk,
    });
    let planted = serde_json::to_vec_pretty(&catalog).expect("serialize");
    std::fs::write(&ard_path, &planted).expect("ard planted");
    std::fs::write(&legacy_path, &planted).expect("legacy planted");

    // A run that does not fetch is silent about the
    // pin in both directions.
    let (code, stdout, _) = check_stdio(&site, &fixture);
    assert_eq!(code, 0, "unengaged check: {stdout}");
    assert!(!stdout.contains("attestation"), "report was {stdout}");

    // Validate without the flag likewise makes no pin
    // claim (its only trust finding is the signature
    // it cannot attempt without keys).
    let (code, stdout, _) = run(&["validate", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "unengaged validate: {stdout}");
    assert!(!stdout.contains("attestation"), "report was {stdout}");

    // Engaged, the served document matches its pin.
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--fetch-attestations",
        "--command",
        &fixture.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "pinned check: {stdout}");
    assert!(!stdout.contains("attestation"), "report was {stdout}");

    // The uri now serves another document: the
    // substitution the pin exists to catch, critical
    // in check and in validate alike.
    *held.lock().expect("lock") = b"a substituted document".to_vec();
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--fetch-attestations",
        "--command",
        &fixture.to_string_lossy(),
    ]);
    assert_eq!(code, 1, "substituted check passed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-attestation-mismatch"),
        "report was {stdout}"
    );
    let (code, stdout, _) = run(&[
        "validate",
        "--site",
        &site_arg(&site),
        "--fetch-attestations",
    ]);
    assert_eq!(code, 1, "substituted validate passed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-attestation-mismatch"),
        "report was {stdout}"
    );

    // The discriminating phases: with the substituted
    // bytes still served, the runs that do not fetch
    // stay silent and pass. An always engaged CLI
    // would fetch, mismatch, and fail here, which is
    // what makes the scoping rule falsifiable at this
    // level and not only in the core.
    let (code, stdout, _) = check_stdio(&site, &fixture);
    assert_eq!(code, 0, "unengaged check after substitution: {stdout}");
    assert!(!stdout.contains("attestation"), "report was {stdout}");
    let (code, stdout, _) = run(&["validate", "--site", &site_arg(&site)]);
    assert_eq!(code, 0, "unengaged validate after substitution: {stdout}");
    assert!(!stdout.contains("attestation"), "report was {stdout}");

    // A fetching check run engages the layer without
    // the flag and catches the same substitution.
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(code, 1, "fetching check passed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-attestation-mismatch"),
        "report was {stdout}"
    );

    // Generate refuses the flag outright, like the
    // other trust flags.
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--fetch-attestations",
        "--command",
        &fixture.to_string_lossy(),
    ]);
    assert_eq!(code, 2, "generate accepted the flag: {stderr}");
    assert!(
        stderr.contains("--fetch-attestations applies to validate and check only"),
        "stderr was {stderr}"
    );
}

#[test]
fn check_judges_cross_origin_descriptors_in_fetching_runs() {
    // The seventeenth slice end to end. The generated
    // entry is repointed at a descriptor served from
    // another host and given a bound envelope, as a
    // publisher announcing a CDN hosted card would
    // leave it. A fetching run retrieves the
    // descriptor: while it declares neither the entry
    // nor its signer the run fails at critical; once
    // it names the entry the class is silent; and a
    // run that does not fetch never judges the class
    // at all.
    let site = temp_site("cross-origin");
    let fixture = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &fixture);
    assert_eq!(code, 0, "generate failed: {stderr}");
    let plain = serde_json::json!({
        "name": "com.example/demo",
        "version": "0.1.0",
        "description": "A card that acknowledges no one.",
    });
    let (descriptor_url, held) = serve_bytes(serde_json::to_vec(&plain).expect("json"));
    let ard_path = site.join(".well-known/ard.json");
    let legacy_path = site.join(".well-known/ai-catalog.json");
    let mut planted_id = String::new();
    for path in [&ard_path, &legacy_path] {
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("catalog")).expect("json");
        let entry = &mut catalog["entries"][0];
        let id = entry["identifier"].as_str().expect("id").to_string();
        let digest = entry["digest"].as_str().expect("digest").to_string();
        let media_type = entry["type"].as_str().expect("type").to_string();
        planted_id = id.clone();
        entry["url"] = serde_json::Value::String(descriptor_url.clone());
        entry["trustManifest"] = serde_json::json!({
            "identity": "did:web:example.com",
            "issuedAt": "2026-11-25T00:00:00Z",
            "expiresAt": "2028-06-01T00:00:00Z",
            "subject": {
                "identifier": id,
                "type": media_type,
                "digest": digest,
            },
            "signature": "eyJhbGciOiJFUzI1NiIsImtpZCI6ImRpZDp3ZWI6ZXhhbXBsZS5jb20ja2V5LTEifQ..AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        });
        std::fs::write(path, serde_json::to_vec_pretty(&catalog).expect("json"))
            .expect("plant catalog");
    }

    // Fetching, the descriptor declares no one.
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_eq!(code, 1, "cross-origin check passed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-cross-origin-unbound"),
        "report was {stdout}"
    );

    // The descriptor names the entry: the class is
    // silent (other classes judge their own terms).
    let declaring = serde_json::json!({
        "name": "com.example/demo",
        "version": "0.1.0",
        "description": format!("The card for {planted_id}."),
    });
    *held.lock().expect("lock") = serde_json::to_vec(&declaring).expect("json");
    let (code, stdout, _) = check_fetch_remote(&site);
    assert_ne!(code, 2, "the declaring run could not run: {stdout}");
    assert!(
        !stdout.contains("trust-manifest-cross-origin-unbound"),
        "report was {stdout}"
    );

    // Undeclared again, but the run does not fetch:
    // the class makes no claim in either direction.
    *held.lock().expect("lock") = serde_json::to_vec(&plain).expect("json");
    let (code, stdout, _) = check_stdio(&site, &fixture);
    assert_ne!(code, 2, "the unengaged run could not run: {stdout}");
    assert!(
        !stdout.contains("trust-manifest-cross-origin-unbound"),
        "report was {stdout}"
    );
}

#[test]
fn check_judges_attestation_statement_subjects() {
    // The eighteenth slice end to end. A bound
    // envelope is planted on the generated catalog
    // whose attestation names a statement a local
    // server holds, with no pin: the statement layer
    // alone judges it. A statement binding by the
    // entry's digest is clean; the same envelope
    // pointing at a statement claiming another
    // subject is the transplant at critical; a
    // statement with no readable claim reports
    // unverified and passes; and a run that does not
    // fetch is silent throughout.
    let site = temp_site("attestation-statements");
    let fixture = fixture_binary("fixture_server");
    let (code, _, stderr) = generate_stdio(&site, &fixture);
    assert_eq!(code, 0, "generate failed: {stderr}");
    let binding = |hex: &str| {
        serde_json::json!({
            "_type": "https://in-toto.io/Statement/v1",
            "subject": [{ "name": "fixture", "digest": {"sha256": hex} }],
            "predicateType": "https://slsa.dev/provenance/v1",
        })
        .to_string()
        .into_bytes()
    };
    // A placeholder binds nothing; the entry's real
    // digest is read from the generated catalog
    // below and the served statement rebuilt around
    // it before the first run.
    let (statement_url, held) = serve_bytes(binding("placeholder"));
    let ard_path = site.join(".well-known/ard.json");
    let legacy_path = site.join(".well-known/ai-catalog.json");
    let mut catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ard_path).expect("ard read")).expect("ard parses");
    let entry = catalog["entries"]
        .as_array_mut()
        .expect("entries")
        .iter_mut()
        .find(|e| e["type"] == "application/mcp-server-card+json")
        .expect("the MCP entry");
    let digest = entry["digest"]
        .as_str()
        .expect("the entry digest")
        .to_string();
    let id = entry["identifier"]
        .as_str()
        .expect("the entry id")
        .to_string();
    let hex = digest
        .strip_prefix("sha256:")
        .expect("the entry digest is sha256")
        .to_string();
    *held.lock().expect("lock") = binding(&hex);
    let junk = format!(
        "eyJhbGciOiJFUzI1NiIsImtpZCI6ImRpZDp3ZWI6ZXhhbXBsZS5jb20ja2V5LTEifQ..{}",
        "A".repeat(86)
    );
    entry["trustManifest"] = serde_json::json!({
        "identity": "did:web:example.com",
        "expiresAt": "2028-06-01",
        "subject": {
            "identifier": id,
            "type": "application/mcp-server-card+json",
            "digest": digest,
        },
        "attestations": [{
            "type": "https://slsa.dev/provenance/v1",
            "uri": statement_url,
        }],
        "signature": junk,
    });
    let planted = serde_json::to_vec_pretty(&catalog).expect("serialize");
    std::fs::write(&ard_path, &planted).expect("ard planted");
    std::fs::write(&legacy_path, &planted).expect("legacy planted");

    // Engaged, the statement binds this entry.
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--fetch-attestations",
        "--command",
        &fixture.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "binding check: {stdout}");
    assert!(!stdout.contains("attestation"), "report was {stdout}");

    // The statement now claims another artifact: the
    // transplant, critical.
    *held.lock().expect("lock") = binding(&"b".repeat(64));
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--fetch-attestations",
        "--command",
        &fixture.to_string_lossy(),
    ]);
    assert_eq!(code, 1, "transplanted check passed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-attestation-unbound"),
        "report was {stdout}"
    );

    // A statement with no claim this build can read
    // reports unverified and does not fail the run.
    *held.lock().expect("lock") = b"a PDF compliance report, as served".to_vec();
    let (code, stdout, _) = run(&[
        "check",
        "--site",
        &site_arg(&site),
        "--fetch-attestations",
        "--command",
        &fixture.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "unreadable check failed: {stdout}");
    assert!(
        stdout.contains("trust-manifest-attestation-unverified"),
        "report was {stdout}"
    );
    assert!(
        !stdout.contains("trust-manifest-attestation-unbound"),
        "report was {stdout}"
    );

    // The transplant served again, but the run does
    // not fetch: the layer is silent.
    *held.lock().expect("lock") = binding(&"b".repeat(64));
    let (code, stdout, _) = check_stdio(&site, &fixture);
    assert_eq!(code, 0, "unengaged check: {stdout}");
    assert!(!stdout.contains("attestation"), "report was {stdout}");
}
