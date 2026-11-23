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
//! the artifacts and skill files the site serves, one snapshot from
//! a probe when the run has a live target, and this module decides.
//! With no snapshot the run is repo mode, and every comparison that
//! needs no network still runs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Severity;
use crate::baseline::{AuthBaseline, Baseline};
use crate::catalog::Catalog;
use crate::snapshot::{AuthSurface, MetadataRecord, SurfaceSnapshot};

/// One divergence, classified.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub kind: String,
    /// The catalog entry the finding belongs to, when it belongs
    /// to one entry rather than to the catalog as a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    pub detail: String,
}

fn finding(severity: Severity, kind: &str, entry: Option<&str>, detail: String) -> Finding {
    Finding {
        severity,
        kind: kind.to_string(),
        entry: entry.map(|e| e.to_string()),
        detail,
    }
}

/// One suppression from the publisher's configuration. A finding
/// whose kind matches, and whose entry matches when the
/// suppression names one, is held back from the report. Every
/// suppression carries a reason and an expiry date, and an expired
/// suppression stops working and reports itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suppression {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    pub reason: String,
    /// ISO date, YYYY-MM-DD. String order is date order.
    pub expires: String,
}

/// The server card reduced to what declared evidence mode
/// compares, the card's own statement of the server's identity
/// and tools.
#[derive(Debug, Clone, PartialEq)]
pub struct CardView {
    pub server_name: String,
    pub server_version: String,
    pub tools: Vec<String>,
}

/// Everything a check run compares, gathered by the CLI.
pub struct CheckInputs<'a> {
    /// The published catalog from the current path, ard.json. In a
    /// legacy only site this is the legacy catalog itself.
    pub published: &'a Catalog,
    /// The published catalog from the legacy path, when present.
    pub legacy: Option<&'a Catalog>,
    /// True when the site serves only the legacy path.
    pub legacy_only: bool,
    pub baseline: &'a Baseline,
    /// The live probe of the MCP surface, when the run has a live
    /// target. Absent in repo mode.
    pub snapshot: Option<&'a SurfaceSnapshot>,
    /// Current SKILL.md hashes by entry identifier, as served.
    pub skill_hashes: &'a [(String, String)],
    /// Current artifact digests by entry identifier for artifacts
    /// the checker can reach, sha256 hex without the catalog
    /// field's prefix. A `None` digest means the artifact maps to
    /// a place the checker reads and is not there. Artifacts with
    /// no entry in this list are not checked.
    pub artifact_digests: &'a [(String, Option<String>)],
    /// Entry identifiers of skill files present on the site now,
    /// whether or not the baseline knows them.
    pub present_skill_ids: &'a [String],
    /// The server card as read now, for declared evidence runs.
    pub card: Option<&'a CardView>,
    /// The MCP endpoint URLs the site's agents.json names, when
    /// the site serves one the CLI could read. None when the site
    /// serves no agents.json, and then the cross check does not
    /// run. Amendment C, DESIGN.md section 7.
    pub agents_json_endpoints: Option<&'a [String]>,
    /// True when the site serves an agents.json that could not
    /// be read as one, so the cross check reports the broken
    /// claim instead of comparing against it.
    pub agents_json_broken: bool,
    /// The MCP endpoints the published catalog announces, by
    /// entry identifier, resolved from each entry's server card.
    /// An entry whose endpoint cannot be determined is absent,
    /// and its direction of the cross check is skipped.
    pub catalog_endpoints: &'a [(String, String)],
    pub suppressions: &'a [Suppression],
    /// Today's date, YYYY-MM-DD, for suppression expiry.
    pub today: &'a str,
}

