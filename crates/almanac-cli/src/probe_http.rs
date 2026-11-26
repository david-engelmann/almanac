//! The Streamable HTTP probe.
//!
//! Implements the probe sequence and the authenticated ladder from
//! docs/HTTP-TRANSPORT.md. The walk enters through
//! server/discover, the current revision's entry point, and walks
//! the lists statelessly when the server answers it. A server that
//! does not know discover is walked through the initialize
//! handshake the earlier revisions use, with the session id echoed
//! when one is issued and the protocol version header sent after
//! initialization. One endpoint, JSON-RPC by POST, both response
//! framings read, no GET stream opened, lists paginated to
//! completion. A 401 at either entry is a branch, not a dead end.
//! It leads to the OAuth metadata ladder, and the run degrades to
//! declared evidence with the metadata recorded for the drift
//! engine. With a publisher credential in hand the walk retries
//! once with it, and a walked retry is live evidence whose auth
//! still records the declared posture.
//!
//! Redirects on the MCP endpoint are not followed. An announced
//! endpoint that forwards agents elsewhere is a surface change to
//! name in the failure record, not a detail to absorb silently.

use std::time::Duration;

use almanac_core::snapshot::{
    AuthSurface, CredentialOutcome, MetadataRecord, ProbeFailure, SurfaceSnapshot, contents_hash,
};
use serde_json::{Value, json};
use ureq::Agent;

use crate::probe::{
    CURRENT_PROTOCOL_VERSION, LIST_KINDS, MAX_PAGES, OFFERED_PROTOCOL_VERSION, discover_params,
    failed_snapshot, handshake_snapshot, is_method_not_found, list_params, request_meta,
};
use crate::sse;

const TRANSPORT: &str = "streamable-http";

/// The Authorization header value for a publisher token.
/// One construction, used by the rpc posts and by the
/// session DELETE courtesy, the endpoint's only requests.
fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

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
    /// The redirect target, when the answer was a redirect. The
    /// transport note asks for the target to be recorded, not
    /// absorbed, so the failure record can name where the
    /// endpoint tried to send the probe.
    location: Option<String>,
    /// The JSON-RPC message answering this request, when one came.
    message: Option<Value>,
}

/// One endpoint conversation: the URL every request
/// posts to, and the riders its requests carry. The
/// session and the version exist only on the legacy
/// ladder, the token only when the publisher supplied
/// one, and all four travel together through the entry
/// walk, the ladder, the list walk, and each request,
/// so they are one value rather than four arguments
/// repeated at every layer.
struct Conversation<'a> {
    url: &'a str,
    session: Option<&'a str>,
    version: Option<&'a str>,
    token: Option<&'a str>,
}

fn rpc(
    agent: &Agent,
    conversation: &Conversation,
    id: Option<i64>,
    method: &str,
    params: Value,
) -> Result<RpcOutcome, String> {
    let url = conversation.url;
    let mut payload = json!({"jsonrpc": "2.0", "method": method, "params": params});
    if let Some(id) = id {
        payload["id"] = json!(id);
    }
    let mut request = agent
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    // The credential rides only on requests to the endpoint
    // itself, which is the only place rpc posts. The metadata
    // ladder fetches its documents through get_bytes, never
    // through here, so the token cannot cross to the
    // authorization server's origin.
    if let Some(token) = conversation.token {
        request = request.header("Authorization", bearer(token));
    }
    if let Some(session) = conversation.session {
        request = request.header("Mcp-Session-Id", session);
    }
    if let Some(version) = conversation.version {
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
    let location = headers
        .get("location")
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
    // The message is parsed on any status, not only on 200,
    // because a JSON-RPC error body is data whatever HTTP
    // status carries it. Servers that refuse an unknown
    // method often answer 400 with the method not found error
    // in the body, and the probe's fallback decisions read
    // that error, so discarding it by status would blind them.
    let message = if id.is_some() {
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
        location,
        message,
    })
}

