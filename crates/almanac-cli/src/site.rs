//! Site reading, writing, and the generate, validate, check
//! and migrate commands.
//!
//! A site directory stands for a publisher's web root plus the
//! repo-side baseline file. The catalog pair lives under
//! .well-known, the server card under .well-known/mcp, skill files
//! under skills, and baseline.json at the site root, where it is
//! committed like a lockfile and never served as part of the
//! catalog. URLs in catalog entries map back to files by stripping
//! the publisher's own origin, which is how the checker reads the
//! artifacts as served.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use almanac_core::Severity;
use almanac_core::a2a::{A2aView, reduce_agent_card};
use almanac_core::baseline::{
    A2aBaseline, ArtifactBaseline, Baseline, EntryProvenance, McpBaseline, OpenApiBaseline,
    SkillBaseline,
};
use almanac_core::catalog::{Catalog, Entry, Host};
use almanac_core::drift::{self, CardView, CheckInputs, Report, Suppression};
use almanac_core::hash;
use almanac_core::metadata::{RepoMetadata, reduce_repo_metadata};
use almanac_core::openapi::{OpenApiView, reduce_openapi};
use almanac_core::snapshot::{AuthSurface, SurfaceSnapshot, ToolSurface};
use serde::Serialize;

use crate::probe;
use crate::probe_http;

/// Where a probe points, a local server command or a remote URL.
pub enum ProbeTarget {
    Stdio(Vec<String>),
    Http(String),
}

pub fn run_probe(target: &ProbeTarget) -> SurfaceSnapshot {
    match target {
        ProbeTarget::Stdio(command) => probe::probe_stdio(command),
        ProbeTarget::Http(url) => probe_http::probe_http(url),
    }
}

pub const SPEC_VERSION: &str = "ard/0.91";

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {} with {e}", parent.display()))?;
    }
    fs::write(path, bytes).map_err(|e| format!("could not write {} with {e}", path.display()))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|e| format!("could not read {} with {e}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("could not parse {} with {e}", path.display()))
}

fn pretty_bytes<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(value).expect("the model always serializes");
    text.push('\n');
    text.into_bytes()
}

/// Map a catalog URL back to the file the site serves for it, when
/// the URL belongs to the publisher's own origin. The origin match
/// must end at a path boundary, so a look-alike host that merely
/// starts with the publisher's domain does not map, and no path
/// segment may climb out of the site directory, because the
/// catalog is input and the checker reads only inside the site.
fn url_to_path(site: &Path, domain: &str, url: &str) -> Option<PathBuf> {
    let prefix = format!("https://{domain}");
    let rest = url
        .strip_prefix(&prefix)
        .or_else(|| url.strip_prefix(&format!("http://{domain}")))?;
    if !rest.is_empty() && !rest.starts_with('/') {
        return None;
    }
    if rest.split('/').any(|segment| segment == "..") {
        return None;
    }
    Some(site.join(rest.trim_start_matches('/')))
}

