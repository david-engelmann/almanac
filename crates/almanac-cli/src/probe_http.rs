//! The Streamable HTTP probe.
//!
//! Implements the probe sequence and the authenticated ladder from
//! docs/HTTP-TRANSPORT.md. One endpoint, JSON-RPC by POST, both
//! response framings read, the session id echoed when one is
//! issued, the protocol version header sent after initialization,
//! no GET stream opened, lists paginated to completion. A 401 is a
//! branch, not a dead end. It leads to the OAuth metadata ladder,
//! and the run degrades to declared evidence with the metadata
//! recorded for the drift engine.
//!
//! Redirects on the MCP endpoint are not followed. An announced
//! endpoint that forwards agents elsewhere is a surface change to
//! name in the failure record, not a detail to absorb silently.

use std::time::Duration;

use almanac_core::snapshot::{AuthSurface, MetadataRecord, ProbeFailure, SurfaceSnapshot};
use serde_json::{Value, json};
use ureq::Agent;

use crate::probe::{
    OFFERED_PROTOCOL_VERSION, failed_snapshot, is_method_not_found, prompt_names, resource_uris,
    tool_surfaces,
};
use crate::sse;

const TRANSPORT: &str = "streamable-http";

fn build_agent() -> Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

struct RpcOutcome {
    status: u16,
    session_id: Option<String>,
    www_authenticate: Option<String>,
    /// The JSON-RPC message answering this request, when one came.
    message: Option<Value>,
}

fn rpc(
    agent: &Agent,
    url: &str,
    session: Option<&str>,
    protocol_version: Option<&str>,
    id: Option<i64>,
    method: &str,
    params: Value,
) -> Result<RpcOutcome, String> {
    let mut payload = json!({"jsonrpc": "2.0", "method": method, "params": params});
    if let Some(id) = id {
        payload["id"] = json!(id);
    }
    let mut request = agent
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    if let Some(session) = session {
        request = request.header("Mcp-Session-Id", session);
    }
    if let Some(version) = protocol_version {
        request = request.header("MCP-Protocol-Version", version);
    }
    let mut response = request
        .send_json(&payload)
        .map_err(|e| format!("request to {url} failed with {e}"))?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let www_authenticate = headers
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = {
        use std::io::Read;
        let mut bytes = Vec::new();
        let _ = response
            .body_mut()
            .as_reader()
            .take(16 * 1024 * 1024)
            .read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).to_string()
    };
    let message = if status == 200 && id.is_some() {
        if content_type.contains("text/event-stream") {
            sse::messages(&body)
                .into_iter()
                .find(|m| m.get("id").and_then(|v| v.as_i64()) == id)
        } else {
            serde_json::from_str::<Value>(&body).ok()
        }
    } else {
        None
    };
    Ok(RpcOutcome {
        status,
        session_id,
        www_authenticate,
        message,
    })
}

fn get_json(agent: &Agent, url: &str) -> Option<Value> {
    let mut response = agent.get(url).call().ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    response.body_mut().read_json().ok()
}

fn origin_and_path(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let scheme = if url.starts_with("https://") {
        "https"
    } else {
        "http"
    };
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    Some((format!("{scheme}://{authority}"), path.to_string()))
}

/// The metadata ladder behind a 401. Fetches the protected
/// resource metadata and each authorization server's metadata, and
/// returns the protected snapshot the drift engine compares.
fn protected_snapshot(agent: &Agent, url: &str, www_authenticate: Option<&str>) -> SurfaceSnapshot {
    let mut prm_url = www_authenticate.and_then(|header| {
        header
            .split("resource_metadata=")
            .nth(1)
            .map(|rest| rest.trim().trim_matches(',').trim_matches('"').to_string())
    });
    if prm_url.is_none()
        && let Some((origin, path)) = origin_and_path(url)
    {
        let candidates = [
            format!("{origin}/.well-known/oauth-protected-resource{path}"),
            format!("{origin}/.well-known/oauth-protected-resource"),
        ];
        for candidate in candidates {
            if get_json(agent, &candidate).is_some() {
                prm_url = Some(candidate);
                break;
            }
        }
    }
    let mut resource_metadata = None;
    let mut authorization_servers = Vec::new();
    if let Some(prm_url) = &prm_url
        && let Some(document) = get_json(agent, prm_url)
    {
        let issuers: Vec<String> = document
            .get("authorization_servers")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        resource_metadata = Some(MetadataRecord::from_document(prm_url, &document));
        for issuer in issuers {
            let issuer = issuer.trim_end_matches('/');
            for suffix in [
                "/.well-known/oauth-authorization-server",
                "/.well-known/openid-configuration",
            ] {
                let meta_url = format!("{issuer}{suffix}");
                if let Some(meta) = get_json(agent, &meta_url) {
                    authorization_servers.push(MetadataRecord::from_document(&meta_url, &meta));
                    break;
                }
            }
        }
    }
    SurfaceSnapshot {
        transport: TRANSPORT.to_string(),
        server_name: String::new(),
        server_version: String::new(),
        protocol_version: String::new(),
        tools: vec![],
        resources: vec![],
        prompts: vec![],
        auth: AuthSurface::Protected(Box::new(almanac_core::snapshot::ProtectedMetadata {
            resource_metadata,
            authorization_servers,
        })),
        failure: None,
    }
}