/// Run every drift class over the inputs. Findings come back
/// sorted by severity, then kind, then entry, then detail, so the
/// same inputs always produce the same report.
pub fn check(inputs: &CheckInputs) -> Vec<Finding> {
    let mut findings = Vec::new();
    let published = inputs.published;
    let baseline = inputs.baseline;

    // The dual path pair must agree, entry for entry, and a site
    // on the legacy path alone is its own major class, the one
    // that makes check useful before migration.
    if inputs.legacy_only {
        findings.push(finding(
            Severity::Major,
            "legacy-only-catalog",
            None,
            "the catalog is served only on the legacy path, ai-catalog.json, with no ard.json beside it".into(),
        ));
    } else {
        match inputs.legacy {
            Some(legacy) => {
                if legacy.entries != published.entries {
                    findings.push(finding(
                        Severity::Critical,
                        "path-divergence",
                        None,
                        "ard.json and ai-catalog.json entries differ".into(),
                    ));
                }
            }
            None => findings.push(finding(
                Severity::Major,
                "path-divergence",
                None,
                "ai-catalog.json is missing beside ard.json".into(),
            )),
        }
    }

    // Entry shape and anchoring, from the catalog alone.
    for entry in &published.entries {
        if entry.has_shape_error() {
            findings.push(finding(
                Severity::Critical,
                "entry-shape",
                Some(&entry.identifier),
                "entry carries both url and data, or neither".into(),
            ));
        }
        if !entry.anchored_to(&published.host.domain) {
            findings.push(finding(
                Severity::Critical,
                "urn-not-anchored",
                Some(&entry.identifier),
                format!(
                    "entry is not anchored to publisher domain {}",
                    published.host.domain
                ),
            ));
        }
    }

    // Digest binding, three sided. The served bytes must match
    // both the digest in the published entry and the hash the
    // baseline recorded at generation, so a catalog edited to
    // match a tampered artifact still fires.
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
                    Some(&artifact.entry_id),
                    format!(
                        "artifact at {} is missing where its catalog entry points",
                        artifact.url
                    ),
                )),
                Some(current) => {
                    if current != published_hex {
                        findings.push(finding(
                            Severity::Critical,
                            "digest-mismatch",
                            Some(&artifact.entry_id),
                            format!(
                                "artifact at {} no longer matches the digest in its catalog entry",
                                artifact.url
                            ),
                        ));
                    }
                    if current != &artifact.sha256 {
                        findings.push(finding(
                            Severity::Critical,
                            "digest-mismatch",
                            Some(&artifact.entry_id),
                            format!(
                                "artifact at {} no longer matches the hash recorded in the baseline at generation",
                                artifact.url
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Skill files are file truth. A changed hash without
    // regeneration is major drift, and a skill file the baseline
    // never recorded is an unannounced capability.
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
                Some(&skill.entry_id),
                "SKILL.md content changed since the catalog was generated".into(),
            )),
            None => findings.push(finding(
                Severity::Major,
                "skill-content-changed",
                Some(&skill.entry_id),
                "SKILL.md announced in the baseline is missing".into(),
            )),
        }
    }
    for present in inputs.present_skill_ids {
        if !baseline.skills.iter().any(|s| &s.entry_id == present) {
            findings.push(finding(
                Severity::Critical,
                "unannounced-skill-served",
                Some(present),
                "a skill file is present that the catalog never announced".into(),
            ));
        }
    }

    if let Some(mcp) = &baseline.mcp {
        // The published catalog against the baseline runs in every
        // mode. The catalog is a claim, and a claim that disagrees
        // with the generation record is drift even when the live
        // surface agrees with one of them.
        check_catalog_against_baseline(published, baseline, &mut findings);

        if let Some(snapshot) = inputs.snapshot {
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
                        Some(&mcp.entry_id),
                        format!(
                            "probe of the announced server failed at {} with {}",
                            failure.step, failure.detail
                        ),
                    ));
                }
                None => {
                    // The auth comparison runs only on a completed
                    // probe. A failed probe says nothing about the
                    // surface's posture, and guessing at it would
                    // report a phantom change beside the real one.
                    check_auth(&mcp.auth, &snapshot.auth, &mut findings);
                    match &snapshot.auth {
                        AuthSurface::Protected(_) => {
                            if let Some(card) = inputs.card {
                                check_card(card, published, baseline, &mut findings);
                            }
                        }
                        AuthSurface::Open => {
                            check_live_surface(baseline, snapshot, &mut findings);
                        }
                    }
                }
            }
        }
    }

    // The agents.json cross check runs in every mode. It
    // compares two published claims, the catalog and the
    // neighbouring agents.json, and never touches the surface.
    check_agents_json(inputs, &mut findings);

    apply_suppressions(&mut findings, inputs.suppressions, inputs.today);

    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.entry.cmp(&b.entry))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    findings
}