/// Parse a SKILL.md frontmatter block for name and description.
/// Frontmatter here is the simple key and value lines between the
/// two --- markers. Anything fancier is the publisher's editor's
/// problem, and a missing name is a hard error at generation.
fn parse_skill_frontmatter(text: &str) -> Result<(String, String), String> {
    let mut in_frontmatter = false;
    let mut name = None;
    let mut description = None;
    for line in text.lines() {
        if line.trim() == "---" {
            if in_frontmatter {
                break;
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter && let Some((key, value)) = line.split_once(':') {
            match key.trim() {
                "name" => name = Some(value.trim().trim_matches('"').to_string()),
                "description" => description = Some(value.trim().trim_matches('"').to_string()),
                _ => {}
            }
        }
    }
    match (name, description) {
        (Some(name), Some(description)) => Ok((name, description)),
        _ => Err("SKILL.md frontmatter must carry name and description".into()),
    }
}

/// Build the generation snapshot for a protected surface from
/// its server card. The card is fetched from the well known
/// card path on the endpoint's origin, and a card already on
/// the site is the fallback when the fetch finds nothing. The
/// returned bytes are the card as read, with the probed
/// endpoint recorded when the card does not name one and the
/// target is HTTP, ready to be written as the site's card so
/// the digest binds what generation read. The card's own
/// description comes back too, when it carries one.
fn snapshot_from_card(
    site: &Path,
    target: &ProbeTarget,
    probe_snapshot: &SurfaceSnapshot,
) -> Result<(SurfaceSnapshot, Vec<u8>, Option<String>), String> {
    let fetched = match target {
        ProbeTarget::Http(url) => probe_http::fetch_server_card(url),
        ProbeTarget::Stdio(_) => None,
    };
    let raw = match fetched {
        Some(bytes) => bytes,
        None => {
            let path = site.join(".well-known/mcp/server-card.json");
            fs::read(&path).map_err(|_| {
                "the surface is protected, and no server card could be read from the \
                 well known card path or from the site, so generation has no declared \
                 surface to record"
                    .to_string()
            })?
        }
    };
    let mut card: serde_json::Value = serde_json::from_slice(&raw)
        .map_err(|e| format!("the server card did not parse with {e}"))?;
    let fields = card_fields(&card);
    if fields.server_name.is_empty() {
        return Err(
            "the server card carries no name, so generation cannot record the surface".into(),
        );
    }
    let snapshot = SurfaceSnapshot {
        transport: probe_snapshot.transport.clone(),
        server_name: fields.server_name,
        server_version: fields.server_version,
        stateless_shape: fields.protocol_version == "2026-07-28",
        protocol_version: fields.protocol_version,
        capabilities_enumerated: fields.tools.is_some(),
        tools: fields.tools.unwrap_or_default(),
        resources: fields.resources,
        prompts: fields.prompts,
        auth: probe_snapshot.auth.clone(),
        failure: None,
    };
    // A card that names no endpoint leaves a later check with
    // nothing to resolve, so the target the card was fetched
    // for is written into the published copy, in the card's
    // own shape: a remotes entry appended for a Final card,
    // the endpoint field for an earlier one. Appending, never
    // replacing, because a Final card's existing remotes are
    // the publisher's own statements and the injection adds
    // the one fact the card was missing.
    let bytes = if fields.endpoint.is_none()
        && let ProbeTarget::Http(url) = target
    {
        if final_shaped(&card) {
            let remote = serde_json::json!({
                "type": "streamable-http",
                "url": url,
            });
            match card.get_mut("remotes") {
                Some(serde_json::Value::Array(items)) => items.push(remote),
                _ => card["remotes"] = serde_json::json!([remote]),
            }
        } else {
            card["endpoint"] = serde_json::json!(url);
        }
        pretty_bytes(&card)
    } else {
        raw
    };
    Ok((snapshot, bytes, fields.description))
}

/// The normative server card location for a probe target, per
/// SEP-2127 Final: the endpoint URL with /server-card appended.
/// The well known path is the legacy location, still served
/// beside it because major scanners still request it. A stdio
/// target has no endpoint URL, so it has no normative location.
fn normative_card_url_for(target: &ProbeTarget) -> Option<String> {
    match target {
        ProbeTarget::Http(url) => Some(crate::probe::normative_card_url(url)),
        ProbeTarget::Stdio(_) => None,
    }
}

/// A server card reduced to the fields the tool reads, from
/// either shape a publisher may serve. The SEP-2127 Final
/// shape names the server in reverse DNS form, carries its
/// protocol versions and endpoint in a remotes array, and
/// lists no tools. The earlier shape, which this tool's first
/// versions wrote and publishers still serve, names the
/// server plainly and carries its endpoint, protocol version
/// and tools as top level fields. One reduction serves both,
/// because dropping the earlier shape would blind the tool
/// to the cards already published.
struct CardFields {
    server_name: String,
    server_version: String,
    protocol_version: String,
    endpoint: Option<String>,
    description: Option<String>,
    /// The card's tools, when it carries a tools array. A
    /// Final shape card carries none, and the absence is
    /// recorded as such, never as an empty list.
    tools: Option<Vec<ToolSurface>>,
    resources: Vec<String>,
    prompts: Vec<String>,
}

/// Whether a card is written in the SEP-2127 Final shape.
/// The Final's own fields are the tell: a card carrying a
/// $schema field or a remotes array speaks the Final, and a
/// card carrying neither speaks the earlier shape.
fn final_shaped(card: &serde_json::Value) -> bool {
    card.get("$schema").is_some() || card.get("remotes").is_some()
}

fn card_fields(card: &serde_json::Value) -> CardFields {
    let text = |key: &str| {
        card.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    // A Final name is the publisher's domain reversed, a
    // slash, and the server's name, so the server's own name
    // is what follows the first slash, slashes inside the
    // server's name included. An earlier card's name is the
    // server's name plainly, slashes and all, and reduces to
    // itself. A Final name whose remainder is empty names no
    // server, and reduces to empty so the caller hears it.
    let raw_name = text("name");
    let server_name = if final_shaped(card) {
        match raw_name.split_once('/') {
            Some((_, rest)) => rest.trim_end_matches('/').to_string(),
            None => raw_name.clone(),
        }
    } else {
        raw_name.clone()
    };
    let remotes: Vec<serde_json::Value> = card
        .get("remotes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    // One remote is primary, so the endpoint and the protocol
    // version come from the same statement: the first
    // streamable-http remote that names a url, because that
    // is the transport the probe speaks, else the first
    // remote that names one, else the first remote.
    let remote_url =
        |r: &serde_json::Value| r.get("url").and_then(|v| v.as_str()).map(String::from);
    let primary = remotes
        .iter()
        .find(|r| {
            r.get("type").and_then(|v| v.as_str()) == Some("streamable-http")
                && remote_url(r).is_some()
        })
        .or_else(|| remotes.iter().find(|r| remote_url(r).is_some()))
        .or_else(|| remotes.first());
    let first_remote_version = |r: &serde_json::Value| {
        r.get("supportedProtocolVersions")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let remote_version = primary
        .and_then(first_remote_version)
        .or_else(|| remotes.iter().find_map(first_remote_version));
    // A Final card's remotes are its authoritative endpoint
    // statement, so they win over a stale legacy field the
    // same document may still carry. An earlier card's own
    // endpoint field wins, as it always has.
    let legacy_endpoint = card
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(String::from);
    let remote_endpoint = primary.and_then(remote_url);
    let endpoint = if final_shaped(card) {
        remote_endpoint.or(legacy_endpoint)
    } else {
        legacy_endpoint.or(remote_endpoint)
    };
    let protocol_version = {
        let legacy = text("protocolVersion");
        if !legacy.is_empty() {
            legacy
        } else {
            remote_version.unwrap_or_default()
        }
    };
    // Enumeration turns on the field's presence, not on its
    // shape. A tools field that is present but not an array
    // is a corrupted statement of the surface, and it reduces
    // to an empty list so the comparisons fire on it, the way
    // they did before this reduction existed. Only a card
    // with no tools field at all is unenumerated.
    let tools = card.get("tools").map(|value| {
        value
            .as_array()
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
                            .unwrap_or_else(|| serde_json::json!({})),
                    })
                    .collect()
            })
            .unwrap_or_default()
    });
    let names = |key: &str, field: &str| -> Vec<String> {
        let mut out: Vec<String> = card
            .get(key)
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.as_str()
                            .map(String::from)
                            .or_else(|| item.get(field).and_then(|v| v.as_str()).map(String::from))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    };
    let description = text("description");
    CardFields {
        server_name,
        server_version: text("version"),
        protocol_version,
        endpoint,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        tools,
        resources: names("resources", "uri"),
        prompts: names("prompts", "name"),
    }
}

/// The server card document for a probed surface, as a value,
/// in the SEP-2127 Final shape. The name is the site's domain
/// reversed, a slash, and the server's name, the form the
/// Final requires. An HTTP surface adds one remote naming the
/// endpoint it was probed at and the protocol version the
/// probe negotiated, so a later check with no target named
/// can resolve the endpoint from the catalog entry's card,
/// the way docs/HTTP-TRANSPORT.md asks, instead of guessing.
/// The card lists no tools, because the Final leaves
/// primitives to the live surface on purpose.
fn card_value(
    snapshot: &SurfaceSnapshot,
    target: &ProbeTarget,
    domain: &str,
    description: Option<&str>,
    title: Option<&str>,
) -> serde_json::Value {
    // The domain is a host name for this purpose: a port or a
    // trailing dot in it would land in the published name,
    // which must be a reverse DNS form and nothing else.
    let host = domain.trim_end_matches('.');
    let host = host.split(':').next().unwrap_or(host);
    let reversed = host.split('.').rev().collect::<Vec<_>>().join(".");
    // The card and the catalog entry describe the server in
    // the same words: the caller passes the description it
    // resolved for the entry, and the standing template is
    // only the default when it passes none.
    let description: String = match description {
        Some(description) => description.chars().take(100).collect(),
        None => format!("The {} MCP server.", snapshot.server_name)
            .chars()
            .take(100)
            .collect(),
    };
    let mut card = serde_json::json!({
        "$schema": "https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json",
        "name": format!("{reversed}/{}", snapshot.server_name),
        "title": title.unwrap_or(&snapshot.server_name),
        "version": snapshot.server_version,
        "description": description,
    });
    if let ProbeTarget::Http(url) = target {
        let mut remote = serde_json::json!({
            "type": "streamable-http",
            "url": url,
        });
        if !snapshot.protocol_version.is_empty() {
            remote["supportedProtocolVersions"] = serde_json::json!([snapshot.protocol_version]);
        }
        card["remotes"] = serde_json::json!([remote]);
    }
    card
}

/// Read the repo metadata generate may fall back on for
/// editorial words: the manifest files in the site
/// directory, then in its parent, in the order package.json,
/// Cargo.toml, pyproject.toml. A manifest that cannot be
/// read or parsed is skipped without noise, because a
/// fallback source must never kill a generation run.
fn read_repo_metadata(site: &Path) -> RepoMetadata {
    let mut dirs: Vec<(PathBuf, &str)> = vec![(site.to_path_buf(), "in the site directory")];
    if let Some(parent) = site.parent() {
        dirs.push((parent.to_path_buf(), "in the parent directory"));
    }
    let mut docs: Vec<(String, String, serde_json::Value)> = Vec::new();
    for (dir, whereabouts) in &dirs {
        for kind in ["package.json", "Cargo.toml", "pyproject.toml"] {
            let path = dir.join(kind);
            // Only a regular file of a plausible manifest size
            // is read: a fallback source must not be able to
            // hang the run on a pipe or exhaust it on a huge
            // file that happens to carry a manifest's name.
            let Ok(facts) = fs::metadata(&path) else {
                continue;
            };
            if !facts.is_file() || facts.len() > 1_048_576 {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let doc = if kind == "package.json" {
                serde_json::from_slice::<serde_json::Value>(&bytes).ok()
            } else {
                std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|text| text.parse::<toml::Value>().ok())
                    .and_then(|value| serde_json::to_value(value).ok())
            };
            if let Some(doc) = doc {
                docs.push((kind.to_string(), format!("{kind} {whereabouts}"), doc));
            }
        }
    }
    let sources: Vec<(&str, &str, &serde_json::Value)> = docs
        .iter()
        .map(|(kind, label, doc)| (kind.as_str(), label.as_str(), doc))
        .collect();
    reduce_repo_metadata(&sources)
}

/// The low confidence flag for one use of repo metadata
/// words, in the generation report's note form. The notes
/// are collected during the run and printed only once the
/// catalog is about to be written, so a run that refuses
/// never claims words for a catalog that does not exist.
fn metadata_note(what: &str, source: &str) -> String {
    format!("note, {what} from repo metadata in {source}, the lowest confidence editorial source")
}

/// The note a suppressed entry leaves in the generation
/// report, naming the entry and the recorded reason.
fn suppression_note(identifier: &str, reason: &str) -> String {
    format!("note, {identifier} is suppressed in almanac.toml and is not announced: {reason}")
}

/// The generate command. Probes the live surface, writes the dual
/// path catalog, the server card and the baseline. A protected
/// surface is generated from its server card instead.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    site: &Path,
    domain: &str,
    display_name: Option<&str>,
    target: &ProbeTarget,
    skill_path: Option<&Path>,
    openapi_paths: &[PathBuf],
    agent_card_paths: &[PathBuf],
    queries: &[String],
) -> Result<(), String> {
    // Publisher config is the first editorial source, loaded
    // and validated before the probe runs, so a configuration
    // error refuses the run without spending a probe on it.
    let config = load_config(site)?;
    validate_entry_blocks(&config)?;
    let probe_snapshot = run_probe(target);
    if let Some(failure) = &probe_snapshot.failure {
        return Err(format!(
            "the probe failed at {} with {}, and generation needs a surface it can read",
            failure.step, failure.detail
        ));
    }
    // Repo metadata is the last editorial source, read once
    // for the whole run: the host's display name and any
    // entry description no stronger source supplies.
    let metadata = read_repo_metadata(site);
    let mut notes: Vec<String> = Vec::new();
    let display_name: String = match display_name {
        Some(name) => name.to_string(),
        None => match config
            .host
            .as_ref()
            .and_then(|host| host.display_name.as_ref())
        {
            Some(name) => name.clone(),
            None => match (&metadata.name, &metadata.name_source) {
                (Some(name), Some(source)) => {
                    notes.push(metadata_note("the host takes its display name", source));
                    name.clone()
                }
                _ => domain.to_string(),
            },
        },
    };
    // A protected surface cannot be probed live, so generation
    // reads it from its server card instead, and the baseline
    // records the auth metadata the ladder fetched alongside.
    let mut card_source: Option<(Vec<u8>, Option<String>)> = None;
    let snapshot = if matches!(probe_snapshot.auth, AuthSurface::Protected(_)) {
        let (built, bytes, description) = snapshot_from_card(site, target, &probe_snapshot)?;
        card_source = Some((bytes, description));
        built
    } else {
        probe_snapshot
    };

    let entry_id = format!("urn:air:{domain}:mcp:{}", snapshot.server_name);
    let mut matched: std::collections::HashSet<String> =
        std::collections::HashSet::from([entry_id.clone()]);
    let mcp_block = entry_block(&config, &entry_id);
    let mcp_suppressed = mcp_block.and_then(|block| block.suppress.as_deref());
    if let Some(reason) = mcp_suppressed {
        notes.push(suppression_note(&entry_id, reason));
    }
    if card_source.is_some()
        && mcp_suppressed.is_none()
        && mcp_block
            .is_some_and(|block| block.description.is_some() || block.display_name.is_some())
    {
        eprintln!(
            "warning, almanac.toml overrides the MCP entry's editorial fields, but the surface \
             is protected and its existing server card is republished unchanged, so the card \
             keeps its own words"
        );
    }
    // The entry's description is resolved before the card is
    // written, because the generated card carries the same
    // words. Config speaks first, then the card, then repo
    // metadata, then the template. A stronger source's
    // whitespace-only description counts as no description,
    // the trim discipline the metadata reduction itself
    // applies.
    let mcp_description = match mcp_block.and_then(|block| block.description.as_ref()) {
        Some(description) => description.clone(),
        None => match &card_source {
            Some((_, Some(description))) if !description.trim().is_empty() => description.clone(),
            _ => match (&metadata.description, &metadata.description_source) {
                (Some(description), Some(source)) => {
                    notes.push(metadata_note(
                        &format!("{entry_id} takes its description"),
                        source,
                    ));
                    description.clone()
                }
                _ => format!("The {} MCP server.", snapshot.server_name),
            },
        },
    };
    // The entry's display name resolves with config first,
    // like its description, and the generated card's title
    // carries the same name.
    let mcp_display_name = mcp_block
        .and_then(|block| block.display_name.clone())
        .unwrap_or_else(|| snapshot.server_name.clone());
    let card_bytes = if mcp_suppressed.is_some() {
        Vec::new()
    } else {
        match &card_source {
            Some((bytes, _)) => bytes.clone(),
            None => {
                let card = card_value(
                    &snapshot,
                    target,
                    domain,
                    Some(&mcp_description),
                    Some(&mcp_display_name),
                );
                warn_on_card_disagreement(site, &card, &snapshot);
                pretty_bytes(&card)
            }
        }
    };
    let card_url = format!("https://{domain}/.well-known/mcp/server-card.json");
    if mcp_suppressed.is_none() {
        write_file(&site.join(".well-known/mcp/server-card.json"), &card_bytes)?;
    }
    // The normative copy, at the endpoint's own path, when the
    // endpoint lives on this site's origin. Identical bytes, so
    // the one digest binds both locations, and the baseline
    // records the copy below as a second artifact of the entry.
    let normative_card: Option<(String, PathBuf)> = normative_card_url_for(target)
        .and_then(|url| url_to_path(site, domain, &url).map(|path| (url, path)));
    if mcp_suppressed.is_none() {
        if let Some((_, path)) = &normative_card {
            write_file(path, &card_bytes)?;
        }
        if matches!(target, ProbeTarget::Http(_)) && normative_card.is_none() {
            eprintln!(
                "warning, the endpoint is not on the site's origin, so the server card \
                 is published at the well known path only"
            );
        }
    }
    if mcp_suppressed.is_some() {
        if card_source.is_some() {
            eprintln!(
                "warning, the surface is protected, so its existing server card remains \
                 published; suppressing the entry silences the catalog only"
            );
        } else {
            // The card on an open surface is one almanac itself
            // generated, so a suppressed server does not stay
            // discoverable through copies an earlier run wrote.
            let _ = fs::remove_file(site.join(".well-known/mcp/server-card.json"));
            if let Some((_, path)) = &normative_card {
                let _ = fs::remove_file(path);
            }
        }
    }

    let tool_names: Vec<String> = snapshot.tools.iter().map(|t| t.name.clone()).collect();
    let representative_queries = if queries.is_empty() {
        vec![
            format!("What can {} do", snapshot.server_name),
            format!("Use {} to complete a task", snapshot.server_name),
            format!("Show the tools {} offers", snapshot.server_name),
        ]
    } else {
        queries.to_vec()
    };
    let mut entries: Vec<Entry> = Vec::new();
    let mut mcp_baseline: Option<McpBaseline> = None;
    let mut artifacts: Vec<ArtifactBaseline> = Vec::new();
    let mut provenance: Vec<EntryProvenance> = Vec::new();
    if mcp_suppressed.is_none() {
        entries.push(Entry {
            identifier: entry_id.clone(),
            display_name: mcp_display_name,
            media_type: "application/mcp-server-card+json".into(),
            url: Some(card_url.clone()),
            data: None,
            description: mcp_description,
            capabilities: tool_names,
            representative_queries,
            tags: vec![],
            version: Some(snapshot.server_version.clone()),
            digest: Some(hash::digest_field(&card_bytes)),
            updated_at: None,
            trust_manifest: None,
        });
        mcp_baseline = Some(McpBaseline::from_snapshot(&entry_id, &snapshot));
        artifacts.push(ArtifactBaseline {
            entry_id: entry_id.clone(),
            url: card_url,
            sha256: hash::sha256_hex(&card_bytes),
        });
        if let Some((url, _)) = &normative_card {
            artifacts.push(ArtifactBaseline {
                entry_id: entry_id.clone(),
                url: url.clone(),
                sha256: hash::sha256_hex(&card_bytes),
            });
        }
        provenance.push(EntryProvenance {
            entry_id: entry_id.clone(),
            produced_by: if card_source.is_some() {
                "server-card"
            } else {
                "live-probe"
            }
            .into(),
            overruled: vec![],
        });
    }
    let mut skills = Vec::new();

    if let Some(skill_path) = skill_path {
        let bytes = fs::read(skill_path)
            .map_err(|e| format!("could not read {} with {e}", skill_path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let (name, description) = parse_skill_frontmatter(&text)?;
        if !valid_plain_name(&name) {
            return Err(format!(
                "the SKILL.md name {name} is not a plain skill name, and generation will not write a path from it"
            ));
        }
        let skill_id = format!("urn:air:{domain}:skill:{name}");
        let skill_url = format!("https://{domain}/skills/{name}/SKILL.md");
        // A suppressed skill is decided here, before its file
        // is written: check walks the site's skills directory,
        // so a served file with no baseline record would read
        // as an unannounced skill at Critical. A stale copy an
        // earlier run placed is removed, unless the file given
        // is that copy itself.
        let skill_suppressed =
            entry_block(&config, &skill_id).and_then(|block| block.suppress.as_deref());
        if let Some(reason) = skill_suppressed {
            notes.push(suppression_note(&skill_id, reason));
            matched.insert(skill_id.clone());
            let dest = site.join(format!("skills/{name}/SKILL.md"));
            if skill_path.canonicalize().ok() != dest.canonicalize().ok() {
                let _ = fs::remove_file(&dest);
            }
        }
        if skill_suppressed.is_none() {
            write_file(&site.join(format!("skills/{name}/SKILL.md")), &bytes)?;
            entries.push(Entry {
                identifier: skill_id.clone(),
                display_name: name.clone(),
                media_type: "text/markdown".into(),
                url: Some(skill_url.clone()),
                data: None,
                description,
                capabilities: vec![],
                representative_queries: vec![
                    format!("How do I use the {name} skill"),
                    format!("Follow the {name} skill instructions"),
                ],
                tags: vec![],
                version: None,
                digest: Some(hash::digest_field(&bytes)),
                updated_at: None,
                trust_manifest: None,
            });
            skills.push(SkillBaseline {
                entry_id: skill_id.clone(),
                sha256: hash::sha256_hex(&bytes),
            });
            provenance.push(EntryProvenance {
                entry_id: skill_id.clone(),
                produced_by: "skill-file".into(),
                overruled: vec![],
            });
            artifacts.push(ArtifactBaseline {
                entry_id: skill_id,
                url: skill_url,
                sha256: hash::sha256_hex(&bytes),
            });
        }
    }

    // The generation roadmap's first input. An OpenAPI document
    // becomes an API entry, and the baseline records its facts
    // for the fact by fact comparison check runs. Every
    // document is read, reduced and planned before any of them
    // is written, so a run that refuses (a bad document, a
    // duplicate identifier) leaves the site exactly as it was,
    // and no document can overwrite another document's source
    // before that source has been read.
    struct PlannedApi {
        source: String,
        bytes: Vec<u8>,
        view: OpenApiView,
        api_id: String,
        api_url: String,
        copy_to: Option<PathBuf>,
    }
    let site_canon = fs::canonicalize(site).unwrap_or_else(|_| site.to_path_buf());
    // Every placed path an input claims, across both
    // document kinds: a copy destination or an in-place
    // source. Two inputs claiming one path would overwrite
    // each other or a publisher's own file, so planning
    // refuses the second claim, whichever kind makes it.
    let mut claimed: Vec<PathBuf> = Vec::new();
    // Copies are written only once every input is planned,
    // so no write can land on a file a later plan still has
    // to read.
    let mut pending_copies: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut planned: Vec<PlannedApi> = Vec::new();
    for openapi_path in openapi_paths {
        let bytes = fs::read(openapi_path)
            .map_err(|e| format!("could not read {} with {e}", openapi_path.display()))?;
        let doc: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "{} is not a JSON document ({e}), and generate reads OpenAPI documents in JSON only, never YAML",
                openapi_path.display()
            )
        })?;
        let view = reduce_openapi(&doc).map_err(|reason| {
            format!("{} cannot be announced, {reason}", openapi_path.display())
        })?;
        let file_name = openapi_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{} has no usable file name", openapi_path.display()))?;
        if file_name == "baseline.json" {
            return Err(
                "an OpenAPI document named baseline.json cannot be announced, because that name belongs to the site's own baseline"
                    .into(),
            );
        }
        let stem = file_name.strip_suffix(".json").unwrap_or(file_name);
        if !valid_plain_name(stem) {
            return Err(format!(
                "the file stem {stem} is not a plain name, and generation will not build an identifier from it"
            ));
        }
        let api_id = format!("urn:air:{domain}:openapi:{stem}");
        if entries.iter().any(|e| e.identifier == api_id)
            || planned.iter().any(|p| p.api_id == api_id)
        {
            return Err(format!(
                "two OpenAPI documents would both announce {api_id}, and generation will not merge them"
            ));
        }
        // Where the document lives in the site, resolved on
        // canonical paths, so a spelled path lands at the URL
        // the checker will look for. Inside the site the
        // document is used in place; outside it, it is copied
        // to the site root under its file name.
        let doc_canon =
            fs::canonicalize(openapi_path).unwrap_or_else(|_| openapi_path.to_path_buf());
        let (api_url, copy_to) = match doc_canon.strip_prefix(&site_canon) {
            Ok(rel) => (
                format!(
                    "https://{domain}/{}",
                    rel.to_string_lossy().replace('\\', "/")
                ),
                None,
            ),
            Err(_) => (
                format!("https://{domain}/{file_name}"),
                Some(site.join(file_name)),
            ),
        };
        let placed = copy_to
            .as_ref()
            .map(|_| site_canon.join(file_name))
            .unwrap_or_else(|| doc_canon.clone());
        if claimed.contains(&placed) {
            return Err(format!(
                "{} would land on {}, which another input already claims, and generation will not overwrite it",
                openapi_path.display(),
                placed.display()
            ));
        }
        claimed.push(placed);
        planned.push(PlannedApi {
            source: openapi_path.display().to_string(),
            bytes,
            view,
            api_id,
            api_url,
            copy_to,
        });
    }
    let mut openapi: Vec<OpenApiBaseline> = Vec::new();
    for plan in planned {
        // A suppressed document is decided here, before its
        // copy is even queued: suppression silences the
        // announcement and the artifact alike.
        if let Some(reason) =
            entry_block(&config, &plan.api_id).and_then(|block| block.suppress.as_deref())
        {
            notes.push(suppression_note(&plan.api_id, reason));
            matched.insert(plan.api_id.clone());
            continue;
        }
        if let Some(dest) = &plan.copy_to {
            pending_copies.push((dest.clone(), plan.bytes.clone()));
        }
        let description = match entry_block(&config, &plan.api_id)
            .and_then(|block| block.description.as_ref())
        {
            Some(description) => description.clone(),
            None => match &plan.view.description {
                Some(description) if !description.trim().is_empty() => description.clone(),
                _ => match (&metadata.description, &metadata.description_source) {
                    (Some(description), Some(source)) => {
                        notes.push(metadata_note(
                            &format!("{} takes its description", plan.api_id),
                            source,
                        ));
                        description.clone()
                    }
                    _ => {
                        eprintln!(
                            "warning, {} carries no info description, so the entry announces its title as its description",
                            plan.source
                        );
                        plan.view.title.clone()
                    }
                },
            },
        };
        entries.push(Entry {
            identifier: plan.api_id.clone(),
            display_name: plan.view.title.clone(),
            media_type: "application/openapi+json".into(),
            url: Some(plan.api_url.clone()),
            data: None,
            description,
            capabilities: plan
                .view
                .operations
                .iter()
                .map(|op| op.id.clone())
                .collect(),
            representative_queries: vec![
                format!("What can the {} API do", plan.view.title),
                format!("Use the {} API to complete a task", plan.view.title),
            ],
            tags: vec![],
            version: Some(plan.view.version.clone()),
            digest: Some(hash::digest_field(&plan.bytes)),
            updated_at: None,
            trust_manifest: None,
        });
        openapi.push(OpenApiBaseline::from_view(
            &plan.api_id,
            &plan.api_url,
            &hash::sha256_hex(&plan.bytes),
            &plan.view,
        ));
        provenance.push(EntryProvenance {
            entry_id: plan.api_id,
            produced_by: "openapi-file".into(),
            overruled: vec![],
        });
    }

    // The generation roadmap's second input. An A2A agent
    // card becomes an agent entry, planned exactly the way
    // the OpenAPI documents are planned: read, reduced and
    // placed before anything is written.
    struct PlannedAgentCard {
        source: String,
        bytes: Vec<u8>,
        view: A2aView,
        examples: Vec<String>,
        card_id: String,
        card_url: String,
        copy_to: Option<PathBuf>,
    }
    let mut planned_cards: Vec<PlannedAgentCard> = Vec::new();
    for card_path in agent_card_paths {
        let bytes = fs::read(card_path)
            .map_err(|e| format!("could not read {} with {e}", card_path.display()))?;
        let doc: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "{} is not a JSON document ({e}), and generate reads agent cards in JSON only",
                card_path.display()
            )
        })?;
        let view = reduce_agent_card(&doc)
            .map_err(|reason| format!("{} cannot be announced, {reason}", card_path.display()))?;
        let file_name = card_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{} has no usable file name", card_path.display()))?;
        if file_name == "baseline.json" {
            return Err(
                "an agent card named baseline.json cannot be announced, because that name belongs to the site's own baseline"
                    .into(),
            );
        }
        let stem = file_name.strip_suffix(".json").unwrap_or(file_name);
        if !valid_plain_name(stem) {
            return Err(format!(
                "the file stem {stem} is not a plain name, and generation will not build an identifier from it"
            ));
        }
        let card_id = format!("urn:air:{domain}:a2a:{stem}");
        if entries.iter().any(|e| e.identifier == card_id)
            || planned_cards.iter().any(|p| p.card_id == card_id)
        {
            return Err(format!(
                "two inputs would both announce {card_id}, and generation will not merge them"
            ));
        }
        // The skills' own examples, in card order, are the
        // representative queries' first source: the
        // publisher's words, never generated text. The
        // reduction harvested them from the skills it kept,
        // so the harvest lives in one place, in the core.
        let examples: Vec<String> = view.examples.iter().take(5).cloned().collect();
        // Where the card lives in the site, resolved the
        // same way as an OpenAPI document's placement.
        let doc_canon = fs::canonicalize(card_path).unwrap_or_else(|_| card_path.to_path_buf());
        let (card_url, copy_to) = match doc_canon.strip_prefix(&site_canon) {
            Ok(rel) => (
                format!(
                    "https://{domain}/{}",
                    rel.to_string_lossy().replace('\\', "/")
                ),
                None,
            ),
            Err(_) => (
                format!("https://{domain}/{file_name}"),
                Some(site.join(file_name)),
            ),
        };
        let placed = copy_to
            .as_ref()
            .map(|_| site_canon.join(file_name))
            .unwrap_or_else(|| doc_canon.clone());
        if claimed.contains(&placed) {
            return Err(format!(
                "{} would land on {}, which another input already claims, and generation will not overwrite it",
                card_path.display(),
                placed.display()
            ));
        }
        claimed.push(placed);
        planned_cards.push(PlannedAgentCard {
            source: card_path.display().to_string(),
            bytes,
            view,
            examples,
            card_id,
            card_url,
            copy_to,
        });
    }
    let mut a2a: Vec<A2aBaseline> = Vec::new();
    for plan in planned_cards {
        // A suppressed card is decided here, before its copy
        // is even queued, the same discipline as the OpenAPI
        // documents.
        if let Some(reason) =
            entry_block(&config, &plan.card_id).and_then(|block| block.suppress.as_deref())
        {
            notes.push(suppression_note(&plan.card_id, reason));
            matched.insert(plan.card_id.clone());
            continue;
        }
        if let Some(dest) = &plan.copy_to {
            pending_copies.push((dest.clone(), plan.bytes.clone()));
        }
        let description = match entry_block(&config, &plan.card_id)
            .and_then(|block| block.description.as_ref())
        {
            Some(description) => description.clone(),
            None => match &plan.view.description {
                Some(description) if !description.trim().is_empty() => description.clone(),
                _ => match (&metadata.description, &metadata.description_source) {
                    (Some(description), Some(source)) => {
                        notes.push(metadata_note(
                            &format!("{} takes its description", plan.card_id),
                            source,
                        ));
                        description.clone()
                    }
                    _ => {
                        eprintln!(
                            "warning, {} carries no description, so the entry announces its name as its description",
                            plan.source
                        );
                        plan.view.name.clone()
                    }
                },
            },
        };
        let mut queries = plan.examples.clone();
        for template in [
            format!("What can the {} agent do", plan.view.name),
            format!("Ask the {} agent to complete a task", plan.view.name),
        ] {
            if queries.len() >= 2 {
                break;
            }
            if !queries.contains(&template) {
                queries.push(template);
            }
        }
        entries.push(Entry {
            identifier: plan.card_id.clone(),
            display_name: plan.view.name.clone(),
            media_type: "application/a2a-agent-card+json".into(),
            url: Some(plan.card_url.clone()),
            data: None,
            description,
            capabilities: plan.view.skills.iter().map(|s| s.id.clone()).collect(),
            representative_queries: queries,
            tags: vec![],
            version: Some(plan.view.version.clone()),
            digest: Some(hash::digest_field(&plan.bytes)),
            updated_at: None,
            trust_manifest: None,
        });
        a2a.push(A2aBaseline::from_view(
            &plan.card_id,
            &plan.card_url,
            &hash::sha256_hex(&plan.bytes),
            &plan.view,
        ));
        provenance.push(EntryProvenance {
            entry_id: plan.card_id,
            produced_by: "agent-card-file".into(),
            overruled: vec![],
        });
    }

    // The publisher config post-pass, DESIGN rule 4.
    // Overrides replace the editorial fields they name on
    // the entries the run actually generated (descriptions
    // were already resolved config first in each build loop
    // and in the MCP chain, so reapplying them here is the
    // same words; the skill entry's description is applied
    // only here, its frontmatter having no rival note to
    // retract). Suppressions were decided at each entry's
    // construction, so nothing is removed here. A block
    // whose identifier the run did not generate is warned
    // on by name: config drift is surfaced, and an entry is
    // never invented to satisfy config.
    for entry in &mut entries {
        if let Some(block) = entry_block(&config, &entry.identifier) {
            matched.insert(block.identifier.clone());
            if let Some(name) = &block.display_name {
                entry.display_name = name.clone();
            }
            if let Some(description) = &block.description {
                entry.description = description.clone();
            }
            if let Some(tags) = &block.tags {
                entry.tags = tags.clone();
            }
            if let Some(queries) = &block.queries {
                entry.representative_queries = queries.clone();
            }
        }
    }
    for block in &config.entries {
        if !matched.contains(block.identifier.as_str()) {
            eprintln!(
                "warning, almanac.toml configures {}, which this run did not generate, so the block had no effect",
                block.identifier
            );
        }
    }

    let catalog = Catalog {
        host: Host {
            display_name: display_name.clone(),
            domain: domain.to_string(),
        },
        entries,
    };
    for note in &notes {
        eprintln!("{note}");
    }
    let catalog_bytes = catalog.canonical_bytes();
    write_file(&site.join(".well-known/ard.json"), &catalog_bytes)?;
    write_file(&site.join(".well-known/ai-catalog.json"), &catalog_bytes)?;
    eprintln!(
        "note, ai-catalog.json is written beside ard.json for the readers still on the \
         legacy path, and almanac never removes the legacy path on its own"
    );

    let baseline = Baseline {
        spec_version: SPEC_VERSION.into(),
        mcp: mcp_baseline,
        skills,
        artifacts,
        openapi,
        a2a,
        provenance,
    };
    // The deferred copies land now, with every input read
    // and planned and the run past every refusal.
    for (dest, bytes) in &pending_copies {
        write_file(dest, bytes)?;
    }
    write_file(&site.join("baseline.json"), &pretty_bytes(&baseline))?;
    Ok(())
}

