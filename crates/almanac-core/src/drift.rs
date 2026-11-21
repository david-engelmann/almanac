//! The drift engine.
//!
//! Three sides. P is the published catalog, B is the baseline
//! recorded at generation time, L is the live surface in a snapshot.
//! Drift is any divergence among the three that a publisher would
//! want to know about before an agent discovers it for them. The
//! classes and severities are fixed by docs/DESIGN.md section 6.
//!
//! The engine is a pure function. The CLI gathers the inputs, file
//! bytes for the catalog pair and the baseline, current hashes for
//! the artifacts and skill files the site serves, and one snapshot
//! from a probe, and this module decides.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::Severity;
use crate::baseline::{AuthBaseline, Baseline};
use crate::catalog::Catalog;
use crate::snapshot::{AuthSurface, MetadataRecord, SurfaceSnapshot};

/// One divergence, classified.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub kind: String,
    pub detail: String,
}

fn finding(severity: Severity, kind: &str, detail: String) -> Finding {
    Finding {
        severity,
        kind: kind.to_string(),
        detail,
    }
}

/// Everything a check run compares, gathered by the CLI.
pub struct CheckInputs<'a> {
    /// The published catalog from the current path, ard.json.
    pub published: &'a Catalog,
    /// The published catalog from the legacy path, when present.
    pub legacy: Option<&'a Catalog>,
    pub baseline: &'a Baseline,
    /// The live probe of the MCP surface the catalog announces.
    pub snapshot: &'a SurfaceSnapshot,
    /// Current SKILL.md hashes by entry identifier, as served.
    pub skill_hashes: &'a [(String, String)],
    /// Current artifact digests by entry identifier for artifacts
    /// the checker can reach, sha256 hex without the catalog
    /// field's prefix. A `None` digest means the artifact maps to
    /// a place the checker reads and is not there. Artifacts with
    /// no entry in this list are not checked.
    pub artifact_digests: &'a [(String, Option<String>)],
}

/// Run every drift class over the inputs. Findings come back
/// sorted by severity, then kind, then detail, so the same three
/// sides always produce the same report.
pub fn check(inputs: &CheckInputs) -> Vec<Finding> {
    let mut findings = Vec::new();
    let published = inputs.published;
    let baseline = inputs.baseline;
    let snapshot = inputs.snapshot;

    // The dual path pair must agree, entry for entry.
    match inputs.legacy {
        Some(legacy) => {
            if legacy.entries != published.entries {
                findings.push(finding(
                    Severity::Critical,
                    "path-divergence",
                    "ard.json and ai-catalog.json entries differ".into(),
                ));
            }
        }
        None => findings.push(finding(
            Severity::Critical,
            "path-divergence",
            "ai-catalog.json is missing beside ard.json".into(),
        )),
    }

    // Entry shape and anchoring, from the catalog alone.
    for entry in &published.entries {
        if entry.has_shape_error() {
            findings.push(finding(
                Severity::Critical,
                "entry-shape",
                format!(
                    "entry {} carries both url and data, or neither",
                    entry.identifier
                ),
            ));
        }
        if !entry.anchored_to(&published.host.domain) {
            findings.push(finding(
                Severity::Critical,
                "urn-not-anchored",
                format!(
                    "entry {} is not anchored to publisher domain {}",
                    entry.identifier, published.host.domain
                ),
            ));
        }
    }

    // Digest binding. A referenced artifact whose served bytes no
    // longer match the digest in its entry is detectable from the
    // catalog alone.
    for artifact in &baseline.artifacts {
        let Some(entry) = published
            .entries
            .iter()
            .find(|e| e.identifier == artifact.entry_id)
        else {
            continue;
        };
        let Some(published_digest) = entry.digest.as_deref() else {
            continue;
        };
        let published_hex = published_digest
            .strip_prefix("sha256:")
            .unwrap_or(published_digest);
        if let Some((_, current)) = inputs
            .artifact_digests
            .iter()
            .find(|(id, _)| id == &artifact.entry_id)
        {
            match current {
                None => findings.push(finding(
                    Severity::Critical,
                    "digest-mismatch",
                    format!(
                        "artifact at {} is missing where its catalog entry points",
                        artifact.url
                    ),
                )),
                Some(current) if current != published_hex => findings.push(finding(
                    Severity::Critical,
                    "digest-mismatch",
                    format!(
                        "artifact at {} no longer matches the digest in its catalog entry",
                        artifact.url
                    ),
                )),
                _ => {}
            }
        }
    }

    // Skill files are file truth. A changed hash without
    // regeneration is major drift.
    for skill in &baseline.skills {
        match inputs
            .skill_hashes
            .iter()
            .find(|(id, _)| id == &skill.entry_id)
        {
            Some((_, current)) if current == &skill.sha256 => {}
            Some(_) => findings.push(finding(
                Severity::Major,
                "skill-content-changed",
                "SKILL.md content changed since the catalog was generated".into(),
            )),
            None => findings.push(finding(
                Severity::Major,
                "skill-content-changed",
                "SKILL.md announced in the baseline is missing".into(),
            )),
        }
    }

    if let Some(mcp) = &baseline.mcp {
        check_auth(&mcp.auth, &snapshot.auth, &mut findings);
        match &snapshot.failure {
            Some(failure) => {
                let kind = if failure.step == "initialize" {
                    "server-unreachable"
                } else {
                    "probe-failure"
                };
                findings.push(finding(
                    Severity::Critical,
                    kind,
                    format!(
                        "probe of the announced server failed at {} with {}",
                        failure.step, failure.detail
                    ),
                ));
            }
            None => match &snapshot.auth {
                AuthSurface::Protected(_) => {
                    check_declared_only(published, baseline, &mut findings);
                }
                AuthSurface::Open => {
                    check_live_surface(published, baseline, snapshot, &mut findings);
                }
            },
        }
    }

    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    findings
}

