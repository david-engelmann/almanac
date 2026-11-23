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
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new(almanac())
        .args(args)
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
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let port = listener.local_addr().expect("addr").port();
    let mode: Mode = Arc::new(Mutex::new("open-clean".to_string()));
    let server_mode = mode.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mode = server_mode.clone();
            std::thread::spawn(move || {
                handle_http(&mut stream, &mode, port);
            });
        }
    });
    (format!("http://127.0.0.1:{port}/mcp"), mode)
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

fn handle_http(stream: &mut std::net::TcpStream, mode: &Mode, port: u16) {
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
            let card = serde_json::json!({
                "name": "telorama",
                "version": "0.1.0",
                "protocolVersion": "2025-06-18",
                "transport": "streamable-http",
                "description": "The telorama MCP server, from its card.",
                "tools": fixture_tools("open-clean"),
                "resources": [{"uri": "ui://telorama/scene"}],
                "prompts": []
            });
            respond(stream, 200, "application/json", "", &card.to_string());
        }
        ("POST", "/mcp") => {
            if mode.starts_with("auth-") {
                let header = format!(
                    "WWW-Authenticate: Bearer resource_metadata=\"{origin}/.well-known/oauth-protected-resource\"\r\n"
                );
                respond(stream, 401, "text/plain", &header, "unauthorized");
                return;
            }
            let request: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let method_name = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
            // Session strict. The fixture issues a session id at
            // initialize and refuses later calls that do not echo
            // it, so the suite catches a probe that stops echoing.
            if method_name != "initialize" && !headers.contains("mcp-session-id: fixture-session") {
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
    // critical divergence in declared evidence mode.
    let card_path = site.join(".well-known/mcp/server-card.json");
    let mut card: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&card_path).expect("card")).expect("card parses");
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