/// How a redirect walk ends, before the caller reads
/// anything from it. The walks in this module differ in
/// what they do with an answer and in how they name a
/// failure, but the hop machinery is one: request,
/// follow a redirect by hand, at most three, and, on a
/// guarded walk, refuse a literal non-public address
/// before every request.
enum FollowOutcome {
    /// A terminal answer: a success, or any status the
    /// walk does not follow.
    Response(ureq::http::Response<ureq::Body>),
    /// A guarded walk met an address, or a redirect
    /// target, naming a non-public literal.
    Refused,
    /// A request could not be made; the value is the
    /// transport error as the transport reported it,
    /// for the caller to name as it names its failures.
    TransportFailed(String),
    /// The chain went nowhere usable; the kind says
    /// which way, and the caller words it.
    DeadEnd(DeadEnd),
}

/// The ways a redirect chain goes nowhere usable.
enum DeadEnd {
    /// A redirect answer named no location, or one in
    /// bytes the checker will not read.
    NoLocation,
    /// The location named could not be resolved
    /// against the address that produced it.
    Unresolvable,
    /// The chain ran past the three redirects the
    /// walk follows.
    TooManyRedirects,
}

/// The module's one redirect walk. The guard is the
/// caller's choice because the postures differ on the
/// record: the fetcher's walks are guarded, and the
/// metadata walk is not.
fn follow(agent: &Agent, url: &str, guarded: bool) -> FollowOutcome {
    let mut current = url.to_string();
    for _ in 0..4 {
        if guarded && host_is_non_public(&current) {
            return FollowOutcome::Refused;
        }
        let response = match agent.get(&current).call() {
            Ok(response) => response,
            Err(e) => return FollowOutcome::TransportFailed(format!("{e}")),
        };
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok());
            match location {
                None => return FollowOutcome::DeadEnd(DeadEnd::NoLocation),
                Some(location) => match resolve_location(&current, location) {
                    Some(next) => {
                        current = next;
                        continue;
                    }
                    None => return FollowOutcome::DeadEnd(DeadEnd::Unresolvable),
                },
            }
        }
        return FollowOutcome::Response(response);
    }
    FollowOutcome::DeadEnd(DeadEnd::TooManyRedirects)
}