/// The live comparisons, run when the probe touched the surface.
fn check_live_surface(
    _published: &Catalog,
    baseline: &Baseline,
    snapshot: &SurfaceSnapshot,
    findings: &mut Vec<Finding>,
) {
    let mcp = baseline.mcp.as_ref().expect("checked by caller");
    let live: BTreeMap<&str, &crate::snapshot::ToolSurface> = snapshot
        .tools
        .iter()
        .map(|tool| (tool.name.as_str(), tool))
        .collect();

    for (name, recorded) in &mcp.tools {
        match live.get(name.as_str()) {
            None => findings.push(finding(
                Severity::Critical,
                "announced-tool-missing",
                format!("catalog announces tool {name}, live server does not serve it"),
            )),
            Some(tool) => {
                if crate::hash::sha256_json(&tool.input_schema) != recorded.schema_sha256 {
                    findings.push(finding(
                        Severity::Major,
                        "tool-schema-changed",
                        format!("tool {name} input schema changed since generation"),
                    ));
                }
                if crate::hash::sha256_str(&tool.description) != recorded.description_sha256 {
                    findings.push(finding(
                        Severity::Minor,
                        "tool-description-changed",
                        format!("tool {name} description changed since generation"),
                    ));
                }
            }
        }
    }
    for tool in &snapshot.tools {
        if !mcp.tools.contains_key(&tool.name) {
            findings.push(finding(
                Severity::Critical,
                "unannounced-tool-served",
                format!(
                    "live server serves tool {}, catalog never announced it",
                    tool.name
                ),
            ));
        }
    }

    if snapshot.server_name != mcp.server_name
        || snapshot.server_version != mcp.server_version
        || snapshot.protocol_version != mcp.protocol_version
    {
        findings.push(finding(
            Severity::Major,
            "server-version-changed",
            format!(
                "server identity changed since generation, was {} {} on {}, is {} {} on {}",
                mcp.server_name,
                mcp.server_version,
                mcp.protocol_version,
                snapshot.server_name,
                snapshot.server_version,
                snapshot.protocol_version
            ),
        ));
    }
    if snapshot.resources != mcp.resources {
        findings.push(finding(
            Severity::Major,
            "resource-set-changed",
            "the MCP resource set changed since generation".into(),
        ));
    }
    if snapshot.prompts != mcp.prompts {
        findings.push(finding(
            Severity::Major,
            "prompt-set-changed",
            "the MCP prompt set changed since generation".into(),
        ));
    }
}