/// A name that becomes a path segment or an identifier
/// segment under the site (a skill name becomes a directory, a
/// document stem becomes part of a URN) must be one plain
/// segment. The inputs are untrusted, and a name carrying path
/// separators would write outside the site directory.
fn valid_plain_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains('/') && !name.contains('\\')
}

/// Generation writes the card from the live surface. When a card
/// already exists and disagrees with the surface on a shared
/// field, the design record's precedence rules ask for a warning
/// naming both values, not a silent overwrite.
fn warn_on_card_disagreement(
    site: &Path,
    new_card: &serde_json::Value,
    snapshot: &SurfaceSnapshot,
) {
    let path = site.join(".well-known/mcp/server-card.json");
    let Ok(existing) = read_json::<serde_json::Value>(&path) else {
        return;
    };
    // Identity is compared through the shared reduction, so an
    // existing card in the earlier shape and the new card in
    // the Final shape disagree only when the surface they
    // describe actually differs, never on the shape itself.
    // Tools are compared against the live snapshot, because
    // the new card no longer lists any: the warning's subject
    // was always the existing card against the live surface.
    let old = card_fields(&existing);
    let new = card_fields(new_card);
    for (field, old_value, new_value) in [
        ("name", &old.server_name, &new.server_name),
        ("version", &old.server_version, &new.server_version),
        (
            "protocolVersion",
            &old.protocol_version,
            &new.protocol_version,
        ),
    ] {
        if old_value != new_value {
            eprintln!(
                "warning, the existing server card says {field} is {old_value}, the live surface says {new_value}"
            );
        }
    }
    if let Some(old_tools) = &old.tools {
        let old_names: Vec<&str> = old_tools.iter().map(|t| t.name.as_str()).collect();
        let new_names: Vec<&str> = snapshot.tools.iter().map(|t| t.name.as_str()).collect();
        if old_names != new_names {
            eprintln!(
                "warning, the existing server card lists tools {}, the live surface serves {}",
                old_names.join(", "),
                new_names.join(", ")
            );
        }
    }
}