/// Fetch a document's bytes, following a short redirect chain
/// by hand. Metadata documents sit behind redirects often
/// enough that a document the agent would not follow reads as
/// absent, and absence is a fact the drift engine acts on, so
/// the ladder follows up to three.
fn get_bytes(agent: &Agent, url: &str) -> Option<Vec<u8>> {
    let FollowOutcome::Response(mut response) = follow(agent, url, false) else {
        return None;
    };
    if response.status().as_u16() != 200 {
        return None;
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(16 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

fn get_json(agent: &Agent, url: &str) -> Option<Value> {
    let bytes = get_bytes(agent, url)?;
    serde_json::from_slice(&bytes).ok()
}

/// Whether a URL's host is a literal non-public address:
/// link-local (the cloud metadata family lives there),
/// unique-local, or unspecified, in either family. Names
/// are not judged: a name that resolves privately is the
/// operator's own infrastructure question, and the
/// recorded posture (fetching is a live run's opt-in
/// behavior against the publisher's own baseline) is
/// what bounds that side.
fn host_is_non_public(url: &str) -> bool {
    use std::net::IpAddr;
    let Some(rest) = url.split("://").nth(1) else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let Ok(ip) = host.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => ipv4_is_non_public(&v4),
        IpAddr::V6(v6) => {
            // An IPv4-mapped literal (::ffff:a.b.c.d) is
            // judged as the IPv4 address it names, or the
            // metadata family would walk in through the
            // v6 door.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return ipv4_is_non_public(&v4);
            }
            v6.is_unspecified()
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

fn ipv4_is_non_public(v4: &std::net::Ipv4Addr) -> bool {
    v4.is_link_local() || v4.is_unspecified() || v4.octets()[0] == 0
}

/// The checker's fetcher for public documents the catalog
/// announces: one agent built once for a run's fetches,
/// against the module's build-once convention. Its walks
/// ride the module's one redirect walk, guarded, with
/// every failure named,
/// because the checker reports the reason, and with the
/// size cap enforced as a refusal rather than a silent
/// truncation. No credential is ever attached: announced
/// artifacts are public documents, and the publisher
/// token is scoped to the MCP endpoint. A URL (or a
/// redirect target) naming a literal non-public address
/// is refused before any request: the baseline is a
/// committed file, and a fetching run must not become a
/// way to make CI read a metadata endpoint.
pub(crate) struct DocumentFetcher {
    agent: Agent,
}

impl DocumentFetcher {
    pub(crate) fn new() -> Self {
        DocumentFetcher {
            agent: build_agent(),
        }
    }

    pub(crate) fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        use std::io::Read;
        const LIMIT: usize = 16 * 1024 * 1024;
        let mut response = match follow(&self.agent, url, true) {
            FollowOutcome::Response(response) => response,
            FollowOutcome::Refused => {
                return Err(
                    "the URL names a non-public address, which the checker will not fetch".into(),
                );
            }
            FollowOutcome::TransportFailed(reason) => {
                return Err(format!("the fetch failed with {reason}"));
            }
            FollowOutcome::DeadEnd(kind) => {
                return Err(match kind {
                    DeadEnd::NoLocation => "the redirect named no location",
                    DeadEnd::Unresolvable => "the redirect location could not be resolved",
                    DeadEnd::TooManyRedirects => "the fetch followed more than three redirects",
                }
                .into());
            }
        };
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("the server answered status {status}"));
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("the fetch failed with {e}"))?;
        if bytes.len() > LIMIT {
            return Err("the document is over the 16 MiB reading limit".into());
        }
        Ok(bytes)
    }

    /// Observe the media type a URL is served with, for
    /// the catalog media type check. The outcome keeps
    /// four cases apart, because the caller answers
    /// them differently: an observation, an answer that
    /// observed nothing, a request that could not be
    /// made at all (the one case the caller answers by
    /// trying the other scheme), and a refusal (an
    /// address, or a redirect target, naming a
    /// non-public literal), which is final. Redirects
    /// are followed as in fetch.
    pub(crate) fn fetch_media_type(&self, url: &str) -> MediaTypeOutcome {
        let response = match follow(&self.agent, url, true) {
            FollowOutcome::Response(response) => response,
            FollowOutcome::Refused => return MediaTypeOutcome::Refused,
            FollowOutcome::TransportFailed(_) => return MediaTypeOutcome::CouldNotBeMade,
            FollowOutcome::DeadEnd(_) => return MediaTypeOutcome::Unobserved,
        };
        if response.status().as_u16() != 200 {
            return MediaTypeOutcome::Unobserved;
        }
        match response.headers().get("content-type") {
            None => MediaTypeOutcome::Observed(String::new()),
            Some(value) => match value.to_str() {
                Ok(text) => MediaTypeOutcome::Observed(text.to_string()),
                // A header in bytes the checker will
                // not read is not an observation of no
                // type; it is no observation.
                Err(_) => MediaTypeOutcome::Unobserved,
            },
        }
    }

    /// Fetch a document's bytes at an https address,
    /// falling back to the same address over http only
    /// when the https request could not be made at all
    /// (a transport failure), the media type
    /// observation's fallback posture with the outcomes
    /// kept apart as that method keeps them. Any other
    /// https outcome is final: an answer, a refusal, and
    /// a dead redirect chain all mean the https surface
    /// was the one to judge. None when neither attempt
    /// serves the document, the best-effort shape its
    /// one caller (the served catalog collection)
    /// records.
    pub(crate) fn fetch_with_http_fallback(&self, url: &str) -> Option<Vec<u8>> {
        let response = match follow(&self.agent, url, true) {
            FollowOutcome::Response(response) => response,
            FollowOutcome::TransportFailed(_) => {
                let http = url.replacen("https://", "http://", 1);
                match follow(&self.agent, &http, true) {
                    FollowOutcome::Response(response) => response,
                    _ => return None,
                }
            }
            _ => return None,
        };
        if response.status().as_u16() != 200 {
            return None;
        }
        let mut response = response;
        use std::io::Read;
        const LIMIT: usize = 16 * 1024 * 1024;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > LIMIT {
            return None;
        }
        Some(bytes)
    }
}

/// The four outcomes of a media type observation
/// request; see DocumentFetcher::fetch_media_type.
pub(crate) enum MediaTypeOutcome {
    /// A success answer; the value is its Content-Type,
    /// empty when the answer carried none.
    Observed(String),
    /// The server answered without a success, or with
    /// a redirect chain that went nowhere usable, or
    /// with a Content-Type in unreadable bytes.
    Unobserved,
    /// The request could not be made at all (a
    /// transport failure).
    CouldNotBeMade,
    /// The address, or a redirect target, names a
    /// non-public literal the checker will not fetch.
    Refused,
}

/// Fetch the server card a surface serves. SEP-2127 Final
/// puts the card at the endpoint's own path with /server-card
/// appended, so that location is tried first, and the well
/// known path on the endpoint's origin is the fallback,
/// because publishers who predate the Final text serve only
/// that. A normative answer counts only when it parses as a
/// JSON object, because a catch-all front end answering every
/// unknown path with a 200 page would otherwise shadow a
/// valid legacy card behind an unparseable body, the same
/// reason the metadata ladder treats an unparseable document
/// as absent. The legacy answer is returned as read, so a
/// broken legacy card surfaces as a parse failure at the
/// caller rather than as silence. Generation for a protected
/// surface reads the surface from this card, because the card
/// is the declared surface the publisher already serves to
/// clients.
pub fn fetch_server_card(endpoint_url: &str) -> Option<Vec<u8>> {
    let (origin, _) = origin_and_path(endpoint_url)?;
    let agent = build_agent();
    let normative = crate::probe::normative_card_url(endpoint_url);
    if let Some(bytes) = get_bytes(&agent, &normative)
        && serde_json::from_slice::<Value>(&bytes)
            .map(|value| value.is_object())
            .unwrap_or(false)
    {
        return Some(bytes);
    }
    get_bytes(
        &agent,
        &format!("{origin}/.well-known/mcp/server-card.json"),
    )
}

/// Resolve a redirect target against the URL that produced it.
/// Absolute targets pass through, root-relative targets resolve
/// against the origin, anything else is not followed.
fn resolve_location(base: &str, location: &str) -> Option<String> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Some(location.to_string());
    }
    let (origin, _) = origin_and_path(base)?;
    if location.starts_with('/') {
        Some(format!("{origin}{location}"))
    } else {
        None
    }
}