/// The declared evidence comparisons, run when the surface is
/// protected and the probe could not touch it. The catalog against
/// the baseline is what can honestly be checked, and the report
/// labels the run declared evidence only.
fn check_declared_only(published: &Catalog, baseline: &Baseline, findings: &mut Vec<Finding>) {
    let mcp = baseline.mcp.as_ref().expect("checked by caller");
    if let Some(entry) = published
        .entries
        .iter()
        .find(|e| e.identifier == mcp.entry_id)
    {
        let announced: std::collections::BTreeSet<&str> =
            entry.capabilities.iter().map(|c| c.as_str()).collect();
        let recorded: std::collections::BTreeSet<&str> =
            mcp.tools.keys().map(|k| k.as_str()).collect();
        if announced != recorded {
            findings.push(finding(
                Severity::Critical,
                "catalog-baseline-divergence",
                "the catalog's announced tools differ from the baseline recorded at generation, on a surface that could not be probed live".into(),
            ));
        }
    }
}

/// The authentication comparisons, measured against the metadata
/// documents themselves rather than inferred from a handshake.
fn check_auth(recorded: &AuthBaseline, live: &AuthSurface, findings: &mut Vec<Finding>) {
    let changed = |field: &str, what: &str| {
        finding(
            Severity::Major,
            "authentication-changed",
            format!("authentication surface changed, {field} of {what} differs since generation"),
        )
    };
    match (recorded, live) {
        (AuthBaseline::Open, AuthSurface::Open) => {}
        (AuthBaseline::Open, AuthSurface::Protected(_))
        | (AuthBaseline::Protected(_), AuthSurface::Open) => findings.push(finding(
            Severity::Major,
            "authentication-changed",
            "the surface's authentication posture changed since generation".into(),
        )),
        (AuthBaseline::Protected(recorded), AuthSurface::Protected(live)) => {
            compare_metadata_slot(
                &recorded.resource_metadata,
                &live.resource_metadata,
                "resource metadata",
                findings,
                &changed,
            );
            let live_by_url: BTreeMap<&str, &MetadataRecord> = live
                .authorization_servers
                .iter()
                .map(|record| (record.url.as_str(), record))
                .collect();
            for record in &recorded.authorization_servers {
                match live_by_url.get(record.url.as_str()) {
                    None => findings.push(changed("presence", "an authorization server")),
                    Some(live_record) => {
                        compare_metadata_fields(record, live_record, findings, &changed)
                    }
                }
            }
            for record in &live.authorization_servers {
                if !recorded
                    .authorization_servers
                    .iter()
                    .any(|r| r.url == record.url)
                {
                    findings.push(changed("presence", "an authorization server"));
                }
            }
        }
    }
}

fn compare_metadata_slot(
    recorded: &Option<MetadataRecord>,
    live: &Option<MetadataRecord>,
    what: &str,
    findings: &mut Vec<Finding>,
    changed: &dyn Fn(&str, &str) -> Finding,
) {
    match (recorded, live) {
        (None, None) => {}
        (Some(_), None) | (None, Some(_)) => findings.push(changed("presence", what)),
        (Some(recorded), Some(live)) => compare_metadata_fields(recorded, live, findings, changed),
    }
}

fn compare_metadata_fields(
    recorded: &MetadataRecord,
    live: &MetadataRecord,
    findings: &mut Vec<Finding>,
    changed: &dyn Fn(&str, &str) -> Finding,
) {
    if recorded.sha256 == live.sha256 {
        return;
    }
    let what = "metadata document";
    if recorded.issuer != live.issuer {
        findings.push(changed("issuer", what));
    }
    if recorded.authorization_endpoint != live.authorization_endpoint {
        findings.push(changed("authorization_endpoint", what));
    }
    if recorded.token_endpoint != live.token_endpoint {
        findings.push(changed("token_endpoint", what));
    }
    if recorded.jwks_uri != live.jwks_uri {
        findings.push(changed("jwks_uri", what));
    }
    if recorded.scopes_supported != live.scopes_supported {
        findings.push(changed("scopes_supported", what));
    }
    if recorded.resource != live.resource {
        findings.push(changed("resource", what));
    }
}

/// The process exit code for a set of findings at a fail severity.
/// Exit 1 when any finding sits at or above it, else 0. Exit 2 is
/// the CLI's, for a check that could not run at all.
pub fn exit_code(findings: &[Finding], fail_at: Severity) -> i32 {
    if findings.iter().any(|f| f.severity >= fail_at) {
        1
    } else {
        0
    }
}

