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
    let (code, _, stderr) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--skill",
        &skill_file.to_string_lossy(),
        "--command",
        &clean.to_string_lossy(),
    ]);
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
    let skill_arg = skill_file.to_string_lossy().to_string();
    let (code, _, _) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--skill",
        &skill_arg,
        "--command",
        &clean.to_string_lossy(),
    ]);
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
    let (code, _, _) = run(&[
        "generate",
        "--site",
        &site_arg(&site),
        "--domain",
        "example.com",
        "--name",
        "Example",
        "--skill",
        &skill_arg,
        "--command",
        &clean.to_string_lossy(),
    ]);
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

fn read_request(stream: &mut std::net::TcpStream) -> (String, String, String) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            return (String::new(), String::new(), String::new());
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
    (method, path, String::from_utf8_lossy(&body).to_string())
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
    let (method, path, body) = read_request(stream);
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
            let doc = serde_json::json!({
                "issuer": origin,
                "authorization_endpoint": format!("{origin}/authorize"),
                "token_endpoint": format!("{origin}/token"),
                "jwks_uri": format!("{origin}/jwks"),
                "scopes_supported": scopes
            });
            respond(stream, 200, "application/json", "", &doc.to_string());
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
            let id = request.get("id").cloned();
            let Some(id) = id else {
                respond(stream, 202, "text/plain", "", "");
                return;
            };
            let payload = match method_name {
                "initialize" => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "serverInfo": {"name": "telorama", "version": "0.1.0"}
                    }
                }),
                "tools/list" => serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"tools": fixture_tools(&mode)}
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

#[test]
fn auth_ladder_and_metadata_breaks() {
    let (url, mode) = start_http_fixture();
    let origin = url.strip_suffix("/mcp").expect("origin").to_string();
    let site = temp_site("auth");

    // Baseline from the open surface first.
    let (code, _, stderr) = generate_http(&site, &url);
    assert_eq!(code, 0, "generate failed with {stderr}");

    // Patch the baseline's auth record from the fixture's real
    // metadata documents, through the core reduction.
    *mode.lock().expect("mode") = "auth-clean".to_string();
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
}