/// Amendment C, the agents.json cross check. agents.json, the
/// structured companion of agents.txt, names a site's MCP
/// endpoints as a public claim beside the catalog's. Two public
/// claims about one surface should not disagree, so a mismatch
/// in either direction is a minor finding, per DESIGN.md
/// section 7. The schema is the agents.txt standard v1.0
/// section 5, read firsthand: the endpoints are the `url` of
/// each object in the top level `mcp` array.
///
/// The comparison is endpoint against endpoint. The catalog's
/// MCP entry names the server card, so the CLI resolves each
/// entry through its card's endpoint field and passes the
/// resolved pairs in. An entry whose endpoint cannot be
/// determined takes no part, and the agents.json direction is
/// judged only when the catalog announces no MCP entry at all
/// or at least one endpoint could be determined, so a stdio
/// card without an endpoint never produces a phantom mismatch.
/// Trailing slashes are normalized away; the two files are
/// written by different tools and a slash is not a claim.
fn check_agents_json(inputs: &CheckInputs, findings: &mut Vec<Finding>) {
    if inputs.agents_json_broken {
        findings.push(finding(
            Severity::Minor,
            "agents-json-unreadable",
            None,
            "the site serves an agents.json that could not be read as an \
             agents.json document, so its claims could not be cross \
             checked against the catalog"
                .into(),
        ));
        return;
    }
    let Some(agents_urls) = inputs.agents_json_endpoints else {
        return;
    };
    let norm = |url: &str| url.trim_end_matches('/').to_string();
    let agents_set: Vec<String> = agents_urls.iter().map(|u| norm(u)).collect();
    let catalog_set: Vec<String> = inputs
        .catalog_endpoints
        .iter()
        .map(|(_, endpoint)| norm(endpoint))
        .collect();
    let catalog_has_mcp = inputs
        .published
        .entries
        .iter()
        .any(|e| e.media_type == "application/mcp-server-card+json");
    if !catalog_has_mcp || !inputs.catalog_endpoints.is_empty() {
        for url in agents_urls {
            if !catalog_set.contains(&norm(url)) {
                findings.push(finding(
                    Severity::Minor,
                    "agents-json-mismatch",
                    None,
                    format!(
                        "agents.json names the MCP endpoint {url}, which \
                         the catalog does not announce"
                    ),
                ));
            }
        }
    }
    for (entry_id, endpoint) in inputs.catalog_endpoints {
        if !agents_set.contains(&norm(endpoint)) {
            findings.push(finding(
                Severity::Minor,
                "agents-json-mismatch",
                Some(entry_id),
                format!(
                    "the catalog announces the MCP endpoint {endpoint}, \
                     which agents.json does not name"
                ),
            ));
        }
    }
}