/// The publisher's configuration file, almanac.toml at the site
/// root. It carries finding suppressions for check and the
/// publisher's editorial choices for generate: a host display
/// name and per entry overrides and suppressions, DESIGN rule
/// 4's first source and rule 1's recorded suppression.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct AlmanacConfig {
    #[serde(default)]
    suppress: Vec<Suppression>,
    #[serde(default)]
    host: Option<HostConfig>,
    #[serde(default)]
    entries: Vec<EntryConfig>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HostConfig {
    display_name: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryConfig {
    identifier: String,
    display_name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
    queries: Option<Vec<String>>,
    suppress: Option<String>,
}

/// The entry block a configuration carries for one identifier,
/// if it carries one.
fn entry_block<'a>(config: &'a AlmanacConfig, identifier: &str) -> Option<&'a EntryConfig> {
    config
        .entries
        .iter()
        .find(|block| block.identifier == identifier)
}

/// Read the site's agents.json, the structured companion of
/// agents.txt, for the Amendment C cross check. Returns the MCP
/// endpoint URLs it names and a broken flag. No file at the site
/// root is (None, false), the cross check does not run. A file
/// that is present but is not an agents.json document, bad JSON,
/// an `mcp` that is not an array, an entry without a string
/// `url`, is (None, true), a broken public claim the check
/// reports rather than compares against. A document with no
/// `mcp` block names no endpoints, which is a claim too. The
/// shape is the agents.txt standard v1.0 section 5, the `url`
/// of each object in the top level `mcp` array.
fn read_agents_json(site: &Path) -> (Option<Vec<String>>, bool) {
    let path = site.join("agents.json");
    if !path.exists() {
        return (None, false);
    }
    let Ok(bytes) = fs::read(&path) else {
        return (None, true);
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return (None, true);
    };
    let Some(mcp) = value.get("mcp") else {
        return (Some(Vec::new()), false);
    };
    let Some(items) = mcp.as_array() else {
        return (None, true);
    };
    let mut urls = Vec::new();
    for item in items {
        match item.get("url").and_then(|v| v.as_str()) {
            Some(url) => urls.push(url.to_string()),
            None => return (None, true),
        }
    }
    (Some(urls), false)
}