/// The deterministic report shape the CLI prints.
#[derive(Debug, Serialize)]
pub struct Report {
    pub counts: BTreeMap<String, usize>,
    pub fail_at: Severity,
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn new(findings: Vec<Finding>, fail_at: Severity) -> Self {
        let mut counts = BTreeMap::new();
        counts.insert("CRITICAL".to_string(), 0);
        counts.insert("MAJOR".to_string(), 0);
        counts.insert("MINOR".to_string(), 0);
        counts.insert("INFO".to_string(), 0);
        for finding in &findings {
            let key = match finding.severity {
                Severity::Critical => "CRITICAL",
                Severity::Major => "MAJOR",
                Severity::Minor => "MINOR",
            };
            *counts.get_mut(key).expect("count key exists") += 1;
        }
        Report {
            counts,
            fail_at,
            findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::{McpBaseline, ToolBaseline};
    use crate::catalog::{Catalog, Entry, Host};
    use crate::snapshot::ToolSurface;

    fn inputs<'a>(
        published: &'a Catalog,
        baseline: &'a Baseline,
        snapshot: &'a SurfaceSnapshot,
    ) -> CheckInputs<'a> {
        CheckInputs {
            published,
            legacy: Some(published),
            baseline,
            snapshot,
            skill_hashes: &[],
            artifact_digests: &[],
        }
    }

    fn catalog() -> Catalog {
        Catalog {
            host: Host {
                display_name: "Example".into(),
                domain: "example.com".into(),
            },
            entries: vec![Entry {
                identifier: "urn:air:example.com:mcp:demo".into(),
                display_name: "Demo".into(),
                media_type: "application/mcp-server-card+json".into(),
                url: Some("https://example.com/mcp".into()),
                data: None,
                description: "Demo.".into(),
                capabilities: vec!["ping".into()],
                representative_queries: vec![],
                tags: vec![],
                version: None,
                digest: None,
                updated_at: None,
            }],
        }
    }

    fn snapshot(tools: Vec<ToolSurface>) -> SurfaceSnapshot {
        SurfaceSnapshot {
            transport: "stdio".into(),
            server_name: "demo".into(),
            server_version: "0.1.0".into(),
            protocol_version: "2025-06-18".into(),
            tools,
            resources: vec![],
            prompts: vec![],
            auth: AuthSurface::Open,
            failure: None,
        }
    }

    fn ping() -> ToolSurface {
        ToolSurface {
            name: "ping".into(),
            description: "Ping.".into(),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    fn baseline_for(snapshot: &SurfaceSnapshot) -> Baseline {
        Baseline {
            spec_version: "ard/0.91".into(),
            mcp: Some(McpBaseline::from_snapshot(
                "urn:air:example.com:mcp:demo",
                snapshot,
            )),
            skills: vec![],
            artifacts: vec![],
        }
    }

    #[test]
    fn clean_surface_produces_no_findings() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let cat = catalog();
        assert!(check(&inputs(&cat, &base, &snap)).is_empty());
    }

    #[test]
    fn renamed_tool_fires_critical_in_both_directions() {
        let base = baseline_for(&snapshot(vec![ping()]));
        let mut renamed = ping();
        renamed.name = "pings".into();
        let snap = snapshot(vec![renamed]);
        let cat = catalog();
        let findings = check(&inputs(&cat, &base, &snap));
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.severity == Severity::Critical));
        assert!(findings.iter().any(|f| f.kind == "announced-tool-missing"));
        assert!(findings.iter().any(|f| f.kind == "unannounced-tool-served"));
    }

    #[test]
    fn schema_change_is_major_and_description_change_is_minor() {
        let base = baseline_for(&snapshot(vec![ping()]));
        let mut changed = ping();
        changed.input_schema = serde_json::json!({"type": "object", "properties": {"x": {}}});
        let snap = snapshot(vec![changed]);
        let cat = catalog();
        let findings = check(&inputs(&cat, &base, &snap));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "tool-schema-changed");
        assert_eq!(findings[0].severity, Severity::Major);

        let mut redescribed = ping();
        redescribed.description = "Ping, but louder.".into();
        let snap = snapshot(vec![redescribed]);
        let findings = check(&inputs(&cat, &base, &snap));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Minor);
    }

    #[test]
    fn baseline_tool_hashes_are_real() {
        let base = baseline_for(&snapshot(vec![ping()]));
        let tool: &ToolBaseline = base.mcp.as_ref().unwrap().tools.get("ping").unwrap();
        assert_ne!(tool.schema_sha256, tool.description_sha256);
    }
}