/// Suppressions hold findings back, and expired suppressions
/// report themselves at major severity.
fn apply_suppressions(findings: &mut Vec<Finding>, suppressions: &[Suppression], today: &str) {
    let mut expired: Vec<Finding> = Vec::new();
    let mut active: Vec<&Suppression> = Vec::new();
    for suppression in suppressions {
        if suppression.expires.as_str() < today {
            expired.push(finding(
                Severity::Major,
                "suppression-expired",
                suppression.entry.as_deref(),
                format!(
                    "the suppression of {} expired on {} and no longer holds findings back",
                    suppression.kind, suppression.expires
                ),
            ));
        } else {
            active.push(suppression);
        }
    }
    findings.retain(|f| {
        !active.iter().any(|s| {
            s.kind == f.kind
                && match (&s.entry, &f.entry) {
                    (None, _) => true,
                    (Some(want), Some(have)) => want == have,
                    (Some(_), None) => false,
                }
        })
    });
    findings.extend(expired);
}

/// The catalog's announced tools against the baseline's recorded
/// tools. Runs in every mode, because it needs no live surface.
fn check_catalog_against_baseline(
    published: &Catalog,
    baseline: &Baseline,
    findings: &mut Vec<Finding>,
) {
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
                Some(&mcp.entry_id),
                "the catalog's announced tools differ from the baseline recorded at generation"
                    .into(),
            ));
        }
    }
}

/// The card against the catalog and the baseline, the declared
/// evidence comparison for a protected surface.
fn check_card(
    card: &CardView,
    published: &Catalog,
    baseline: &Baseline,
    findings: &mut Vec<Finding>,
) {
    let mcp = baseline.mcp.as_ref().expect("checked by caller");
    let card_tools: std::collections::BTreeSet<&str> =
        card.tools.iter().map(|t| t.as_str()).collect();
    let recorded: std::collections::BTreeSet<&str> = mcp.tools.keys().map(|k| k.as_str()).collect();
    if card_tools != recorded {
        findings.push(finding(
            Severity::Critical,
            "card-divergence",
            Some(&mcp.entry_id),
            "the server card's tools differ from the baseline recorded at generation".into(),
        ));
    }
    if let Some(entry) = published
        .entries
        .iter()
        .find(|e| e.identifier == mcp.entry_id)
    {
        let announced: std::collections::BTreeSet<&str> =
            entry.capabilities.iter().map(|c| c.as_str()).collect();
        if card_tools != announced {
            findings.push(finding(
                Severity::Critical,
                "card-divergence",
                Some(&mcp.entry_id),
                "the server card's tools differ from the tools the catalog announces".into(),
            ));
        }
    }
    if card.server_name != mcp.server_name || card.server_version != mcp.server_version {
        findings.push(finding(
            Severity::Major,
            "card-divergence",
            Some(&mcp.entry_id),
            format!(
                "the server card's identity differs from the baseline, card says {} {}, baseline recorded {} {}",
                card.server_name, card.server_version, mcp.server_name, mcp.server_version
            ),
        ));
    }
}

/// The live comparisons, run when the probe touched the surface.
fn check_live_surface(
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
                Some(&mcp.entry_id),
                format!("catalog announces tool {name}, live server does not serve it"),
            )),
            Some(tool) => {
                if crate::hash::sha256_json(&tool.input_schema) != recorded.schema_sha256 {
                    findings.push(finding(
                        Severity::Major,
                        "tool-schema-changed",
                        Some(&mcp.entry_id),
                        format!("tool {name} input schema changed since generation"),
                    ));
                }
                if crate::hash::sha256_str(&tool.description) != recorded.description_sha256 {
                    findings.push(finding(
                        Severity::Minor,
                        "tool-description-changed",
                        Some(&mcp.entry_id),
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
                Some(&mcp.entry_id),
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
            Some(&mcp.entry_id),
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
            Some(&mcp.entry_id),
            "the MCP resource set changed since generation".into(),
        ));
    }
    if snapshot.prompts != mcp.prompts {
        findings.push(finding(
            Severity::Major,
            "prompt-set-changed",
            Some(&mcp.entry_id),
            "the MCP prompt set changed since generation".into(),
        ));
    }
}