/// Load the publisher's configuration. A suppression without a
/// reason, or with a malformed expiry, is a configuration error
/// and the command does not run, because a silent misreading
/// here would hold real findings back.
fn load_config(site: &Path) -> Result<AlmanacConfig, String> {
    let path = site.join("almanac.toml");
    if !path.exists() {
        return Ok(AlmanacConfig::default());
    }
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("could not read {} with {e}", path.display()))?;
    let config: AlmanacConfig = toml::from_str(&text)
        .map_err(|e| format!("could not parse {} with {e}", path.display()))?;
    for suppression in &config.suppress {
        if suppression.reason.trim().is_empty() {
            return Err(format!(
                "a suppression of {} in almanac.toml carries no reason, and a suppression without a reason is not accepted",
                suppression.kind
            ));
        }
        if !valid_iso_date(&suppression.expires) {
            return Err(format!(
                "a suppression of {} in almanac.toml expires on {}, which is not a YYYY-MM-DD date",
                suppression.kind, suppression.expires
            ));
        }
    }
    Ok(config)
}

fn load_suppressions(site: &Path) -> Result<Vec<Suppression>, String> {
    Ok(load_config(site)?.suppress)
}

/// Validate the editorial entry blocks before a generation
/// run relies on them. A duplicate identifier, a block with
/// no identifier, an empty queries list, a suppression
/// without a reason, and a block that both suppresses and
/// overrides are configuration errors, because each can only
/// be a mistake about what the run will do.
fn validate_entry_blocks(config: &AlmanacConfig) -> Result<(), String> {
    if let Some(name) = config
        .host
        .as_ref()
        .and_then(|host| host.display_name.as_ref())
        && name.trim().is_empty()
    {
        return Err(
            "the host display name in almanac.toml is empty, and an empty name configures nothing"
                .into(),
        );
    }
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for block in &config.entries {
        if block.identifier.trim().is_empty() {
            return Err(
                "an entries block in almanac.toml carries no identifier, and a block without an identifier configures nothing"
                    .into(),
            );
        }
        if !seen.insert(block.identifier.as_str()) {
            return Err(format!(
                "almanac.toml configures {} twice, and duplicate blocks are not merged",
                block.identifier
            ));
        }
        for (field, value) in [
            ("display name", &block.display_name),
            ("description", &block.description),
        ] {
            if matches!(value, Some(word) if word.trim().is_empty()) {
                return Err(format!(
                    "almanac.toml configures an empty {field} for {}, and an empty word configures nothing",
                    block.identifier
                ));
            }
        }
        if let Some(reason) = &block.suppress {
            if reason.trim().is_empty() {
                return Err(format!(
                    "the suppression of {} in almanac.toml carries no reason, and a suppression without a reason is not accepted",
                    block.identifier
                ));
            }
            if block.display_name.is_some()
                || block.description.is_some()
                || block.tags.is_some()
                || block.queries.is_some()
            {
                return Err(format!(
                    "almanac.toml both suppresses and overrides {}, and a block must do one or the other",
                    block.identifier
                ));
            }
        }
        if matches!(&block.queries, Some(queries) if queries.is_empty()) {
            return Err(format!(
                "almanac.toml configures an empty queries list for {}, which would announce the entry with no representative queries",
                block.identifier
            ));
        }
        if let Some(queries) = &block.queries
            && !(2..=5).contains(&queries.len())
        {
            eprintln!(
                "warning, almanac.toml configures {} representative queries for {}, and the draft asks for two to five",
                queries.len(),
                block.identifier
            );
        }
        for (field, values) in [("queries", &block.queries), ("tags", &block.tags)] {
            if let Some(values) = values {
                let mut unique: std::collections::HashSet<&str> = std::collections::HashSet::new();
                if values.iter().any(|value| !unique.insert(value.as_str())) {
                    eprintln!(
                        "warning, almanac.toml configures duplicate {field} for {}, and the duplicates are published as written",
                        block.identifier
                    );
                }
            }
        }
    }
    Ok(())
}

fn valid_iso_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && text
            .chars()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        && (1..=12).contains(&text[5..7].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&text[8..10].parse::<u32>().unwrap_or(0))
}

/// Today's date in UTC, YYYY-MM-DD, from the system clock. The
/// civil conversion is the standard days-from-epoch algorithm,
/// kept here so the CLI needs no date crate for one string.
fn today_utc() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or(0);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02}")
}

/// Resolve a probe target from the published catalog when the
/// operator named none. The MCP entry's URL names its server
/// card, and a card the generator wrote carries the endpoint it
/// probed, so the endpoint comes from the catalog's own chain of
/// references. No discovery and no guessing at well known paths.
/// A card that cannot be read on this site resolves nothing, and
/// the run falls back to repo mode.
fn resolve_target(site: &Path, published: &Catalog) -> Option<ProbeTarget> {
    let entry = published
        .entries
        .iter()
        .find(|e| e.media_type == "application/mcp-server-card+json")?;
    let card_path = url_to_path(site, &published.host.domain, entry.url.as_deref()?)?;
    let card = read_json::<serde_json::Value>(&card_path).ok()?;
    let endpoint = card_fields(&card).endpoint?;
    Some(ProbeTarget::Http(endpoint))
}

