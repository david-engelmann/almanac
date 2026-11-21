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
use std::time::{SystemTime, UNIX_EPOCH};

use almanac_core::Severity;
use almanac_core::baseline::{
    ArtifactBaseline, Baseline, EntryProvenance, McpBaseline, SkillBaseline,
};
use almanac_core::catalog::{Catalog, Entry, Host};
use almanac_core::drift::{self, CardView, CheckInputs, Report, Suppression};
use almanac_core::hash;
use almanac_core::snapshot::{AuthSurface, SurfaceSnapshot};
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
    let rest = url.strip_prefix(&prefix)?;
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
    let mut card = serde_json::json!({
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
    // For an HTTP surface the card records the endpoint it was
    // probed at, so a later check with no target named can
    // resolve the endpoint from the catalog entry's card, the
    // way docs/HTTP-TRANSPORT.md asks, instead of guessing.
    if let ProbeTarget::Http(url) = target {
        card["endpoint"] = serde_json::json!(url);
    }
    warn_on_card_disagreement(site, &card);
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

    let mcp_baseline = McpBaseline::from_snapshot(&entry_id, &snapshot);
    let mut artifacts = vec![ArtifactBaseline {
        entry_id: entry_id.clone(),
        url: card_url,
        sha256: hash::sha256_hex(&card_bytes),
    }];
    let mut provenance = vec![EntryProvenance {
        entry_id: entry_id.clone(),
        produced_by: "live-probe".into(),
        overruled: vec![],
    }];
    let mut skills = Vec::new();

    if let Some(skill_path) = skill_path {
        let bytes = fs::read(skill_path)
            .map_err(|e| format!("could not read {} with {e}", skill_path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let (name, description) = parse_skill_frontmatter(&text)?;
        if !valid_skill_name(&name) {
            return Err(format!(
                "the SKILL.md name {name} is not a plain skill name, and generation will not write a path from it"
            ));
        }
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
    eprintln!(
        "note, ai-catalog.json is written beside ard.json for the readers still on the \
         legacy path, and almanac never removes the legacy path on its own"
    );

    let baseline = Baseline {
        spec_version: SPEC_VERSION.into(),
        mcp: Some(mcp_baseline),
        skills,
        artifacts,
        provenance,
    };
    write_file(&site.join("baseline.json"), &pretty_bytes(&baseline))?;
    Ok(())
}

/// A skill name becomes a directory name under the site, so it
/// must be one plain segment. Frontmatter is untrusted input,
/// and a name carrying path separators would write outside the
/// site directory.
fn valid_skill_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains('/') && !name.contains('\\')
}

/// Generation writes the card from the live surface. When a card
/// already exists and disagrees with the surface on a shared
/// field, the design record's precedence rules ask for a warning
/// naming both values, not a silent overwrite.
fn warn_on_card_disagreement(site: &Path, new_card: &serde_json::Value) {
    let path = site.join(".well-known/mcp/server-card.json");
    let Ok(existing) = read_json::<serde_json::Value>(&path) else {
        return;
    };
    for field in ["name", "version", "protocolVersion"] {
        let old = existing
            .get(field)
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let new = new_card
            .get(field)
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if old != new {
            eprintln!(
                "warning, the existing server card says {field} is {old}, the live surface says {new}"
            );
        }
    }
    let names = |card: &serde_json::Value| -> Vec<String> {
        card.get("tools")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (old_tools, new_tools) = (names(&existing), names(new_card));
    if old_tools != new_tools {
        eprintln!(
            "warning, the existing server card lists tools {}, the live surface serves {}",
            old_tools.join(", "),
            new_tools.join(", ")
        );
    }
}

/// The publisher's configuration file, almanac.toml at the site
/// root. Today it carries suppressions and nothing else.
#[derive(serde::Deserialize, Default)]
struct AlmanacConfig {
    #[serde(default)]
    suppress: Vec<Suppression>,
}

/// Load and validate the suppressions. A suppression without a
/// reason, or with a malformed expiry, is a configuration error
/// and the check does not run, because a silent misreading here
/// would hold real findings back.
fn load_suppressions(site: &Path) -> Result<Vec<Suppression>, String> {
    let path = site.join("almanac.toml");
    if !path.exists() {
        return Ok(Vec::new());
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
    Ok(config.suppress)
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
    let endpoint = card.get("endpoint")?.as_str()?;
    Some(ProbeTarget::Http(endpoint.to_string()))
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
    let tools = card
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Some(CardView {
        server_name: card
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        server_version: card
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        tools,
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
            artifact_digests.push((artifact.entry_id.clone(), digest));
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