/// Extract one parameter from a WWW-Authenticate header. The
/// value may be quoted, and parameters may follow it, so the
/// parse reads the quoted string to its closing quote instead
/// of trimming the rest of the header away.
fn header_param(header: &str, name: &str) -> Option<String> {
    let idx = header.find(name)?;
    let rest = header[idx + name.len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted.find('"')?;
        Some(quoted[..end].to_string())
    } else {
        let end = rest.find(',').unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    }
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
    let mut prm_url = www_authenticate.and_then(|header| header_param(header, "resource_metadata"));
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
        stateless_shape: false,
        capabilities: None,
        capabilities_enumerated: false,
        tools: vec![],
        resources: vec![],
        resource_surfaces: vec![],
        resource_templates: vec![],
        template_surfaces: vec![],
        resource_contents: Default::default(),
        prompts: vec![],
        prompt_surfaces: vec![],
        auth: AuthSurface::Protected(Box::new(almanac_core::snapshot::ProtectedMetadata {
            resource_metadata,
            authorization_servers,
        })),
        credential: CredentialOutcome::NotPresented,
        failure: None,
    }
}

/// How a list walk ended. A protected answer mid walk hands
/// back the degraded snapshot the metadata ladder built and
/// the step the challenge arrived at, and the caller returns
/// it instead of the walked one.
enum ListWalk {
    Done,
    Protected(Box<SurfaceSnapshot>, &'static str),
}

/// Walk the four lists into the snapshot over HTTP, following
/// pagination to completion. Under the current revision the
/// requests are stateless, identity in the meta block, no
/// session and no version header. Under the earlier revisions
/// the session and the negotiated version header ride along.
/// Both shapes share this walk, because a partial list is a
/// wrong list under either.
fn walk_lists(
    agent: &Agent,
    conversation: &Conversation,
    snapshot: &mut SurfaceSnapshot,
    stateless: bool,
    next_id: &mut i64,
) -> ListWalk {
    let url = conversation.url;
    for kind in LIST_KINDS {
        let method = kind.method();
        let mut cursor: Option<String> = None;
        let mut terminated = false;
        for _page in 0..MAX_PAGES {
            let params = if stateless {
                list_params(cursor.as_deref())
            } else {
                match &cursor {
                    Some(cursor) => json!({"cursor": cursor}),
                    None => json!({}),
                }
            };
            let id = *next_id;
            *next_id += 1;
            let outcome = match rpc(agent, conversation, Some(id), method, params) {
                Ok(outcome) => outcome,
                Err(detail) => {
                    snapshot.failure = Some(ProbeFailure {
                        step: method.into(),
                        detail,
                    });
                    return ListWalk::Done;
                }
            };
            if outcome.status == 401 {
                return ListWalk::Protected(
                    Box::new(protected_snapshot(
                        agent,
                        url,
                        outcome.www_authenticate.as_deref(),
                    )),
                    method,
                );
            }
            let Some(message) = outcome.message else {
                snapshot.failure = Some(ProbeFailure {
                    step: method.into(),
                    detail: format!("endpoint answered HTTP {} without a result", outcome.status),
                });
                return ListWalk::Done;
            };
            if is_method_not_found(&message) {
                terminated = true;
                break;
            }
            if let Some(error) = message.get("error") {
                snapshot.failure = Some(ProbeFailure {
                    step: method.into(),
                    detail: format!("server answered {method} with error {error}"),
                });
                return ListWalk::Done;
            }
            let page = message.get("result").cloned().unwrap_or(Value::Null);
            kind.apply(snapshot, &page);
            match page.get("nextCursor").and_then(|v| v.as_str()) {
                Some(next) if Some(next) != cursor.as_deref() => {
                    cursor = Some(next.to_string());
                }
                Some(_) => {
                    snapshot.failure = Some(ProbeFailure {
                        step: method.into(),
                        detail: "the server repeated a pagination cursor".into(),
                    });
                    return ListWalk::Done;
                }
                None => {
                    terminated = true;
                    break;
                }
            }
        }
        if !terminated {
            snapshot.failure = Some(ProbeFailure {
                step: method.into(),
                detail: format!("pagination did not terminate after {MAX_PAGES} pages"),
            });
            return ListWalk::Done;
        }
    }
    snapshot.resources.sort();
    snapshot.resource_surfaces.sort_by(|a, b| a.uri.cmp(&b.uri));
    snapshot.resource_templates.sort();
    snapshot
        .template_surfaces
        .sort_by(|a, b| a.uri_template.cmp(&b.uri_template));
    snapshot.prompts.sort();
    snapshot.prompt_surfaces.sort_by(|a, b| a.name.cmp(&b.name));
    ListWalk::Done
}

/// How the read step ended, in the list walk's shape:
/// done, or protected at the named step, carrying the
/// declared snapshot built from the challenge.
enum ReadWalk {
    Done,
    Protected(Box<SurfaceSnapshot>, &'static str),
}

/// Read each announced resource's contents once, after
/// the list walk, recording its hash in the snapshot.
/// A failed or unhashable read records nothing for
/// that uri, the posture the stdio walk takes: one bad
/// resource never fails the probe. A 401 on a read is
/// the one answer not swallowed: the step returns the
/// declared snapshot built from that challenge, so the
/// caller's credentialed retry covers the reads like
/// every other step (the twenty-ninth slice).
/// Templates are never expanded into reads.
fn read_resource_contents_http(
    agent: &Agent,
    conversation: &Conversation,
    snapshot: &mut SurfaceSnapshot,
    stateless: bool,
    next_id: &mut i64,
) -> ReadWalk {
    if snapshot.failure.is_some() {
        return ReadWalk::Done;
    }
    let mut uris = snapshot.resources.clone();
    uris.dedup();
    for uri in uris {
        let params = if stateless {
            json!({"uri": uri, "_meta": request_meta()})
        } else {
            json!({"uri": uri})
        };
        let id = *next_id;
        *next_id += 1;
        let Ok(outcome) = rpc(agent, conversation, Some(id), "resources/read", params) else {
            continue;
        };
        if outcome.status == 401 {
            return ReadWalk::Protected(
                Box::new(protected_snapshot(
                    agent,
                    conversation.url,
                    outcome.www_authenticate.as_deref(),
                )),
                "resources/read",
            );
        }
        let Some(message) = outcome.message else {
            continue;
        };
        if is_method_not_found(&message) || message.get("error").is_some() {
            continue;
        }
        if let Some(result) = message.get("result")
            && let Some(hash) = contents_hash(result)
        {
            snapshot.resource_contents.insert(uri, hash);
        }
    }
    ReadWalk::Done
}

/// How one entry walk ended. A challenge is not a failure:
/// the endpoint answered 401 at the named step, and the
/// snapshot is the declared mode one the metadata ladder
/// built from that challenge. The caller decides whether a
/// credential retries the walk or the declared snapshot is
/// the run's answer.
enum EntryOutcome {
    Snapshot(SurfaceSnapshot),
    Challenged {
        step: &'static str,
        snapshot: SurfaceSnapshot,
    },
}

/// Probe an MCP server over Streamable HTTP. The walk runs
/// unauthenticated first. A challenge, with a publisher
/// credential in hand, retries the whole walk once with the
/// credential presented; a walked retry is a live pass whose
/// auth still records the declared posture the first
/// challenge's metadata ladder read, and a refused retry
/// returns the declared snapshot with the refusal named. No
/// credential, and the declared snapshot is the answer, as
/// before.
pub fn probe_http(url: &str, token: Option<&str>) -> SurfaceSnapshot {
    let agent = build_agent();
    match entry_walk(&agent, url, None) {
        EntryOutcome::Snapshot(snapshot) => snapshot,
        EntryOutcome::Challenged { snapshot, .. } => {
            let Some(token) = token else {
                return snapshot;
            };
            match entry_walk(&agent, url, Some(token)) {
                EntryOutcome::Snapshot(mut walked) => {
                    // A walk that failed partway is a failed
                    // probe, not an accepted credential: its
                    // lists are partial, and the failure
                    // branch of every consumer is what reads
                    // it. Only a completed walk earns the
                    // mark and the declared posture.
                    if walked.failure.is_none() {
                        walked.auth = snapshot.auth.clone();
                        walked.credential = CredentialOutcome::Accepted;
                    }
                    walked
                }
                EntryOutcome::Challenged { step, .. } => {
                    let mut refused = snapshot;
                    refused.credential = CredentialOutcome::Refused(step.to_string());
                    refused
                }
            }
        }
    }
}

/// One walk of the endpoint: discover first, the current
/// revision's entry point, and the initialize ladder behind
/// it for the servers that predate discover.
///
/// The fallback is evidence based, because the two entries
/// diagnose different servers. Only an answer that says the
/// method is unknown, a JSON-RPC method not found error or an
/// HTTP 404 or 405, proves the server predates discover, and
/// only that answer falls back. A server that claims the
/// current revision and breaks at discover (a 500, another
/// JSON-RPC error, a result that is not an object, a result
/// naming no server) fails at the server/discover step with
/// the break named, because falling back would manufacture
/// drift against whichever surface the legacy ladder happened
/// to show, and would let a broken discover hide behind a
/// healthy handshake indefinitely. A transport failure is a
/// discover failure too, for the same reason.
fn entry_walk(agent: &Agent, url: &str, token: Option<&str>) -> EntryOutcome {
    let conversation = Conversation {
        url,
        session: None,
        version: None,
        token,
    };
    let outcome = match rpc(
        agent,
        &conversation,
        Some(1),
        "server/discover",
        discover_params(),
    ) {
        Ok(outcome) => outcome,
        Err(detail) => {
            return EntryOutcome::Snapshot(failed_snapshot(TRANSPORT, "server/discover", detail));
        }
    };
    if outcome.status == 401 {
        return EntryOutcome::Challenged {
            step: "server/discover",
            snapshot: protected_snapshot(agent, url, outcome.www_authenticate.as_deref()),
        };
    }
    if outcome.message.as_ref().is_some_and(is_method_not_found)
        || matches!(outcome.status, 404 | 405)
    {
        return initialize_ladder(agent, url, token);
    }
    let result = outcome
        .message
        .as_ref()
        .and_then(|message| message.get("result"))
        .filter(|result| result.is_object());
    let Some(result) = result else {
        let detail = match &outcome.message {
            Some(message) => match message.get("error") {
                Some(error) => format!("server answered server/discover with error {error}"),
                None => "server answered server/discover without a usable result".to_string(),
            },
            None => format!("endpoint answered HTTP {} without a result", outcome.status),
        };
        return EntryOutcome::Snapshot(failed_snapshot(TRANSPORT, "server/discover", detail));
    };
    let mut snapshot = handshake_snapshot(TRANSPORT, result, true);
    if snapshot.server_name.is_empty() {
        return EntryOutcome::Snapshot(failed_snapshot(
            TRANSPORT,
            "server/discover",
            "the discover result named no server".into(),
        ));
    }
    if snapshot.protocol_version.is_empty() {
        snapshot.protocol_version = CURRENT_PROTOCOL_VERSION.into();
    }
    let mut next_id = 2_i64;
    if let ListWalk::Protected(protected, step) =
        walk_lists(agent, &conversation, &mut snapshot, true, &mut next_id)
    {
        return EntryOutcome::Challenged {
            step,
            snapshot: *protected,
        };
    }
    if let ReadWalk::Protected(protected, step) =
        read_resource_contents_http(agent, &conversation, &mut snapshot, true, &mut next_id)
    {
        return EntryOutcome::Challenged {
            step,
            snapshot: *protected,
        };
    }
    EntryOutcome::Snapshot(snapshot)
}

/// The earlier revisions' entry: an initialize handshake, the
/// initialized notification, the lists with the session echoed,
/// and the DELETE courtesy when the server issued a session.
fn initialize_ladder(agent: &Agent, url: &str, token: Option<&str>) -> EntryOutcome {
    let bare = Conversation {
        url,
        session: None,
        version: None,
        token,
    };
    let init = match rpc(
        agent,
        &bare,
        Some(1),
        "initialize",
        json!({
            "protocolVersion": OFFERED_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "almanac", "version": env!("CARGO_PKG_VERSION")}
        }),
    ) {
        Ok(outcome) => outcome,
        Err(detail) => {
            return EntryOutcome::Snapshot(failed_snapshot(TRANSPORT, "initialize", detail));
        }
    };
    if init.status == 401 {
        return EntryOutcome::Challenged {
            step: "initialize",
            snapshot: protected_snapshot(agent, url, init.www_authenticate.as_deref()),
        };
    }
    if init.status != 200 {
        let detail = match (&init.location, init.status) {
            (Some(location), status) if (300..400).contains(&status) => {
                format!("endpoint answered HTTP {status} redirect to {location}")
            }
            _ => format!("endpoint answered HTTP {}", init.status),
        };
        return EntryOutcome::Snapshot(failed_snapshot(TRANSPORT, "initialize", detail));
    }
    let Some(init_message) = init.message else {
        return EntryOutcome::Snapshot(failed_snapshot(
            TRANSPORT,
            "initialize",
            "endpoint answered 200 without an initialize result".into(),
        ));
    };
    if let Some(error) = init_message.get("error") {
        return EntryOutcome::Snapshot(failed_snapshot(
            TRANSPORT,
            "initialize",
            format!("server answered initialize with error {error}"),
        ));
    }
    let result = init_message.get("result").cloned().unwrap_or(Value::Null);
    let negotiated = result
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    // The snapshot's stateless flag describes the walk's shape,
    // not the negotiated string, and this ladder walks statefully
    // whatever version the server names.
    let mut snapshot = handshake_snapshot(TRANSPORT, &result, false);
    let session = init.session_id.as_deref();
    let version = Some(negotiated.as_str());
    let mut next_id = 2_i64;
    // The notification expects a 202 with no body, and its
    // outcome is otherwise uninteresting. A 401 there is not,
    // because it means the surface is protected after all, so
    // that one answer routes to the metadata ladder.
    let stateful = Conversation {
        url,
        session,
        version,
        token,
    };
    if let Ok(outcome) = rpc(
        agent,
        &stateful,
        None,
        "notifications/initialized",
        json!({}),
    ) && outcome.status == 401
    {
        return EntryOutcome::Challenged {
            step: "notifications/initialized",
            snapshot: protected_snapshot(agent, url, outcome.www_authenticate.as_deref()),
        };
    }

    if let ListWalk::Protected(protected, step) =
        walk_lists(agent, &stateful, &mut snapshot, false, &mut next_id)
    {
        return EntryOutcome::Challenged {
            step,
            snapshot: *protected,
        };
    }
    if let ReadWalk::Protected(protected, step) =
        read_resource_contents_http(agent, &stateful, &mut snapshot, false, &mut next_id)
    {
        return EntryOutcome::Challenged {
            step,
            snapshot: *protected,
        };
    }

    if let Some(session) = session {
        let mut delete = agent
            .delete(url)
            .header("Mcp-Session-Id", session)
            .header("MCP-Protocol-Version", negotiated.as_str());
        if let Some(token) = token {
            delete = delete.header("Authorization", bearer(token));
        }
        let _ = delete.call();
    }
    EntryOutcome::Snapshot(snapshot)
}