/// Read the server card as the drift engine's declared evidence
/// view, when the run is a protected one.
fn read_card_view(site: &Path, published: &Catalog) -> Option<CardView> {
    let entry = published
        .entries
        .iter()
        .find(|e| e.media_type == "application/mcp-server-card+json")?;
    let card_path = url_to_path(site, &published.host.domain, entry.url.as_deref()?)?;
    let card = read_json::<serde_json::Value>(&card_path).ok()?;
    let fields = card_fields(&card);
    Some(CardView {
        server_name: fields.server_name,
        server_version: fields.server_version,
        tools_enumerated: fields.tools.is_some(),
        tools: fields
            .tools
            .map(|items| {
                items
                    .iter()
                    .map(|t| t.name.clone())
                    .filter(|name| !name.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// The printed check report. The core report's fields plus the
/// run's mode, serialized as one struct so the shape lives in
/// exactly one place.
#[derive(Serialize)]
struct PrintedReport<'a> {
    mode: &'a str,
    #[serde(flatten)]
    report: &'a Report,
}

/// The check command. Reads the site, probes the live surface
/// when the run has a target, prints the report and returns the
/// exit code. With no target named and none resolvable from the
/// catalog, the run is repo mode and every offline comparison
/// still runs. An Err is a check that could not run, exit 2 at
/// the caller.
pub fn check(site: &Path, target: Option<&ProbeTarget>, fail_at: Severity) -> Result<i32, String> {
    let ard_path = site.join(".well-known/ard.json");
    let legacy_path = site.join(".well-known/ai-catalog.json");
    let legacy_catalog: Option<Catalog> = if legacy_path.exists() {
        Some(read_json(&legacy_path)?)
    } else {
        None
    };
    let (published, legacy_only): (Catalog, bool) = if ard_path.exists() {
        (read_json(&ard_path)?, false)
    } else if let Some(legacy) = &legacy_catalog {
        (legacy.clone(), true)
    } else {
        return Err(format!(
            "could not read {} with no catalog on either path",
            ard_path.display()
        ));
    };
    let legacy_ref: Option<&Catalog> = if legacy_only {
        Some(&published)
    } else {
        legacy_catalog.as_ref()
    };
    let baseline: Baseline = read_json(&site.join("baseline.json"))?;
    let suppressions = load_suppressions(site)?;

    let resolved;
    let target = match target {
        Some(target) => Some(target),
        None => {
            resolved = resolve_target(site, &published);
            resolved.as_ref()
        }
    };
    let snapshot = target.map(run_probe);
    let card = match &snapshot {
        Some(snapshot) if matches!(snapshot.auth, AuthSurface::Protected(_)) => {
            read_card_view(site, &published)
        }
        _ => None,
    };

    let mut skill_hashes: Vec<(String, String)> = Vec::new();
    for skill in &baseline.skills {
        let path = published
            .entries
            .iter()
            .find(|e| e.identifier == skill.entry_id)
            .and_then(|entry| entry.url.as_deref())
            .and_then(|url| url_to_path(site, &published.host.domain, url));
        if let Some(path) = path
            && let Ok(bytes) = fs::read(&path)
        {
            skill_hashes.push((skill.entry_id.clone(), hash::sha256_hex(&bytes)));
        }
    }
    let mut artifact_digests: Vec<(String, Option<String>)> = Vec::new();
    for artifact in &baseline.artifacts {
        if let Some(path) = url_to_path(site, &published.host.domain, &artifact.url) {
            let digest = fs::read(&path).ok().map(|bytes| hash::sha256_hex(&bytes));
            artifact_digests.push((artifact.url.clone(), digest));
        }
    }
    // Skill files present on the site now, announced or not. A
    // file the baseline never recorded is an unannounced
    // capability, and only a directory walk sees it.
    let mut present_skill_ids: Vec<String> = Vec::new();
    if let Ok(read_dir) = fs::read_dir(site.join("skills")) {
        for entry in read_dir.flatten() {
            if entry.path().join("SKILL.md").exists()
                && let Some(name) = entry.file_name().to_str()
            {
                present_skill_ids.push(format!("urn:air:{}:skill:{name}", published.host.domain));
            }
        }
    }
    present_skill_ids.sort();

    // Amendment C. The catalog's MCP entries name server cards,
    // and each card carries the endpoint it describes, so the
    // catalog's announced endpoints resolve through the cards
    // the site serves. agents.json, when the site serves one,
    // names its endpoints directly.
    let mut catalog_endpoints: Vec<(String, String)> = Vec::new();
    for entry in &published.entries {
        if entry.media_type == "application/mcp-server-card+json"
            && let Some(url) = &entry.url
            && let Some(path) = url_to_path(site, &published.host.domain, url)
            && let Ok(bytes) = fs::read(&path)
            && let Ok(card) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(endpoint) = card_fields(&card).endpoint
        {
            catalog_endpoints.push((entry.identifier.clone(), endpoint));
        }
    }
    let (agents_json_endpoints, agents_json_broken) = read_agents_json(site);

    // The OpenAPI documents the baseline records, reduced now.
    // A document that is missing, not JSON, or no longer reads
    // as an OpenAPI document is reported broken with its
    // reason, never silently compared against.
    let mut openapi_views: Vec<(String, OpenApiView)> = Vec::new();
    let mut openapi_broken: Vec<(String, String)> = Vec::new();
    for api in &baseline.openapi {
        let Some(path) = url_to_path(site, &published.host.domain, &api.url) else {
            openapi_broken.push((
                api.entry_id.clone(),
                "its URL does not map into the site".into(),
            ));
            continue;
        };
        match fs::read(&path) {
            Err(_) => openapi_broken.push((
                api.entry_id.clone(),
                "the document is missing where its catalog entry points".into(),
            )),
            Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Err(e) => openapi_broken.push((
                    api.entry_id.clone(),
                    format!("the document no longer parses as JSON ({e})"),
                )),
                Ok(doc) => match reduce_openapi(&doc) {
                    Ok(view) => openapi_views.push((api.entry_id.clone(), view)),
                    Err(reason) => openapi_broken.push((api.entry_id.clone(), reason)),
                },
            },
        }
    }
    // The agent cards the baseline records, reduced now, with
    // the same broken-with-reason posture as the documents.
    let mut a2a_views: Vec<(String, A2aView)> = Vec::new();
    let mut a2a_broken: Vec<(String, String)> = Vec::new();
    for card in &baseline.a2a {
        let Some(path) = url_to_path(site, &published.host.domain, &card.url) else {
            a2a_broken.push((
                card.entry_id.clone(),
                "its URL does not map into the site".into(),
            ));
            continue;
        };
        match fs::read(&path) {
            Err(_) => a2a_broken.push((
                card.entry_id.clone(),
                "the card is missing where its catalog entry points".into(),
            )),
            Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Err(e) => a2a_broken.push((
                    card.entry_id.clone(),
                    format!("the card no longer parses as JSON ({e})"),
                )),
                Ok(doc) => match reduce_agent_card(&doc) {
                    Ok(view) => a2a_views.push((card.entry_id.clone(), view)),
                    Err(reason) => a2a_broken.push((card.entry_id.clone(), reason)),
                },
            },
        }
    }

    let today = today_utc();
    let findings = drift::check(&CheckInputs {
        published: &published,
        legacy: legacy_ref,
        legacy_only,
        baseline: &baseline,
        snapshot: snapshot.as_ref(),
        skill_hashes: &skill_hashes,
        artifact_digests: &artifact_digests,
        present_skill_ids: &present_skill_ids,
        card: card.as_ref(),
        agents_json_endpoints: agents_json_endpoints.as_deref(),
        agents_json_broken,
        catalog_endpoints: &catalog_endpoints,
        openapi_views: &openapi_views,
        openapi_broken: &openapi_broken,
        a2a_views: &a2a_views,
        a2a_broken: &a2a_broken,
        suppressions: &suppressions,
        today: &today,
    });
    let mode = match &snapshot {
        None => "repo",
        Some(snapshot) => match &snapshot.auth {
            AuthSurface::Protected(_) => "declared-evidence-only",
            AuthSurface::Open => "live",
        },
    };
    let report = Report::new(findings, fail_at);
    let printed = PrintedReport {
        mode,
        report: &report,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&printed).expect("the report always serializes")
    );
    if mode == "declared-evidence-only" {
        eprintln!("declared evidence only, the surface is protected and was not probed live");
    }
    Ok(drift::exit_code(&report.findings, fail_at))
}

/// The validate command over a site, reading the catalog pair.
/// Exit 1 when any finding is major or above, 0 when the
/// catalog is sound or only warns. An Err is a catalog that
/// could not be read, exit 2 at the caller.
pub fn validate_site(site: &Path) -> Result<i32, String> {
    let published: Catalog = read_json(&site.join(".well-known/ard.json"))?;
    let legacy_path = site.join(".well-known/ai-catalog.json");
    let legacy: Option<Catalog> = if legacy_path.exists() {
        Some(read_json(&legacy_path)?)
    } else {
        None
    };
    Ok(report_validation(&published, legacy.as_ref()))
}

/// The validate command over one catalog file.
pub fn validate_file(path: &Path) -> Result<i32, String> {
    let published: Catalog = read_json(path)?;
    Ok(report_validation(&published, None))
}

/// One entry migration could not verify, with the reason.
#[derive(Serialize)]
struct MigrationFailure {
    entry: String,
    reason: String,
}

/// The migration report. The entries carried, the entries that
/// failed verification with their reasons, and the notes naming
/// what migration changed on the entries it carried.
#[derive(Serialize)]
struct MigrationReport {
    mode: &'static str,
    carried: Vec<String>,
    failed: Vec<MigrationFailure>,
    notes: Vec<String>,
}

fn migration_failure(entry: &str, reason: &str) -> MigrationFailure {
    MigrationFailure {
        entry: entry.to_string(),
        reason: reason.to_string(),
    }
}

/// Carry one verified artifact entry. The artifact's bytes go
/// into the baseline, the entry's digest is recorded afresh
/// from those bytes, and provenance names the migration as the
/// producing input.
fn carry_artifact(
    carried: &mut Vec<Entry>,
    artifacts: &mut Vec<ArtifactBaseline>,
    provenance: &mut Vec<EntryProvenance>,
    entry: &Entry,
    url: &str,
    bytes: &[u8],
) {
    artifacts.push(ArtifactBaseline {
        entry_id: entry.identifier.clone(),
        url: url.to_string(),
        sha256: hash::sha256_hex(bytes),
    });
    let mut kept = entry.clone();
    kept.digest = Some(hash::digest_field(bytes));
    provenance.push(EntryProvenance {
        entry_id: entry.identifier.clone(),
        produced_by: "migration".into(),
        overruled: vec![],
    });
    carried.push(kept);
}

/// The migrate command, the adoption wedge for the publishers
/// stranded on the legacy path. The existing ai-catalog.json is
/// read as a claim, every entry is verified against the live
/// surface or the site's own artifacts, and the entries that
/// verify are written out as the dual path pair with a baseline
/// recorded from what verification actually saw, plus the CI
/// wiring that keeps the pair honest. Entries that fail are
/// reported with their reasons, never silently carried and
/// never silently dropped. Exit 0 when every entry verified,
/// exit 1 when some failed or none did, and an Err, exit 2 at
/// the caller, when migration could not run at all.
pub fn migrate_site(
    site: &Path,
    target: Option<&ProbeTarget>,
    ci_path: &Path,
    site_arg: &str,
) -> Result<i32, String> {
    let legacy_path = site.join(".well-known/ai-catalog.json");
    if !legacy_path.exists() {
        return Err(
            "the site carries no ai-catalog.json, so there is no legacy catalog to migrate".into(),
        );
    }
    if site.join(".well-known/ard.json").exists() {
        return Err(
            "the site already publishes ard.json, so it is not on the legacy path. \
             Run check to gate the pair it has"
                .into(),
        );
    }
    // The catalog is read as raw JSON first and its entries
    // parsed one by one, so one malformed entry is a reported
    // failure instead of a dead end for the whole file. Only a
    // host block that will not parse stops the run.
    let raw: serde_json::Value = read_json(&legacy_path)?;
    let host: Host =
        serde_json::from_value(raw.get("host").cloned().unwrap_or(serde_json::Value::Null))
            .map_err(|e| format!("the legacy catalog's host block did not parse with {e}"))?;
    let domain = host.domain.clone();
    let mut entries: Vec<Entry> = Vec::new();
    let mut failed: Vec<MigrationFailure> = Vec::new();
    if let Some(raw_entries) = raw.get("entries").and_then(|v| v.as_array()) {
        for (index, raw_entry) in raw_entries.iter().enumerate() {
            match serde_json::from_value::<Entry>(raw_entry.clone()) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    let label = raw_entry
                        .get("identifier")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| format!("entry {}", index + 1));
                    let reason = format!("the entry does not parse as a catalog entry with {e}");
                    failed.push(migration_failure(&label, &reason));
                }
            }
        }
    }
    let has_mcp = entries
        .iter()
        .any(|e| e.media_type == "application/mcp-server-card+json");

    // The server claim verifies against the live surface, or
    // against the server card when the surface is protected,
    // the same declared evidence path generate uses. A surface
    // that cannot be read does not stop the run. The server
    // entries fail with the reason, and the entries that verify
    // against the site alone are still judged, because an
    // unreachable announced server is a fact about the catalog,
    // not a failure of the tool.
    let mut card_source: Option<(Vec<u8>, Option<String>)> = None;
    let mut probed_target: Option<&ProbeTarget> = None;
    let mut probe_failure: Option<String> = None;
    let snapshot = if has_mcp {
        let named = target.ok_or(
            "the legacy catalog announces an MCP server, and migration verifies it \
             against the live surface. Name the surface with --url or --command",
        )?;
        probed_target = Some(named);
        let probe_snapshot = run_probe(named);
        if let Some(failure) = &probe_snapshot.failure {
            probe_failure = Some(format!(
                "the probe failed at {} with {}",
                failure.step, failure.detail
            ));
            None
        } else if matches!(probe_snapshot.auth, AuthSurface::Protected(_)) {
            match snapshot_from_card(site, named, &probe_snapshot) {
                Ok((built, bytes, description)) => {
                    card_source = Some((bytes, description));
                    Some(built)
                }
                Err(e) => {
                    probe_failure = Some(e);
                    None
                }
            }
        } else {
            Some(probe_snapshot)
        }
    } else {
        None
    };
    let card_bytes = snapshot.as_ref().map(|snap| match &card_source {
        Some((bytes, _)) => bytes.clone(),
        None => pretty_bytes(&card_value(
            snap,
            probed_target.expect("a snapshot implies a target"),
            &domain,
            None,
            None,
        )),
    });

    let mut carried: Vec<Entry> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    if !has_mcp {
        notes.push("the legacy catalog announces no MCP server, so no surface was probed".into());
    }
    let mut mcp_baseline: Option<McpBaseline> = None;
    let mut normative_card_path: Option<PathBuf> = None;
    let mut mcp_seen = false;
    let mut skill_baselines: Vec<SkillBaseline> = Vec::new();
    let mut artifacts: Vec<ArtifactBaseline> = Vec::new();
    let mut openapi_baselines: Vec<OpenApiBaseline> = Vec::new();
    let mut a2a_baselines: Vec<A2aBaseline> = Vec::new();
    let mut provenance: Vec<EntryProvenance> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in &entries {
        let id = entry.identifier.as_str();
        if !seen.insert(id.to_string()) {
            failed.push(migration_failure(
                id,
                "the catalog announces this identifier more than once, and only the first entry was judged",
            ));
            continue;
        }
        if entry.has_shape_error() {
            failed.push(migration_failure(
                id,
                "the entry carries both url and data, or neither",
            ));
            continue;
        }
        if entry.data.is_some() {
            failed.push(migration_failure(
                id,
                "an inlined entry makes a claim migration cannot check against the surface or the site, and migration carries only what it can verify",
            ));
            continue;
        }
        if !entry.anchored_to(&domain) {
            failed.push(migration_failure(
                id,
                "the entry URN is not anchored to the publisher domain",
            ));
            continue;
        }
        match entry.media_type.as_str() {
            "application/mcp-server-card+json" => {
                if mcp_seen {
                    failed.push(migration_failure(
                        id,
                        "the catalog announces more than one MCP server, and this build's baseline records one, so this entry was not migrated",
                    ));
                    continue;
                }
                mcp_seen = true;
                let Some(snap) = &snapshot else {
                    let reason = format!(
                        "{}, so the announced server claim does not verify",
                        probe_failure
                            .as_deref()
                            .unwrap_or("the surface could not be probed")
                    );
                    failed.push(migration_failure(id, &reason));
                    continue;
                };
                let segment = id.rsplit(':').next().unwrap_or_default();
                if segment != snap.server_name {
                    let reason = format!(
                        "the entry names {segment} and the probed surface calls itself {}, so the claim does not verify against this surface",
                        snap.server_name
                    );
                    failed.push(migration_failure(id, &reason));
                    continue;
                }
                if snap.capabilities_enumerated && snap.tools.is_empty() {
                    failed.push(migration_failure(
                        id,
                        "the live surface serves no tools, so the announced server claim does not verify",
                    ));
                    continue;
                }
                // A server card already in the site is a repo
                // artifact the entry makes a claim about. When
                // the entry announced a digest for it, the bytes
                // on disk must still match that digest.
                if let (Some(announced), Some(url)) = (&entry.digest, &entry.url)
                    && let Some(path) = url_to_path(site, &domain, url)
                    && let Ok(existing) = fs::read(&path)
                    && *announced != hash::digest_field(&existing)
                {
                    failed.push(migration_failure(
                        id,
                        "the server card in the site no longer matches the digest the catalog announced, so the entry does not verify",
                    ));
                    continue;
                }
                if card_source.is_none()
                    && let Some(named) = probed_target
                {
                    warn_on_card_disagreement(
                        site,
                        &card_value(snap, named, &domain, None, None),
                        snap,
                    );
                }
                // The capability and version claims are judged,
                // not just noted. Each one that fails is a
                // failure of the entry's claim and the migration
                // exits nonzero, while the entry itself carries
                // in its reconciled form, because the server is
                // real and dropping its entry would lose the
                // publisher's listing entirely.
                let live_names: Vec<String> = snap.tools.iter().map(|t| t.name.clone()).collect();
                // The capability claims are judged only when the
                // surface was enumerated. A protected surface read
                // from a Final shape card declares identity alone,
                // so its entry's tool claims have nothing to be
                // judged against and carry unverified, noted here,
                // rather than failing against an empty list.
                if snap.capabilities_enumerated {
                    for name in &entry.capabilities {
                        if !live_names.contains(name) {
                            let reason = format!(
                                "the entry announced {name}, which the surface no longer serves"
                            );
                            failed.push(migration_failure(id, &reason));
                        }
                    }
                    for name in &live_names {
                        if !entry.capabilities.contains(name) {
                            let reason = format!(
                                "the surface serves {name}, which the entry never announced"
                            );
                            failed.push(migration_failure(id, &reason));
                        }
                    }
                } else {
                    notes.push(format!(
                        "{id} announced tools the server card does not enumerate, so its tool claims were carried unverified"
                    ));
                }
                if let Some(announced) = &entry.version
                    && *announced != snap.server_version
                {
                    let reason = format!(
                        "the entry announced version {announced} and the surface serves {}",
                        snap.server_version
                    );
                    failed.push(migration_failure(id, &reason));
                }
                let bytes = card_bytes.as_ref().expect("card bytes for the MCP entry");
                let card_url = format!("https://{domain}/.well-known/mcp/server-card.json");
                let mut kept = entry.clone();
                kept.capabilities = if snap.capabilities_enumerated {
                    live_names
                } else {
                    entry.capabilities.clone()
                };
                if kept.url.as_deref() != Some(card_url.as_str()) {
                    notes.push(format!(
                        "{id} pointed at {}, and the migrated entry points at the server card the pair publishes",
                        entry.url.clone().unwrap_or_default()
                    ));
                    kept.url = Some(card_url.clone());
                }
                kept.digest = Some(hash::digest_field(bytes));
                if entry.version.is_none() {
                    notes.push(format!(
                        "{id} carried no version, and the migrated entry records the surface's version {}",
                        snap.server_version
                    ));
                }
                kept.version = Some(snap.server_version.clone());
                mcp_baseline = Some(McpBaseline::from_snapshot(id, snap));
                artifacts.push(ArtifactBaseline {
                    entry_id: id.to_string(),
                    url: card_url,
                    sha256: hash::sha256_hex(bytes),
                });
                // The normative card copy rides the same
                // binding, when the probed endpoint lives on
                // this site's origin.
                if let Some(target) = probed_target
                    && let Some(norm_url) = normative_card_url_for(target)
                {
                    match url_to_path(site, &domain, &norm_url) {
                        Some(path) => {
                            artifacts.push(ArtifactBaseline {
                                entry_id: id.to_string(),
                                url: norm_url,
                                sha256: hash::sha256_hex(bytes),
                            });
                            normative_card_path = Some(path);
                        }
                        None => notes.push(
                            "the endpoint is not on the site's origin, so the server card \
                             was published at the well known path only"
                                .into(),
                        ),
                    }
                }
                provenance.push(EntryProvenance {
                    entry_id: id.to_string(),
                    produced_by: if card_source.is_some() {
                        "server-card"
                    } else {
                        "live-probe"
                    }
                    .into(),
                    overruled: vec![],
                });
                carried.push(kept);
            }
            "text/markdown" => {
                let Some(url) = &entry.url else {
                    failed.push(migration_failure(
                        id,
                        "a skill entry with no url cannot be verified",
                    ));
                    continue;
                };
                let Some(path) = url_to_path(site, &domain, url) else {
                    failed.push(migration_failure(
                        id,
                        "the skill url is not on the publisher's own origin",
                    ));
                    continue;
                };
                let bytes = match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        failed.push(migration_failure(
                            id,
                            "the skill file the entry announces is not in the site",
                        ));
                        continue;
                    }
                    Err(e) => {
                        let reason = format!("the skill file could not be read with {e}");
                        failed.push(migration_failure(id, &reason));
                        continue;
                    }
                };
                let text = String::from_utf8_lossy(&bytes);
                let Ok((name, _)) = parse_skill_frontmatter(&text) else {
                    failed.push(migration_failure(
                        id,
                        "the skill file's frontmatter does not parse, so the entry does not verify",
                    ));
                    continue;
                };
                // The frontmatter is authoritative for its
                // entry, so a file declaring a different skill
                // does not verify this entry.
                let segment = id.rsplit(':').next().unwrap_or_default();
                if name != segment {
                    let reason = format!(
                        "the skill file declares the name {name} and the entry announces {segment}, so the entry does not verify"
                    );
                    failed.push(migration_failure(id, &reason));
                    continue;
                }
                if let Some(announced) = &entry.digest
                    && *announced != hash::digest_field(&bytes)
                {
                    failed.push(migration_failure(
                        id,
                        "the skill file no longer matches the digest the catalog announced, so the entry does not verify",
                    ));
                    continue;
                }
                skill_baselines.push(SkillBaseline {
                    entry_id: id.to_string(),
                    sha256: hash::sha256_hex(&bytes),
                });
                carry_artifact(
                    &mut carried,
                    &mut artifacts,
                    &mut provenance,
                    entry,
                    url,
                    &bytes,
                );
            }
            _ => {
                let url = entry.url.clone().unwrap_or_default();
                let Some(path) = url_to_path(site, &domain, &url) else {
                    failed.push(migration_failure(
                        id,
                        "the entry url is not on the publisher's own origin, and migration verifies against the site and the surface only",
                    ));
                    continue;
                };
                let bytes = match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        failed.push(migration_failure(
                            id,
                            "the artifact the entry announces is not in the site",
                        ));
                        continue;
                    }
                    Err(e) => {
                        let reason = format!("the artifact could not be read with {e}");
                        failed.push(migration_failure(id, &reason));
                        continue;
                    }
                };
                if let Some(announced) = &entry.digest
                    && *announced != hash::digest_field(&bytes)
                {
                    failed.push(migration_failure(
                        id,
                        "the artifact no longer matches the digest the catalog announced, so the entry does not verify",
                    ));
                    continue;
                }
                if entry.media_type == "application/openapi+json" {
                    // An OpenAPI entry is verified as the
                    // surface it is, not as a generic artifact,
                    // so a migrated document is gated the same
                    // way a generated one is. The document is
                    // reduced, its operation and version claims
                    // are judged the way the MCP entry's are,
                    // and the baseline records the reduction.
                    let view = match serde_json::from_slice::<serde_json::Value>(&bytes)
                        .map_err(|e| format!("the document does not parse as JSON with {e}"))
                        .and_then(|doc| reduce_openapi(&doc))
                    {
                        Ok(view) => view,
                        Err(reason) => {
                            let reason = format!(
                                "the document the entry announces cannot be read as an OpenAPI document, {reason}"
                            );
                            failed.push(migration_failure(id, &reason));
                            continue;
                        }
                    };
                    let doc_ops: Vec<String> =
                        view.operations.iter().map(|op| op.id.clone()).collect();
                    for name in &entry.capabilities {
                        if !doc_ops.contains(name) {
                            let reason = format!(
                                "the entry announced {name}, which the document does not serve"
                            );
                            failed.push(migration_failure(id, &reason));
                        }
                    }
                    for name in &doc_ops {
                        if !entry.capabilities.contains(name) {
                            let reason = format!(
                                "the document serves {name}, which the entry never announced"
                            );
                            failed.push(migration_failure(id, &reason));
                        }
                    }
                    if let Some(announced) = &entry.version
                        && *announced != view.version
                    {
                        let reason = format!(
                            "the entry announced version {announced} and the document carries {}",
                            view.version
                        );
                        failed.push(migration_failure(id, &reason));
                    }
                    let mut kept = entry.clone();
                    kept.capabilities = doc_ops;
                    kept.version = Some(view.version.clone());
                    kept.digest = Some(hash::digest_field(&bytes));
                    openapi_baselines.push(OpenApiBaseline::from_view(
                        id,
                        &url,
                        &hash::sha256_hex(&bytes),
                        &view,
                    ));
                    provenance.push(EntryProvenance {
                        entry_id: id.to_string(),
                        produced_by: "migration".into(),
                        overruled: vec![],
                    });
                    carried.push(kept);
                    continue;
                }
                if entry.media_type == "application/a2a-agent-card+json" {
                    // An A2A entry is verified as the surface
                    // it is, the way an OpenAPI entry is: the
                    // card is reduced, its version and skill
                    // claims are judged, and the baseline
                    // records the reduction.
                    let view = match serde_json::from_slice::<serde_json::Value>(&bytes)
                        .map_err(|e| format!("the card does not parse as JSON with {e}"))
                        .and_then(|doc| reduce_agent_card(&doc))
                    {
                        Ok(view) => view,
                        Err(reason) => {
                            let reason = format!(
                                "the card the entry announces cannot be read as an agent card, {reason}"
                            );
                            failed.push(migration_failure(id, &reason));
                            continue;
                        }
                    };
                    let card_skills: Vec<String> =
                        view.skills.iter().map(|s| s.id.clone()).collect();
                    for name in &entry.capabilities {
                        if !card_skills.contains(name) {
                            let reason = format!(
                                "the entry announced {name}, which the card does not serve"
                            );
                            failed.push(migration_failure(id, &reason));
                        }
                    }
                    for name in &card_skills {
                        if !entry.capabilities.contains(name) {
                            let reason =
                                format!("the card serves {name}, which the entry never announced");
                            failed.push(migration_failure(id, &reason));
                        }
                    }
                    if let Some(announced) = &entry.version
                        && *announced != view.version
                    {
                        let reason = format!(
                            "the entry announced version {announced} and the card carries {}",
                            view.version
                        );
                        failed.push(migration_failure(id, &reason));
                    }
                    let mut kept = entry.clone();
                    kept.capabilities = card_skills;
                    kept.version = Some(view.version.clone());
                    kept.digest = Some(hash::digest_field(&bytes));
                    a2a_baselines.push(A2aBaseline::from_view(
                        id,
                        &url,
                        &hash::sha256_hex(&bytes),
                        &view,
                    ));
                    provenance.push(EntryProvenance {
                        entry_id: id.to_string(),
                        produced_by: "migration".into(),
                        overruled: vec![],
                    });
                    carried.push(kept);
                    continue;
                }
                carry_artifact(
                    &mut carried,
                    &mut artifacts,
                    &mut provenance,
                    entry,
                    &url,
                    &bytes,
                );
            }
        }
    }

    let exit = if carried.is_empty() || !failed.is_empty() {
        1
    } else {
        0
    };

    if !carried.is_empty() {
        // The baseline lands before the pair, so an interrupted
        // run never leaves a published pair whose first check
        // cannot run for want of a baseline.
        if let (Some(bytes), Some(_)) = (&card_bytes, &mcp_baseline) {
            if site.join(".well-known/mcp/server-card.json").exists() {
                notes.push(
                    "an existing server card was replaced with the card migration produced".into(),
                );
            }
            write_file(&site.join(".well-known/mcp/server-card.json"), bytes)?;
            if let Some(path) = &normative_card_path {
                write_file(path, bytes)?;
            }
        }
        if site.join("baseline.json").exists() {
            notes.push(
                "an existing baseline.json was replaced with the baseline migration recorded"
                    .into(),
            );
        }
        let baseline = Baseline {
            spec_version: SPEC_VERSION.into(),
            mcp: mcp_baseline,
            skills: skill_baselines,
            artifacts,
            openapi: openapi_baselines,
            a2a: a2a_baselines,
            provenance,
        };
        write_file(&site.join("baseline.json"), &pretty_bytes(&baseline))?;
        let catalog = Catalog {
            host: host.clone(),
            entries: carried.clone(),
        };
        let pair = catalog.canonical_bytes();
        write_file(&site.join(".well-known/ard.json"), &pair)?;
        write_file(&site.join(".well-known/ai-catalog.json"), &pair)?;
        write_ci(ci_path, site_arg, probed_target, &mut notes)?;
        notes.push(
            "digests on the carried entries were recorded afresh from the artifacts as written"
                .into(),
        );
    }

    let report = MigrationReport {
        mode: "migrate",
        carried: carried.iter().map(|e| e.identifier.clone()).collect(),
        failed,
        notes,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("the report always serializes")
    );
    if card_source.is_some() {
        eprintln!("declared evidence only, the surface is protected and was not probed live");
    }
    Ok(exit)
}