/// The authentication comparisons, measured against the metadata
/// documents themselves rather than inferred from a handshake.
fn check_auth(recorded: &AuthBaseline, live: &AuthSurface, findings: &mut Vec<Finding>) {
    let changed = |field: &str, what: &str| {
        finding(
            Severity::Major,
            "authentication-changed",
            None,
            format!("authentication surface changed, {field} of {what} differs since generation"),
        )
    };
    match (recorded, live) {
        (AuthBaseline::Open, AuthSurface::Open) => {}
        (AuthBaseline::Open, AuthSurface::Protected(_))
        | (AuthBaseline::Protected(_), AuthSurface::Open) => findings.push(finding(
            Severity::Major,
            "authentication-changed",
            None,
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
    use crate::baseline::McpBaseline;
    use crate::catalog::{Catalog, Entry, Host};
    use crate::snapshot::ToolSurface;

    fn inputs<'a>(
        published: &'a Catalog,
        baseline: &'a Baseline,
        snapshot: Option<&'a SurfaceSnapshot>,
    ) -> CheckInputs<'a> {
        CheckInputs {
            published,
            legacy: Some(published),
            legacy_only: false,
            baseline,
            snapshot,
            skill_hashes: &[],
            artifact_digests: &[],
            present_skill_ids: &[],
            card: None,
            agents_json_endpoints: None,
            agents_json_broken: false,
            catalog_endpoints: &[],
            suppressions: &[],
            today: "2026-11-21",
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
                trust_manifest: None,
            }],
        }
    }

    fn snapshot(tools: Vec<ToolSurface>) -> SurfaceSnapshot {
        SurfaceSnapshot {
            transport: "stdio".into(),
            server_name: "demo".into(),
            server_version: "0.1.0".into(),
            protocol_version: "2025-06-18".into(),
            stateless_shape: false,
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
            provenance: vec![],
        }
    }

    #[test]
    fn clean_surface_produces_no_findings() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let cat = catalog();
        assert!(check(&inputs(&cat, &base, Some(&snap))).is_empty());
    }

    #[test]
    fn repo_mode_clean_produces_no_findings() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let cat = catalog();
        assert!(check(&inputs(&cat, &base, None)).is_empty());
    }

    #[test]
    fn phantom_tool_in_the_catalog_fires_in_live_mode() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let mut cat = catalog();
        cat.entries[0].capabilities.push("evil_tool".into());
        let findings = check(&inputs(&cat, &base, Some(&snap)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "catalog-baseline-divergence");
        assert_eq!(findings[0].severity, Severity::Critical);
    }

    #[test]
    fn renamed_tool_fires_critical_in_both_directions() {
        let base = baseline_for(&snapshot(vec![ping()]));
        let mut renamed = ping();
        renamed.name = "pings".into();
        let snap = snapshot(vec![renamed]);
        let cat = catalog();
        let findings = check(&inputs(&cat, &base, Some(&snap)));
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
        let findings = check(&inputs(&cat, &base, Some(&snap)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "tool-schema-changed");
        assert_eq!(findings[0].severity, Severity::Major);

        let mut redescribed = ping();
        redescribed.description = "Ping, but louder.".into();
        let snap = snapshot(vec![redescribed]);
        let findings = check(&inputs(&cat, &base, Some(&snap)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Minor);
    }

    #[test]
    fn entry_shape_and_urn_classes_fire() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let mut cat = catalog();
        cat.entries[0].data = Some(serde_json::json!({}));
        cat.entries[0].identifier = "urn:air:other.example:mcp:demo".into();
        let findings = check(&inputs(&cat, &base, Some(&snap)));
        assert!(findings.iter().any(|f| f.kind == "entry-shape"));
        assert!(findings.iter().any(|f| f.kind == "urn-not-anchored"));
    }

    #[test]
    fn suppression_holds_a_finding_and_expiry_fires() {
        let base = baseline_for(&snapshot(vec![ping()]));
        let mut changed = ping();
        changed.input_schema = serde_json::json!({"type": "object", "properties": {"x": {}}});
        let snap = snapshot(vec![changed]);
        let cat = catalog();
        let suppression = Suppression {
            kind: "tool-schema-changed".into(),
            entry: None,
            reason: "known and accepted".into(),
            expires: "2027-01-01".into(),
        };
        let mut with = inputs(&cat, &base, Some(&snap));
        with.suppressions = std::slice::from_ref(&suppression);
        assert!(check(&with).is_empty());

        let mut expired = suppression.clone();
        expired.expires = "2026-01-01".into();
        let mut with = inputs(&cat, &base, Some(&snap));
        with.suppressions = std::slice::from_ref(&expired);
        let findings = check(&with);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| f.kind == "suppression-expired"));
        assert!(findings.iter().any(|f| f.kind == "tool-schema-changed"));
    }

    #[test]
    fn agents_json_agreement_produces_no_findings() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let cat = catalog();
        let agents = vec!["https://example.com/mcp".to_string()];
        let announced = vec![(
            "urn:air:example.com:mcp:demo".to_string(),
            "https://example.com/mcp/".to_string(),
        )];
        let mut with = inputs(&cat, &base, None);
        with.agents_json_endpoints = Some(&agents);
        with.catalog_endpoints = &announced;
        assert!(check(&with).is_empty());
    }

    #[test]
    fn agents_json_mismatch_fires_in_both_directions() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let cat = catalog();
        let agents = vec!["https://example.com/other-mcp".to_string()];
        let announced = vec![(
            "urn:air:example.com:mcp:demo".to_string(),
            "https://example.com/mcp".to_string(),
        )];
        let mut with = inputs(&cat, &base, None);
        with.agents_json_endpoints = Some(&agents);
        with.catalog_endpoints = &announced;
        let findings = check(&with);
        assert_eq!(findings.len(), 2);
        assert!(
            findings
                .iter()
                .all(|f| f.kind == "agents-json-mismatch" && f.severity == Severity::Minor)
        );
        assert!(findings.iter().any(|f| f.entry.is_none()));
        assert!(
            findings
                .iter()
                .any(|f| f.entry.as_deref() == Some("urn:air:example.com:mcp:demo"))
        );
    }

    #[test]
    fn agents_json_endpoint_with_no_catalog_mcp_entry_fires() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let mut cat = catalog();
        cat.entries.clear();
        let agents = vec!["https://example.com/mcp".to_string()];
        let mut with = inputs(&cat, &base, None);
        with.agents_json_endpoints = Some(&agents);
        let findings = check(&with);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "agents-json-mismatch");
    }

    #[test]
    fn agents_json_unreadable_and_absent_cases() {
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let cat = catalog();
        let mut broken = inputs(&cat, &base, None);
        broken.agents_json_broken = true;
        let findings = check(&broken);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "agents-json-unreadable");
        assert_eq!(findings[0].severity, Severity::Minor);

        // No agents.json at all, the cross check does not run,
        // even with an announced endpoint it never names.
        let announced = vec![(
            "urn:air:example.com:mcp:demo".to_string(),
            "https://example.com/mcp".to_string(),
        )];
        let mut absent = inputs(&cat, &base, None);
        absent.catalog_endpoints = &announced;
        assert!(check(&absent).is_empty());
    }

    #[test]
    fn agents_json_skipped_when_no_endpoint_is_determinable() {
        // The catalog announces an MCP entry but its card carries
        // no endpoint (a stdio server), so the agents.json
        // direction cannot be judged and must not fire.
        let snap = snapshot(vec![ping()]);
        let base = baseline_for(&snap);
        let cat = catalog();
        let agents = vec!["https://example.com/mcp".to_string()];
        let mut with = inputs(&cat, &base, None);
        with.agents_json_endpoints = Some(&agents);
        assert!(check(&with).is_empty());
    }
}