/// Probe an MCP server over Streamable HTTP.
pub fn probe_http(url: &str) -> SurfaceSnapshot {
    let agent = build_agent();
    let init = match rpc(
        &agent,
        url,
        None,
        None,
        Some(1),
        "initialize",
        json!({
            "protocolVersion": OFFERED_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "almanac", "version": env!("CARGO_PKG_VERSION")}
        }),
    ) {
        Ok(outcome) => outcome,
        Err(detail) => return failed_snapshot(TRANSPORT, "initialize", detail),
    };
    if init.status == 401 {
        return protected_snapshot(&agent, url, init.www_authenticate.as_deref());
    }
    if init.status != 200 {
        return failed_snapshot(
            TRANSPORT,
            "initialize",
            format!("endpoint answered HTTP {}", init.status),
        );
    }
    let Some(init_message) = init.message else {
        return failed_snapshot(
            TRANSPORT,
            "initialize",
            "endpoint answered 200 without an initialize result".into(),
        );
    };
    if let Some(error) = init_message.get("error") {
        return failed_snapshot(
            TRANSPORT,
            "initialize",
            format!("server answered initialize with error {error}"),
        );
    }
    let result = init_message.get("result").cloned().unwrap_or(Value::Null);
    let server_info = result.get("serverInfo").cloned().unwrap_or(Value::Null);
    let negotiated = result
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let mut snapshot = SurfaceSnapshot {
        transport: TRANSPORT.to_string(),
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
        protocol_version: negotiated.clone(),
        tools: vec![],
        resources: vec![],
        prompts: vec![],
        auth: AuthSurface::Open,
        failure: None,
    };
    let session = init.session_id.as_deref();
    let version = Some(negotiated.as_str());
    let mut next_id = 2_i64;
    let _ = rpc(
        &agent,
        url,
        session,
        version,
        None,
        "notifications/initialized",
        json!({}),
    );

    for (method, apply) in [
        ("tools/list", 0),
        ("resources/list", 1),
        ("prompts/list", 2),
    ] {
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(cursor) => json!({"cursor": cursor}),
                None => json!({}),
            };
            let id = next_id;
            next_id += 1;
            let outcome = match rpc(&agent, url, session, version, Some(id), method, params) {
                Ok(outcome) => outcome,
                Err(detail) => {
                    snapshot.failure = Some(ProbeFailure {
                        step: method.into(),
                        detail,
                    });
                    return snapshot;
                }
            };
            if outcome.status == 401 {
                return protected_snapshot(&agent, url, outcome.www_authenticate.as_deref());
            }
            let Some(message) = outcome.message else {
                snapshot.failure = Some(ProbeFailure {
                    step: method.into(),
                    detail: format!("endpoint answered HTTP {} without a result", outcome.status),
                });
                return snapshot;
            };
            if is_method_not_found(&message) {
                break;
            }
            if let Some(error) = message.get("error") {
                snapshot.failure = Some(ProbeFailure {
                    step: method.into(),
                    detail: format!("server answered {method} with error {error}"),
                });
                return snapshot;
            }
            let page = message.get("result").cloned().unwrap_or(Value::Null);
            match apply {
                0 => snapshot.tools.extend(tool_surfaces(&page)),
                1 => snapshot.resources.extend(resource_uris(&page)),
                _ => snapshot.prompts.extend(prompt_names(&page)),
            }
            cursor = page
                .get("nextCursor")
                .and_then(|v| v.as_str())
                .map(String::from);
            if cursor.is_none() {
                break;
            }
        }
    }
    snapshot.resources.sort();
    snapshot.prompts.sort();

    if let Some(session) = session {
        let delete = agent
            .delete(url)
            .header("Mcp-Session-Id", session)
            .header("MCP-Protocol-Version", negotiated.as_str());
        let _ = delete.call();
    }
    snapshot
}