/// The CI wiring migration emits, a workflow that installs
/// almanac from its repository and runs the gate, repo mode on
/// pull requests and the live check on a weekly schedule when
/// the surface was probed over HTTP. An existing file is never
/// overwritten.
fn write_ci(
    path: &Path,
    site_arg: &str,
    target: Option<&ProbeTarget>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if path.exists() {
        notes.push(format!(
            "CI wiring already exists at {}, and migration did not overwrite it",
            path.display()
        ));
        return Ok(());
    }
    // The workflow runs in CI, so an absolute site path from
    // the operator's machine is relativized against the working
    // directory when it can be, and falls back to the working
    // directory itself.
    let site_display = if Path::new(site_arg).is_absolute() {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| {
                Path::new(site_arg)
                    .strip_prefix(cwd)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| ".".to_string())
    } else {
        site_arg.to_string()
    };
    let live = match target {
        Some(ProbeTarget::Http(url)) => format!(
            "\n      - name: Live check\n        if: github.event_name == 'schedule'\n        run: almanac check --site {site_display} --url {url}\n"
        ),
        _ => String::new(),
    };
    let text = format!(
        "name: almanac\n\non:\n  pull_request:\n  schedule:\n    - cron: \"17 5 * * 1\"\n\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - name: Install almanac\n        run: cargo install --git https://github.com/david-engelmann/almanac almanac-cli --locked\n      - name: Repo mode check\n        run: almanac check --site {site_display}\n{live}"
    );
    write_file(path, text.as_bytes())?;
    notes.push(format!("CI wiring written to {}", path.display()));
    Ok(())
}

