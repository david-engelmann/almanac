//! Site reading, writing, and the generate and check commands.
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

use almanac_core::Severity;
use almanac_core::baseline::{ArtifactBaseline, Baseline, McpBaseline, SkillBaseline};
use almanac_core::catalog::{Catalog, Entry, Host};
use almanac_core::drift::{self, CheckInputs, Report};
use almanac_core::hash;
use almanac_core::snapshot::{AuthSurface, SurfaceSnapshot};

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
/// the URL belongs to the publisher's own origin.
fn url_to_path(site: &Path, domain: &str, url: &str) -> Option<PathBuf> {
    let prefix = format!("https://{domain}");
    let rest = url.strip_prefix(&prefix)?;
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

/// The generate command. Probes the live surface, writes the dual
/// path catalog, the server card and the baseline.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    site: &Path,
    domain: &str,
    display_name: &str,
    target: &ProbeTarget,
    skill_path: Option<&Path>,
    queries: &[String],
) -> Result<(), String> {
    let snapshot = run_probe(target);
    if let Some(failure) = &snapshot.failure {
        return Err(format!(
            "the probe failed at {} with {}, and generation needs a surface it can read",
            failure.step, failure.detail
        ));
    }
    if matches!(snapshot.auth, AuthSurface::Protected(_)) {
        return Err(
            "the surface is protected, so a live probe cannot read it. Generation for \
             authenticated servers runs from the server card and lands in a later milestone"
                .into(),
        );
    }

    let entry_id = format!("urn:air:{domain}:mcp:{}", snapshot.server_name);
    let card = serde_json::json!({
        "name": snapshot.server_name,
        "version": snapshot.server_version,
        "protocolVersion": snapshot.protocol_version,
        "transport": snapshot.transport,
        "tools": snapshot.tools.iter().map(|tool| serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "inputSchema": tool.input_schema,
        })).collect::<Vec<_>>(),
        "resources": snapshot.resources,
        "prompts": snapshot.prompts,
    });
    let card_bytes = pretty_bytes(&card);
    let card_url = format!("https://{domain}/.well-known/mcp/server-card.json");
    write_file(&site.join(".well-known/mcp/server-card.json"), &card_bytes)?;

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
    let mut entries = vec![Entry {
        identifier: entry_id.clone(),
        display_name: snapshot.server_name.clone(),
        media_type: "application/mcp-server-card+json".into(),
        url: Some(card_url.clone()),
        data: None,
        description: format!("The {} MCP server.", snapshot.server_name),
        capabilities: tool_names,
        representative_queries,
        tags: vec![],
        version: Some(snapshot.server_version.clone()),
        digest: Some(hash::digest_field(&card_bytes)),
        updated_at: None,
    }];

    let mut mcp_baseline = McpBaseline::from_snapshot(&entry_id, &snapshot);
    mcp_baseline.card_sha256 = Some(hash::sha256_hex(&card_bytes));
    let mut artifacts = vec![ArtifactBaseline {
        entry_id: entry_id.clone(),
        url: card_url,
        sha256: hash::sha256_hex(&card_bytes),
    }];
    let mut skills = Vec::new();

    if let Some(skill_path) = skill_path {
        let bytes = fs::read(skill_path)
            .map_err(|e| format!("could not read {} with {e}", skill_path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let (name, description) = parse_skill_frontmatter(&text)?;
        let skill_id = format!("urn:air:{domain}:skill:{name}");
        let skill_url = format!("https://{domain}/skills/{name}/SKILL.md");
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
        });
        skills.push(SkillBaseline {
            entry_id: skill_id.clone(),
            sha256: hash::sha256_hex(&bytes),
        });
        artifacts.push(ArtifactBaseline {
            entry_id: skill_id,
            url: skill_url,
            sha256: hash::sha256_hex(&bytes),
        });
    }

    let catalog = Catalog {
        host: Host {
            display_name: display_name.to_string(),
            domain: domain.to_string(),
        },
        entries,
    };
    let catalog_bytes = catalog.canonical_bytes();
    write_file(&site.join(".well-known/ard.json"), &catalog_bytes)?;
    write_file(&site.join(".well-known/ai-catalog.json"), &catalog_bytes)?;

    let baseline = Baseline {
        spec_version: SPEC_VERSION.into(),
        mcp: Some(mcp_baseline),
        skills,
        artifacts,
    };
    write_file(&site.join("baseline.json"), &pretty_bytes(&baseline))?;
    Ok(())
}

/// The check command. Reads the site, probes the live surface,
/// prints the deterministic report and returns the exit code.
/// An Err is a check that could not run, exit 2 at the caller.
pub fn check(site: &Path, target: &ProbeTarget, fail_at: Severity) -> Result<i32, String> {
    let published: Catalog = read_json(&site.join(".well-known/ard.json"))?;
    let legacy_path = site.join(".well-known/ai-catalog.json");
    let legacy: Option<Catalog> = if legacy_path.exists() {
        Some(read_json(&legacy_path)?)
    } else {
        None
    };
    let baseline: Baseline = read_json(&site.join("baseline.json"))?;
    let snapshot = run_probe(target);

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
            artifact_digests.push((artifact.entry_id.clone(), digest));
        }
    }

    let findings = drift::check(&CheckInputs {
        published: &published,
        legacy: legacy.as_ref(),
        baseline: &baseline,
        snapshot: &snapshot,
        skill_hashes: &skill_hashes,
        artifact_digests: &artifact_digests,
    });
    let mode = match &snapshot.auth {
        AuthSurface::Protected(_) => "declared-evidence-only",
        AuthSurface::Open => "live",
    };
    let report = Report::new(findings, fail_at);
    let rendered = serde_json::json!({
        "mode": mode,
        "counts": report.counts,
        "fail_at": report.fail_at,
        "findings": report.findings,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&rendered).expect("the report always serializes")
    );
    if mode == "declared-evidence-only" {
        eprintln!("declared evidence only, the surface is protected and was not probed live");
    }
    Ok(drift::exit_code(&report.findings, fail_at))
}