fn report_validation(published: &Catalog, legacy: Option<&Catalog>) -> i32 {
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let findings =
        almanac_core::validate::validate_catalog(published, legacy, &today_utc(), now_unix);
    let report = Report::new(findings, Severity::Major);
    let printed = PrintedReport {
        mode: "validate",
        report: &report,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&printed).expect("the report always serializes")
    );
    drift::exit_code(&report.findings, Severity::Major)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_cards_description_is_cut_at_one_hundred_characters() {
        // The Final caps a card description at 100 characters.
        // A server with a long name must still produce a card
        // inside the cap, so the synthesized description is
        // cut, and this test fails if the cut is removed.
        let snapshot = SurfaceSnapshot {
            transport: "stdio".into(),
            server_name: "a".repeat(150),
            server_version: "0.1.0".into(),
            protocol_version: "2025-06-18".into(),
            stateless_shape: false,
            capabilities_enumerated: true,
            tools: vec![],
            resources: vec![],
            prompts: vec![],
            auth: AuthSurface::Open,
            failure: None,
        };
        let card = card_value(
            &snapshot,
            &ProbeTarget::Stdio(vec!["server".into()]),
            "example.com",
            None,
            None,
        );
        let description = card["description"].as_str().expect("description");
        assert_eq!(
            description.chars().count(),
            100,
            "description was {description}"
        );
        assert_eq!(
            card["name"].as_str(),
            Some(format!("com.example/{}", "a".repeat(150)).as_str())
        );
    }
}
