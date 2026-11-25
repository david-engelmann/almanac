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
use crate::baseline::{AuthBaseline, Baseline, CardDisplay, McpBaseline};
use crate::catalog::Catalog;
use crate::openapi::OpenApiView;
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
#[serde(deny_unknown_fields)]
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
    /// False when the card carries no tools array, the
    /// SEP-2127 Final shape. An unenumerated card is compared
    /// at identity level only, because it never claimed to
    /// list tools and reading the absence as an empty list
    /// would report every recorded tool as vanished.
    pub tools_enumerated: bool,
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
    /// Current artifact digests by artifact URL for artifacts
    /// the checker can reach, sha256 hex without the catalog
    /// field's prefix. The key is the URL, not the entry
    /// identifier, because one entry can announce several
    /// artifacts (the server card at two locations) and each is
    /// checked at its own URL. A `None` digest means the
    /// artifact maps to a place the checker reads and is not
    /// there. Artifacts with no entry in this list are not
    /// checked.
    pub artifact_digests: &'a [(String, Option<String>)],
    /// Display reductions of the server cards the checker
    /// read, keyed by the artifact URL they were read from,
    /// for the display comparison in the artifact pass.
    pub card_displays: &'a [(String, CardDisplay)],
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
    /// The current reduction of each OpenAPI document the
    /// baseline records, by entry identifier. A baselined
    /// document that could not be read or reduced is absent
    /// here and present in `openapi_broken` instead.
    pub openapi_views: &'a [(String, OpenApiView)],
    /// Baselined OpenAPI documents that are missing or no
    /// longer read as OpenAPI documents, by entry identifier,
    /// with the reason.
    pub openapi_broken: &'a [(String, String)],
    /// The current reduction of each A2A agent card the
    /// baseline records, by entry identifier. A baselined
    /// card that could not be read or reduced is absent here
    /// and present in `a2a_broken` instead.
    pub a2a_views: &'a [(String, crate::a2a::A2aView)],
    /// Baselined A2A agent cards that are missing or no
    /// longer read as agent cards, by entry identifier, with
    /// the reason.
    pub a2a_broken: &'a [(String, String)],
    pub suppressions: &'a [Suppression],
    /// Today's date, YYYY-MM-DD, for suppression expiry.
    pub today: &'a str,
    /// The media type the catalog itself was observed
    /// served with, in a run that could observe it. None
    /// when the run made no observation (repo mode, or
    /// the serving could not be reached), and then the
    /// class does not run: an unobserved serving is not
    /// evidence about the serving. An observed empty
    /// string means the answer carried no Content-Type.
    pub catalog_media_type: Option<&'a str>,
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
            .find(|(url, _)| url == &artifact.url)
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
                    // A server card whose substance is
                    // unchanged has not drifted in any way
                    // the gate fails on: when the baseline
                    // recorded the card's display fields and
                    // the served card's remainder still
                    // matches them, with the primary endpoint
                    // it names unchanged, the byte change is
                    // display only and reports at Minor
                    // instead of failing the digest binding.
                    let display_pair = if current != published_hex || current != &artifact.sha256 {
                        let recorded = baseline
                            .mcp
                            .as_ref()
                            .and_then(|mcp| mcp.card_display.as_ref());
                        let served = inputs
                            .card_displays
                            .iter()
                            .find(|(url, _)| url == &artifact.url)
                            .map(|(_, served)| served);
                        match (entry.media_type.as_str(), recorded, served) {
                            ("application/mcp-server-card+json", Some(recorded), Some(served))
                                if served.remainder_sha256 == recorded.remainder_sha256
                                    && served.endpoint == recorded.endpoint =>
                            {
                                Some((recorded, served))
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    // The class covers display FIELD changes.
                    // A byte change no recorded field explains
                    // (a trailing newline, a key reordering in
                    // the file) leaves the parsed card identical
                    // and keeps the digest Criticals: nothing a
                    // reader sees changed, so nothing here
                    // excuses the bytes.
                    let display_detail = display_pair.and_then(|(recorded, served)| {
                        let mut changed: Vec<&str> = Vec::new();
                        if served.title != recorded.title {
                            changed.push("title");
                        }
                        if served.description != recorded.description {
                            changed.push("description");
                        }
                        if served.icons != recorded.icons {
                            changed.push("icons");
                        }
                        if served.website_url != recorded.website_url {
                            changed.push("websiteUrl");
                        }
                        if served.remotes != recorded.remotes {
                            changed.push("remotes");
                        }
                        if changed.is_empty() {
                            None
                        } else {
                            Some(format!(
                                "the server card at {} changed only in display fields ({})",
                                artifact.url,
                                changed.join(", ")
                            ))
                        }
                    });
                    if let Some(detail) = display_detail {
                        findings.push(finding(
                            Severity::Minor,
                            "declared-display-changed",
                            Some(&artifact.entry_id),
                            detail,
                        ));
                    } else {
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
                    // A failure at the entry step, discover
                    // under the current revision or initialize
                    // behind it, means the announced server
                    // could not be reached at all. A failure
                    // further into the walk is a probe failure
                    // against a server that did answer.
                    let kind = if matches!(failure.step.as_str(), "initialize" | "server/discover")
                    {
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
                            // A protected surface the run walked
                            // live, on a credential the surface
                            // accepted, gives live evidence, and
                            // the live comparison runs on it. The
                            // auth comparison above has already
                            // read the declared posture the
                            // snapshot carries. Any other protected
                            // snapshot is declared evidence, and
                            // the card is its comparison.
                            if snapshot.walked_live() {
                                check_live_surface(baseline, snapshot, &mut findings);
                            } else if let Some(card) = inputs.card {
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

    check_catalog_media_type(inputs, &mut findings);

    // The OpenAPI comparison runs in every mode too. The
    // document is a file surface, read in repo mode and live
    // mode alike, and compared operation by operation.
    check_openapi(inputs, &mut findings);
    check_a2a(inputs, &mut findings);

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
/// The served media type, when the run observed it:
/// the catalog served as anything but its own media
/// type warns at Minor. Parameters are the same type
/// wearing a parameter, and media types compare
/// case-insensitively.
fn check_catalog_media_type(inputs: &CheckInputs, findings: &mut Vec<Finding>) {
    if let Some(observed) = inputs.catalog_media_type {
        let media = observed
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if media != "application/ard+json" {
            let detail = if observed.trim().is_empty() {
                "the catalog is served with no media type rather than application/ard+json"
                    .to_string()
            } else {
                format!(
                    "the catalog is served as {} rather than application/ard+json",
                    observed.trim()
                )
            };
            findings.push(finding(Severity::Minor, "catalog-media-type", None, detail));
        }
    }
}

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

/// The OpenAPI documents against the baseline, fact by fact.
/// The classes are the design record's: a removed operation
/// is major, an unannounced operation is critical, a new
/// required parameter is major, a dropped one is minor, a
/// description change is minor, and a change to the rest of
/// an operation object is major, the schema hash class in
/// document form. The three channels are compared
/// independently, so no change can mask another. At the
/// document level the version, the servers list, the
/// security posture and the components pool are compared
/// too. The catalog itself is compared against the baseline
/// in both directions: an entry announcing operations
/// generation never recorded, an entry the baseline records
/// that the catalog dropped, and a fabricated entry with no
/// baseline record of any kind are all divergence, the
/// phantom rule.
fn check_openapi(inputs: &CheckInputs, findings: &mut Vec<Finding>) {
    let published = inputs.published;
    let baseline = inputs.baseline;
    for entry in &published.entries {
        if entry.media_type == "application/openapi+json"
            && !baseline
                .openapi
                .iter()
                .any(|a| a.entry_id == entry.identifier)
            && !baseline
                .artifacts
                .iter()
                .any(|a| a.entry_id == entry.identifier)
        {
            findings.push(finding(
                Severity::Critical,
                "catalog-baseline-divergence",
                Some(&entry.identifier),
                "the catalog announces an OpenAPI entry the baseline does not record".into(),
            ));
        }
    }
    for api in &baseline.openapi {
        match published
            .entries
            .iter()
            .find(|e| e.identifier == api.entry_id)
        {
            Some(entry) => {
                let announced: std::collections::BTreeSet<&str> =
                    entry.capabilities.iter().map(|c| c.as_str()).collect();
                let recorded: std::collections::BTreeSet<&str> =
                    api.operations.iter().map(|op| op.id.as_str()).collect();
                if announced != recorded {
                    findings.push(finding(
                        Severity::Critical,
                        "catalog-baseline-divergence",
                        Some(&api.entry_id),
                        "the catalog's announced operations differ from the baseline recorded at generation"
                            .into(),
                    ));
                }
                // The entry's own pointer and version are
                // claims too, as in the A2A pass: an entry
                // edited to name a different document, or a
                // version the baseline never recorded,
                // diverges even when the recorded document
                // itself is untouched.
                if entry.url.as_deref() != Some(api.url.as_str()) {
                    findings.push(finding(
                        Severity::Critical,
                        "catalog-baseline-divergence",
                        Some(&api.entry_id),
                        "the catalog's OpenAPI entry points at a different document than the baseline recorded at generation"
                            .into(),
                    ));
                }
                if let Some(announced) = &entry.version
                    && *announced != api.version
                {
                    findings.push(finding(
                        Severity::Critical,
                        "catalog-baseline-divergence",
                        Some(&api.entry_id),
                        "the catalog's OpenAPI entry announces a version the baseline did not record"
                            .into(),
                    ));
                }
            }
            None => findings.push(finding(
                Severity::Critical,
                "catalog-baseline-divergence",
                Some(&api.entry_id),
                "the baseline records an OpenAPI entry the catalog no longer announces".into(),
            )),
        }
        if let Some((_, reason)) = inputs
            .openapi_broken
            .iter()
            .find(|(id, _)| id == &api.entry_id)
        {
            findings.push(finding(
                Severity::Critical,
                "openapi-document-unreadable",
                Some(&api.entry_id),
                format!(
                    "the OpenAPI document at {} cannot be read as one, {reason}",
                    api.url
                ),
            ));
            continue;
        }
        let Some((_, view)) = inputs
            .openapi_views
            .iter()
            .find(|(id, _)| id == &api.entry_id)
        else {
            continue;
        };
        if view.version != api.version {
            findings.push(finding(
                Severity::Major,
                "server-version-changed",
                Some(&api.entry_id),
                format!(
                    "the document's version changed from {} at generation to {}",
                    api.version, view.version
                ),
            ));
        }
        if view.servers != api.servers {
            findings.push(finding(
                Severity::Major,
                "openapi-servers-changed",
                Some(&api.entry_id),
                "the document's servers list differs from the baseline recorded at generation"
                    .into(),
            ));
        }
        if view.security_required != api.security_required {
            findings.push(finding(
                Severity::Major,
                "authentication-changed",
                Some(&api.entry_id),
                "the document's security posture changed since generation".into(),
            ));
        }
        if view.components_sha256 != api.components_sha256 {
            findings.push(finding(
                Severity::Major,
                "openapi-components-changed",
                Some(&api.entry_id),
                "the document's components changed since generation, and the gate cannot attribute the change to one operation without resolving references"
                    .into(),
            ));
        }
        let current_ops: std::collections::HashMap<&str, &crate::openapi::OperationView> = view
            .operations
            .iter()
            .map(|op| (op.id.as_str(), op))
            .collect();
        for recorded in &api.operations {
            match current_ops.get(recorded.id.as_str()) {
                None => findings.push(finding(
                    Severity::Major,
                    "openapi-operation-removed",
                    Some(&api.entry_id),
                    format!(
                        "operation {} is recorded in the baseline and the document no longer serves it",
                        recorded.id
                    ),
                )),
                Some(current) => {
                    // Three independent channels. A change in
                    // one never suppresses a finding in another.
                    let added: Vec<&String> = current
                        .required
                        .iter()
                        .filter(|name| !recorded.required.contains(name))
                        .collect();
                    if !added.is_empty() {
                        findings.push(finding(
                            Severity::Major,
                            "openapi-required-parameter-added",
                            Some(&api.entry_id),
                            format!(
                                "operation {} now requires parameter {} that the baseline does not record",
                                recorded.id,
                                added
                                    .iter()
                                    .map(|s| s.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    }
                    let dropped: Vec<&String> = recorded
                        .required
                        .iter()
                        .filter(|name| !current.required.contains(name))
                        .collect();
                    if !dropped.is_empty() {
                        findings.push(finding(
                            Severity::Minor,
                            "openapi-required-parameter-dropped",
                            Some(&api.entry_id),
                            format!(
                                "operation {} no longer requires parameter {} that the baseline records",
                                recorded.id,
                                dropped
                                    .iter()
                                    .map(|s| s.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    }
                    if crate::hash::sha256_str(&current.description) != recorded.description_sha256
                    {
                        findings.push(finding(
                            Severity::Minor,
                            "openapi-operation-description-changed",
                            Some(&api.entry_id),
                            format!(
                                "operation {} changed its description since the baseline",
                                recorded.id
                            ),
                        ));
                    }
                    if current.contract_sha256 != recorded.contract_sha256 {
                        findings.push(finding(
                            Severity::Major,
                            "openapi-operation-changed",
                            Some(&api.entry_id),
                            format!(
                                "operation {} changed its contract since the baseline, apart from its description and parameters",
                                recorded.id
                            ),
                        ));
                    }
                }
            }
        }
        for current in &view.operations {
            if !api.operations.iter().any(|op| op.id == current.id) {
                findings.push(finding(
                    Severity::Critical,
                    "openapi-operation-unannounced",
                    Some(&api.entry_id),
                    format!(
                        "operation {} is served by the document and the catalog never announced it",
                        current.id
                    ),
                ));
            }
        }
    }
}

/// The A2A pass. Each baselined agent card is compared
/// against the reduction of the card the site serves now,
/// channel by channel, and the phantom rule runs in both
/// directions between the catalog and the baseline.
fn check_a2a(inputs: &CheckInputs, findings: &mut Vec<Finding>) {
    let published = inputs.published;
    let baseline = inputs.baseline;
    for entry in &published.entries {
        if entry.media_type == "application/a2a-agent-card+json"
            && !baseline.a2a.iter().any(|a| a.entry_id == entry.identifier)
            && !baseline
                .artifacts
                .iter()
                .any(|a| a.entry_id == entry.identifier)
        {
            findings.push(finding(
                Severity::Critical,
                "catalog-baseline-divergence",
                Some(&entry.identifier),
                "the catalog announces an A2A entry the baseline does not record".into(),
            ));
        }
    }
    for card in &baseline.a2a {
        match published
            .entries
            .iter()
            .find(|e| e.identifier == card.entry_id)
        {
            Some(entry) => {
                let announced: std::collections::BTreeSet<&str> =
                    entry.capabilities.iter().map(|c| c.as_str()).collect();
                let recorded: std::collections::BTreeSet<&str> =
                    card.skills.iter().map(|s| s.id.as_str()).collect();
                if announced != recorded {
                    findings.push(finding(
                        Severity::Critical,
                        "catalog-baseline-divergence",
                        Some(&card.entry_id),
                        "the catalog's announced skills differ from the baseline recorded at generation"
                            .into(),
                    ));
                }
                // The entry's own pointer and version are
                // claims too: an entry edited to name a
                // different card, or a version the baseline
                // never recorded, diverges even when the
                // recorded card itself is untouched.
                if entry.url.as_deref() != Some(card.url.as_str()) {
                    findings.push(finding(
                        Severity::Critical,
                        "catalog-baseline-divergence",
                        Some(&card.entry_id),
                        "the catalog's A2A entry points at a different card than the baseline recorded at generation"
                            .into(),
                    ));
                }
                if let Some(announced) = &entry.version
                    && *announced != card.version
                {
                    findings.push(finding(
                        Severity::Critical,
                        "catalog-baseline-divergence",
                        Some(&card.entry_id),
                        "the catalog's A2A entry announces a version the baseline did not record"
                            .into(),
                    ));
                }
            }
            None => findings.push(finding(
                Severity::Critical,
                "catalog-baseline-divergence",
                Some(&card.entry_id),
                "the baseline records an A2A entry the catalog no longer announces".into(),
            )),
        }
        if let Some((_, reason)) = inputs
            .a2a_broken
            .iter()
            .find(|(id, _)| id == &card.entry_id)
        {
            findings.push(finding(
                Severity::Critical,
                "a2a-card-unreadable",
                Some(&card.entry_id),
                format!(
                    "the agent card at {} cannot be read as one, {reason}",
                    card.url
                ),
            ));
            continue;
        }
        let Some((_, view)) = inputs.a2a_views.iter().find(|(id, _)| id == &card.entry_id) else {
            continue;
        };
        if view.version != card.version {
            findings.push(finding(
                Severity::Major,
                "server-version-changed",
                Some(&card.entry_id),
                format!(
                    "the agent's version changed from {} at generation to {}",
                    card.version, view.version
                ),
            ));
        }
        if view.name != card.name {
            findings.push(finding(
                Severity::Major,
                "server-version-changed",
                Some(&card.entry_id),
                format!(
                    "the agent's name changed from {} at generation to {}",
                    card.name, view.name
                ),
            ));
        }
        if crate::hash::sha256_str(view.description.as_deref().unwrap_or_default())
            != card.description_sha256
        {
            findings.push(finding(
                Severity::Minor,
                "a2a-description-changed",
                Some(&card.entry_id),
                "the agent's description changed since generation".into(),
            ));
        }
        let current_interfaces: Vec<crate::baseline::InterfaceBaseline> = view
            .interfaces
            .iter()
            .map(crate::baseline::InterfaceBaseline::from_view)
            .collect();
        if current_interfaces != card.interfaces {
            findings.push(finding(
                Severity::Major,
                "a2a-interfaces-changed",
                Some(&card.entry_id),
                "the card's interfaces differ from the baseline recorded at generation".into(),
            ));
        }
        if view.security_required != card.security_required {
            findings.push(finding(
                Severity::Major,
                "authentication-changed",
                Some(&card.entry_id),
                "the card's security posture changed since generation".into(),
            ));
        }
        if view.capability_flags != card.capability_flags || view.extensions != card.extensions {
            findings.push(finding(
                Severity::Major,
                "a2a-capabilities-changed",
                Some(&card.entry_id),
                "the card's capability flags or declared extensions changed since generation"
                    .into(),
            ));
        }
        if view.default_input_modes != card.default_input_modes
            || view.default_output_modes != card.default_output_modes
        {
            findings.push(finding(
                Severity::Major,
                "a2a-modes-changed",
                Some(&card.entry_id),
                "the card's default input or output modes changed since generation".into(),
            ));
        }
        let current_skills: std::collections::HashMap<&str, &crate::a2a::A2aSkillView> =
            view.skills.iter().map(|s| (s.id.as_str(), s)).collect();
        for recorded in &card.skills {
            match current_skills.get(recorded.id.as_str()) {
                None => findings.push(finding(
                    Severity::Major,
                    "a2a-skill-removed",
                    Some(&card.entry_id),
                    format!(
                        "skill {} is recorded in the baseline and the card no longer serves it",
                        recorded.id
                    ),
                )),
                Some(current) => {
                    if crate::hash::sha256_str(&current.description) != recorded.description_sha256
                    {
                        findings.push(finding(
                            Severity::Minor,
                            "a2a-skill-description-changed",
                            Some(&card.entry_id),
                            format!(
                                "skill {} changed its description since the baseline",
                                recorded.id
                            ),
                        ));
                    }
                    if current.contract_sha256 != recorded.contract_sha256 {
                        findings.push(finding(
                            Severity::Major,
                            "a2a-skill-changed",
                            Some(&card.entry_id),
                            format!(
                                "skill {} changed since the baseline, apart from its description",
                                recorded.id
                            ),
                        ));
                    }
                }
            }
        }
        for current in &view.skills {
            if !card.skills.iter().any(|s| s.id == current.id) {
                findings.push(finding(
                    Severity::Critical,
                    "a2a-skill-unannounced",
                    Some(&card.entry_id),
                    format!(
                        "skill {} is served by the card and the catalog never announced it",
                        current.id
                    ),
                ));
            }
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
    // The comparison runs only on a baseline that
    // enumerated. A baseline recorded from a Final shape
    // card holds no tool record, and the catalog's claims
    // cannot diverge from a record that was never made.
    if mcp.capabilities_enumerated
        && let Some(entry) = published
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
    // The tool comparisons run only on a card that enumerates.
    // A Final shape card carries no tools array, and comparing
    // its silence against the recorded set would manufacture
    // a divergence the card never stated.
    if card.tools_enumerated {
        let card_tools: std::collections::BTreeSet<&str> =
            card.tools.iter().map(|t| t.as_str()).collect();
        // Against the baseline, both sides must have
        // enumerated: a baseline recorded from a Final shape
        // card holds no tool record for an enumerating card
        // to diverge from.
        if mcp.capabilities_enumerated {
            let recorded: std::collections::BTreeSet<&str> =
                mcp.tools.keys().map(|k| k.as_str()).collect();
            if card_tools != recorded {
                findings.push(finding(
                    Severity::Critical,
                    "card-divergence",
                    Some(&mcp.entry_id),
                    "the server card's tools differ from the baseline recorded at generation"
                        .into(),
                ));
            }
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

    // The primitive set comparisons run only on a baseline
    // that enumerated. A baseline recorded from a Final
    // shape card holds no tool, resource or prompt record,
    // and a live surface's sets cannot diverge from a
    // record that was never made. Identity and auth are
    // recorded either way; the name and version always
    // compare, and the protocol version compares between
    // live records, for the reason given below.
    if mcp.capabilities_enumerated {
        check_live_primitive_sets(mcp, snapshot, &live, findings);
    }

    // A baseline recorded from a server card holds the
    // card's declared protocol version, the first of its
    // supported list, not a negotiation, so a live walk's
    // negotiated version cannot diverge from it. The card
    // comparison never compared it either. Protocol
    // compares only between two live records, which the
    // entry's provenance distinguishes.
    let card_recorded = baseline
        .provenance
        .iter()
        .any(|record| record.entry_id == mcp.entry_id && record.produced_by == "server-card");
    if snapshot.server_name != mcp.server_name
        || snapshot.server_version != mcp.server_version
        || (!card_recorded && snapshot.protocol_version != mcp.protocol_version)
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
}

/// The live primitive set comparisons: tools in both
/// directions with their schema and description channels,
/// and the resource and prompt sets. Called only when the
/// baseline enumerated, so every comparison here reads a
/// record that was actually made.
fn check_live_primitive_sets(
    mcp: &McpBaseline,
    snapshot: &SurfaceSnapshot,
    live: &BTreeMap<&str, &crate::snapshot::ToolSurface>,
    findings: &mut Vec<Finding>,
) {
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
    use crate::baseline::{ArtifactBaseline, CardDisplay, McpBaseline};
    use crate::catalog::{Catalog, Entry, Host};
    use crate::snapshot::{CredentialOutcome, ToolSurface};

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
            card_displays: &[],
            present_skill_ids: &[],
            card: None,
            agents_json_endpoints: None,
            agents_json_broken: false,
            catalog_endpoints: &[],
            openapi_views: &[],
            openapi_broken: &[],
            a2a_views: &[],
            a2a_broken: &[],
            suppressions: &[],
            today: "2026-11-21",
            catalog_media_type: None,
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
                trust_manifests: vec![],
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
            capabilities_enumerated: true,
            tools,
            resources: vec![],
            prompts: vec![],
            auth: AuthSurface::Open,
            credential: CredentialOutcome::NotPresented,
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
            openapi: vec![],
            a2a: vec![],
            provenance: vec![],
        }
    }

    #[test]
    fn each_artifact_is_checked_at_its_own_url() {
        // One entry announcing two artifacts, the server card
        // at its two locations. The digest loop must compare
        // each artifact against the bytes at its own URL, so a
        // change to the second copy alone fires at that URL and
        // a healthy first copy is never accused in its place.
        let mut cat = catalog();
        cat.entries[0].digest = Some("sha256:aaaa".into());
        let mut base = baseline_for(&snapshot(vec![ping()]));
        base.artifacts = vec![
            ArtifactBaseline {
                entry_id: "urn:air:example.com:mcp:demo".into(),
                url: "https://example.com/.well-known/mcp/server-card.json".into(),
                sha256: "aaaa".into(),
            },
            ArtifactBaseline {
                entry_id: "urn:air:example.com:mcp:demo".into(),
                url: "https://example.com/mcp/server-card".into(),
                sha256: "aaaa".into(),
            },
        ];
        let digests = vec![
            (
                "https://example.com/.well-known/mcp/server-card.json".to_string(),
                Some("aaaa".to_string()),
            ),
            (
                "https://example.com/mcp/server-card".to_string(),
                Some("bbbb".to_string()),
            ),
        ];
        let mut inp = inputs(&cat, &base, None);
        inp.artifact_digests = &digests;
        let findings = check(&inp);
        assert_eq!(findings.len(), 2, "findings were {findings:?}");
        assert!(
            findings
                .iter()
                .all(|f| f.detail.contains("https://example.com/mcp/server-card")
                    && !f.detail.contains(".well-known")),
            "every finding must name the moved copy only, findings were {findings:?}"
        );
        // A missing second copy is a missing finding at its own
        // URL, and the intact first copy stays silent.
        let digests = vec![
            (
                "https://example.com/.well-known/mcp/server-card.json".to_string(),
                Some("aaaa".to_string()),
            ),
            ("https://example.com/mcp/server-card".to_string(), None),
        ];
        let mut inp = inputs(&cat, &base, None);
        inp.artifact_digests = &digests;
        let findings = check(&inp);
        assert_eq!(findings.len(), 1, "findings were {findings:?}");
        assert!(
            findings[0]
                .detail
                .contains("https://example.com/mcp/server-card"),
            "the missing finding must name the missing copy, findings were {findings:?}"
        );
    }

    #[test]
    fn a_display_only_card_change_warns_instead_of_failing_the_digest() {
        // The baseline recorded the card's display fields.
        // The served card's bytes changed (its digest misses
        // both bindings) but its remainder is the recorded
        // one and only its description moved: one Minor, no
        // Critical. A substantive change (a different
        // remainder) keeps the digest Criticals, and a
        // baseline with no display record does too.
        let url = "https://example.com/.well-known/mcp/server-card.json";
        let recorded = CardDisplay {
            title: Some("Demo".into()),
            description: Some("The demo server.".into()),
            icons: vec![],
            website_url: None,
            remotes: vec![],
            endpoint: None,
            remainder_sha256: "rest".into(),
        };
        let mut cat = catalog();
        cat.entries[0].digest = Some("sha256:aaaa".into());
        let mut base = baseline_for(&snapshot(vec![ping()]));
        base.mcp.as_mut().expect("mcp").card_display = Some(recorded.clone());
        base.artifacts = vec![ArtifactBaseline {
            entry_id: "urn:air:example.com:mcp:demo".into(),
            url: url.into(),
            sha256: "aaaa".into(),
        }];
        let digests = vec![(url.to_string(), Some("bbbb".to_string()))];

        let mut served = recorded.clone();
        served.description = Some("The demo server, described better.".into());
        let displays = vec![(url.to_string(), served)];
        let mut inp = inputs(&cat, &base, None);
        inp.artifact_digests = &digests;
        inp.card_displays = &displays;
        let findings = check(&inp);
        assert_eq!(findings.len(), 1, "findings were {findings:?}");
        assert_eq!(findings[0].severity, Severity::Minor);
        assert_eq!(findings[0].kind, "declared-display-changed");
        assert!(
            findings[0].detail.contains("description"),
            "the detail names the field, findings were {findings:?}"
        );

        // Same record, a remainder that moved: substance
        // changed, so the digest speaks at Critical.
        let mut substantive = recorded.clone();
        substantive.remainder_sha256 = "different".into();
        let displays = vec![(url.to_string(), substantive)];
        let mut inp = inputs(&cat, &base, None);
        inp.artifact_digests = &digests;
        inp.card_displays = &displays;
        let findings = check(&inp);
        assert_eq!(findings.len(), 2, "findings were {findings:?}");
        assert!(
            findings
                .iter()
                .all(|f| f.severity == Severity::Critical && f.kind == "digest-mismatch"),
            "findings were {findings:?}"
        );

        // A served reduction identical to the record, with
        // the digest still missing: the parsed card is the
        // recorded card and only its bytes moved (a
        // trailing newline, a key reordering), which no
        // display field explains, so the Criticals stand.
        let displays = vec![(url.to_string(), recorded.clone())];
        let mut inp = inputs(&cat, &base, None);
        inp.artifact_digests = &digests;
        inp.card_displays = &displays;
        let findings = check(&inp);
        assert_eq!(findings.len(), 2, "findings were {findings:?}");
        assert!(
            findings
                .iter()
                .all(|f| f.severity == Severity::Critical && f.kind == "digest-mismatch"),
            "findings were {findings:?}"
        );

        // No display record in the baseline: digest alone,
        // as before the class existed.
        let mut base = baseline_for(&snapshot(vec![ping()]));
        base.artifacts = vec![ArtifactBaseline {
            entry_id: "urn:air:example.com:mcp:demo".into(),
            url: url.into(),
            sha256: "aaaa".into(),
        }];
        let mut inp = inputs(&cat, &base, None);
        inp.artifact_digests = &digests;
        inp.card_displays = &displays;
        let findings = check(&inp);
        assert_eq!(findings.len(), 2, "findings were {findings:?}");
        assert!(
            findings
                .iter()
                .all(|f| f.severity == Severity::Critical && f.kind == "digest-mismatch"),
            "findings were {findings:?}"
        );
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
    fn a_catalog_served_with_a_generic_media_type_warns() {
        let base = baseline_for(&snapshot(vec![ping()]));
        let snap = snapshot(vec![ping()]);
        let cat = catalog();

        // A generic type observed: one Minor, nothing else.
        let mut inp = inputs(&cat, &base, Some(&snap));
        inp.catalog_media_type = Some("application/json");
        let findings = check(&inp);
        assert_eq!(findings.len(), 1, "findings were {findings:?}");
        assert_eq!(findings[0].severity, Severity::Minor);
        assert_eq!(findings[0].kind, "catalog-media-type");
        assert!(findings[0].detail.contains("application/json"));

        // The catalog's own type, with a parameter, is clean.
        let mut inp = inputs(&cat, &base, Some(&snap));
        inp.catalog_media_type = Some("application/ard+json; charset=utf-8");
        assert!(check(&inp).is_empty());

        // Media types compare case-insensitively.
        let mut inp = inputs(&cat, &base, Some(&snap));
        inp.catalog_media_type = Some("Application/ARD+JSON");
        assert!(check(&inp).is_empty());

        // An answer with no Content-Type observes as
        // empty and warns as no media type.
        let mut inp = inputs(&cat, &base, Some(&snap));
        inp.catalog_media_type = Some("");
        let findings = check(&inp);
        assert_eq!(findings.len(), 1, "findings were {findings:?}");
        assert!(findings[0].detail.contains("no media type"));

        // No observation: the class does not run.
        let inp = inputs(&cat, &base, Some(&snap));
        assert!(check(&inp).is_empty());
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

    // ------------------------------------------------------------
    // The OpenAPI comparison. The baseline records one document
    // with two operations; each test perturbs the current view.

    const API_ID: &str = "urn:air:example.com:openapi:billing";

    fn api_doc() -> serde_json::Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Billing", "version": "1.0.0", "description": "Charges."},
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

    fn api_setup() -> (Catalog, Baseline, crate::openapi::OpenApiView) {
        let view = crate::openapi::reduce_openapi(&api_doc()).expect("reduces");
        let mut base = baseline_for(&snapshot(vec![ping()]));
        base.openapi = vec![crate::baseline::OpenApiBaseline::from_view(
            API_ID,
            "https://example.com/billing.json",
            "00",
            &view,
        )];
        let mut cat = catalog();
        cat.entries.push(Entry {
            identifier: API_ID.into(),
            display_name: "Billing".into(),
            media_type: "application/openapi+json".into(),
            url: Some("https://example.com/billing.json".into()),
            data: None,
            description: "Charges.".into(),
            capabilities: view.operations.iter().map(|op| op.id.clone()).collect(),
            representative_queries: vec![],
            tags: vec![],
            version: Some("1.0.0".into()),
            digest: None,
            updated_at: None,
            trust_manifest: None,
            trust_manifests: vec![],
        });
        (cat, base, view)
    }

    fn kinds(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.kind.as_str()).collect()
    }

    const A2A_ID: &str = "urn:air:example.com:a2a:review-agent";

    fn agent_doc() -> serde_json::Value {
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

    fn a2a_setup() -> (Catalog, Baseline, crate::a2a::A2aView) {
        let view = crate::a2a::reduce_agent_card(&agent_doc()).expect("reduces");
        let mut base = baseline_for(&snapshot(vec![ping()]));
        base.a2a = vec![crate::baseline::A2aBaseline::from_view(
            A2A_ID,
            "https://example.com/review-agent.json",
            "00",
            &view,
        )];
        let mut cat = catalog();
        cat.entries.push(Entry {
            identifier: A2A_ID.into(),
            display_name: "Code Review Agent".into(),
            media_type: "application/a2a-agent-card+json".into(),
            url: Some("https://example.com/review-agent.json".into()),
            data: None,
            description: "Reviews code changes.".into(),
            capabilities: view.skills.iter().map(|s| s.id.clone()).collect(),
            representative_queries: vec![],
            tags: vec![],
            version: Some("1.2.0".into()),
            digest: None,
            updated_at: None,
            trust_manifest: None,
            trust_manifests: vec![],
        });
        (cat, base, view)
    }

    fn a2a_check(cat: &Catalog, base: &Baseline, doc: serde_json::Value) -> Vec<Finding> {
        let view = crate::a2a::reduce_agent_card(&doc).expect("reduces");
        let views = vec![(A2A_ID.to_string(), view)];
        let mut with = inputs(cat, base, None);
        with.a2a_views = &views;
        check(&with)
    }

    #[test]
    fn a2a_agreement_produces_no_findings() {
        let (cat, base, _) = a2a_setup();
        assert!(a2a_check(&cat, &base, agent_doc()).is_empty());
    }

    #[test]
    fn a2a_removed_and_unannounced_skills_fire() {
        let (cat, base, _) = a2a_setup();
        let mut doc = agent_doc();
        doc["skills"]
            .as_array_mut()
            .expect("skills")
            .retain(|s| s["id"] != "summarize-repo");
        let findings = a2a_check(&cat, &base, doc);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "a2a-skill-removed" && f.severity == Severity::Major),
            "findings were {findings:?}"
        );
        let mut doc = agent_doc();
        doc["skills"].as_array_mut().expect("skills").push(
            serde_json::json!({"id": "delete-repo", "name": "Delete", "description": "Deletes.", "tags": []}),
        );
        let findings = a2a_check(&cat, &base, doc);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "a2a-skill-unannounced" && f.severity == Severity::Critical),
            "findings were {findings:?}"
        );
    }

    #[test]
    fn a2a_description_change_cannot_mask_a_contract_change() {
        let (cat, base, _) = a2a_setup();
        // A description edit alone is the minor channel only.
        let mut doc = agent_doc();
        doc["skills"][0]["description"] = serde_json::json!("Looks at a diff.");
        let findings = a2a_check(&cat, &base, doc);
        assert_eq!(
            kinds(&findings),
            vec!["a2a-skill-description-changed"],
            "findings were {findings:?}"
        );
        assert_eq!(findings[0].severity, Severity::Minor);
        // A contract edit wearing a description edit still
        // fires the major channel, and only it besides.
        let mut doc = agent_doc();
        doc["skills"][0]["description"] = serde_json::json!("Looks at a diff.");
        doc["skills"][0]["tags"] = serde_json::json!(["code", "review"]);
        let findings = a2a_check(&cat, &base, doc);
        assert_eq!(
            kinds(&findings),
            vec!["a2a-skill-changed", "a2a-skill-description-changed"],
            "findings were {findings:?}"
        );
        assert_eq!(findings[0].severity, Severity::Major);
        assert_eq!(findings[1].severity, Severity::Minor);
    }

    #[test]
    fn a2a_document_level_changes_fire() {
        let (cat, base, _) = a2a_setup();
        let mut doc = agent_doc();
        doc["version"] = serde_json::json!("2.0.0");
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"server-version-changed"));
        let mut doc = agent_doc();
        doc["name"] = serde_json::json!("Renamed Agent");
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"server-version-changed"));
        let mut doc = agent_doc();
        doc["description"] = serde_json::json!("Does something else now.");
        let findings = a2a_check(&cat, &base, doc);
        assert_eq!(kinds(&findings), vec!["a2a-description-changed"]);
        let mut doc = agent_doc();
        doc["securityRequirements"] = serde_json::json!([{"bearer": []}]);
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"authentication-changed"));
        let mut doc = agent_doc();
        doc["capabilities"]["pushNotifications"] = serde_json::json!(true);
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"a2a-capabilities-changed"));
        let mut doc = agent_doc();
        doc["capabilities"]["extensions"] =
            serde_json::json!([{"uri": "https://example.com/ext/pay", "required": true}]);
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"a2a-capabilities-changed"));
        let mut doc = agent_doc();
        doc["defaultInputModes"] = serde_json::json!(["application/json"]);
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"a2a-modes-changed"));
        let mut doc = agent_doc();
        doc["supportedInterfaces"][0]["url"] = serde_json::json!("https://example.com/review-v2");
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"a2a-interfaces-changed"));
        // A change to another field of the interface object
        // fires the same class through the rest hash.
        let mut doc = agent_doc();
        doc["supportedInterfaces"][0]["tenant"] = serde_json::json!("acme");
        assert!(kinds(&a2a_check(&cat, &base, doc)).contains(&"a2a-interfaces-changed"));
    }

    #[test]
    fn a2a_unreadable_card_and_phantom_rule_fire() {
        let (cat, base, _) = a2a_setup();
        let broken = vec![(A2A_ID.to_string(), "the card carries no name".to_string())];
        let mut with = inputs(&cat, &base, None);
        with.a2a_broken = &broken;
        let findings = check(&with);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "a2a-card-unreadable" && f.severity == Severity::Critical),
            "findings were {findings:?}"
        );

        // A baselined entry the catalog dropped.
        let (mut cat, base, view) = a2a_setup();
        cat.entries.retain(|e| e.identifier != A2A_ID);
        let views = vec![(A2A_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.a2a_views = &views;
        assert!(kinds(&check(&with)).contains(&"catalog-baseline-divergence"));

        // A published entry with no baseline record.
        let (cat, mut base, view) = a2a_setup();
        base.a2a.clear();
        let views = vec![(A2A_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.a2a_views = &views;
        assert!(kinds(&check(&with)).contains(&"catalog-baseline-divergence"));

        // An entry edited to point at another card, and one
        // edited to announce another version.
        let (mut cat, base, view) = a2a_setup();
        cat.entries
            .iter_mut()
            .find(|e| e.identifier == A2A_ID)
            .expect("entry")
            .url = Some("https://example.com/other.json".into());
        let views = vec![(A2A_ID.to_string(), view.clone())];
        let mut with = inputs(&cat, &base, None);
        with.a2a_views = &views;
        assert!(kinds(&check(&with)).contains(&"catalog-baseline-divergence"));
        let (mut cat, base, _) = a2a_setup();
        cat.entries
            .iter_mut()
            .find(|e| e.identifier == A2A_ID)
            .expect("entry")
            .version = Some("9.9.9".into());
        let views = vec![(A2A_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.a2a_views = &views;
        assert!(kinds(&check(&with)).contains(&"catalog-baseline-divergence"));
    }

    #[test]
    fn openapi_agreement_produces_no_findings() {
        let (cat, base, view) = api_setup();
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        assert!(check(&with).is_empty());
    }

    #[test]
    fn openapi_removed_and_unannounced_operations_fire() {
        let (cat, base, _) = api_setup();
        let mut doc = api_doc();
        doc["paths"]["/charges"]
            .as_object_mut()
            .expect("path item")
            .remove("get");
        doc["paths"]["/refunds"] = serde_json::json!({
            "post": {"operationId": "createRefund", "description": "Refund."}
        });
        let view = crate::openapi::reduce_openapi(&doc).expect("reduces");
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert!(kinds(&findings).contains(&"openapi-operation-removed"));
        assert!(kinds(&findings).contains(&"openapi-operation-unannounced"));
        let removed = findings
            .iter()
            .find(|f| f.kind == "openapi-operation-removed")
            .expect("removed finding");
        assert_eq!(removed.severity, Severity::Major);
        let added = findings
            .iter()
            .find(|f| f.kind == "openapi-operation-unannounced")
            .expect("unannounced finding");
        assert_eq!(added.severity, Severity::Critical);
    }

    #[test]
    fn openapi_required_parameter_and_description_changes_fire() {
        let (cat, base, _) = api_setup();
        let mut doc = api_doc();
        doc["paths"]["/charges"]["post"]["parameters"]
            .as_array_mut()
            .expect("params")
            .push(serde_json::json!({"name": "tenant", "in": "header", "required": true}));
        doc["paths"]["/charges"]["get"]["description"] =
            serde_json::json!("List every charge ever.");
        let view = crate::openapi::reduce_openapi(&doc).expect("reduces");
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert!(kinds(&findings).contains(&"openapi-required-parameter-added"));
        assert!(
            kinds(&findings).contains(&"openapi-operation-description-changed"),
            "kinds were {:?}",
            kinds(&findings)
        );
        // Neither edit touches the contract channel, which is
        // recorded apart from descriptions and parameters, so
        // the contract class stays silent on its own evidence.
        assert!(!kinds(&findings).contains(&"openapi-operation-changed"));
    }

    #[test]
    fn openapi_other_operation_change_and_broken_document_fire() {
        let (cat, base, _) = api_setup();
        let mut doc = api_doc();
        doc["paths"]["/charges"]["get"]["responses"] =
            serde_json::json!({"200": {"description": "ok"}});
        let view = crate::openapi::reduce_openapi(&doc).expect("reduces");
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert!(kinds(&findings).contains(&"openapi-operation-changed"));

        let broken = vec![(API_ID.to_string(), "the file is missing".to_string())];
        let mut with = inputs(&cat, &base, None);
        with.openapi_broken = &broken;
        let findings = check(&with);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "openapi-document-unreadable");
        assert_eq!(findings[0].severity, Severity::Critical);
    }

    #[test]
    fn openapi_description_change_cannot_mask_a_contract_change() {
        // The checkpoint red team's case: a wording tweak and a
        // contract change in the same edit to the same
        // operation. The channels are independent, so both
        // findings fire and the major one fails the gate.
        let (cat, base, _) = api_setup();
        let mut doc = api_doc();
        doc["paths"]["/charges"]["post"]["description"] = serde_json::json!("Create one charge.");
        doc["paths"]["/charges"]["post"]["responses"] =
            serde_json::json!({"201": {"description": "made"}});
        let view = crate::openapi::reduce_openapi(&doc).expect("reduces");
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert!(kinds(&findings).contains(&"openapi-operation-description-changed"));
        assert!(kinds(&findings).contains(&"openapi-operation-changed"));
    }

    #[test]
    fn openapi_dropped_required_parameter_is_minor_only() {
        let (cat, base, _) = api_setup();
        let mut doc = api_doc();
        doc["paths"]["/charges"]["post"]
            .as_object_mut()
            .expect("operation")
            .remove("parameters");
        let view = crate::openapi::reduce_openapi(&doc).expect("reduces");
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "openapi-required-parameter-dropped");
        assert_eq!(findings[0].severity, Severity::Minor);
    }

    #[test]
    fn openapi_document_level_changes_fire() {
        let (cat, base, _) = api_setup();
        let mut doc = api_doc();
        doc["info"]["version"] = serde_json::json!("2.0.0");
        doc["servers"] = serde_json::json!([{"url": "https://api.example.com/v2"}]);
        doc["security"] = serde_json::json!([{"apiKey": []}]);
        doc["components"] = serde_json::json!({"schemas": {"Charge": {"type": "object"}}});
        let view = crate::openapi::reduce_openapi(&doc).expect("reduces");
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        for kind in [
            "server-version-changed",
            "openapi-servers-changed",
            "authentication-changed",
            "openapi-components-changed",
        ] {
            assert!(
                kinds(&findings).contains(&kind),
                "missing {kind} in {:?}",
                kinds(&findings)
            );
        }
    }

    #[test]
    fn openapi_deleted_and_fabricated_entries_are_divergence() {
        let (mut cat, base, view) = api_setup();
        cat.entries.retain(|e| e.identifier != API_ID);
        let views = vec![(API_ID.to_string(), view.clone())];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "catalog-baseline-divergence");

        let (mut cat, base, view) = api_setup();
        let mut fabricated = cat
            .entries
            .iter()
            .find(|e| e.identifier == API_ID)
            .expect("api entry")
            .clone();
        fabricated.identifier = "urn:air:example.com:openapi:forged".into();
        cat.entries.push(fabricated);
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "catalog-baseline-divergence");
        assert_eq!(
            findings[0].entry.as_deref(),
            Some("urn:air:example.com:openapi:forged")
        );
    }

    #[test]
    fn openapi_phantom_capability_in_the_catalog_is_divergence() {
        let (mut cat, base, view) = api_setup();
        cat.entries
            .iter_mut()
            .find(|e| e.identifier == API_ID)
            .expect("api entry")
            .capabilities
            .push("deleteEverything".into());
        let views = vec![(API_ID.to_string(), view)];
        let mut with = inputs(&cat, &base, None);
        with.openapi_views = &views;
        let findings = check(&with);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "catalog-baseline-divergence");
        assert_eq!(findings[0].severity, Severity::Critical);
    }

    #[test]
    fn an_unenumerated_card_is_compared_at_identity_level_only() {
        // The baseline records a tool the catalog announces.
        // The card view names the same server and version but
        // enumerates nothing, the SEP-2127 Final shape. Read
        // as an empty tool list, it would diverge from both
        // the baseline and the catalog; read as unenumerated,
        // it must say nothing about tools at all. The same
        // view marked enumerated must diverge, so the guard
        // itself is what this test pins.
        let cat = catalog();
        let mut snap = snapshot(vec![ping()]);
        snap.auth = AuthSurface::Protected(Box::new(crate::snapshot::ProtectedMetadata {
            resource_metadata: None,
            authorization_servers: vec![],
        }));
        let base = baseline_for(&snap);
        let view = |enumerated: bool| CardView {
            server_name: "demo".into(),
            server_version: "0.1.0".into(),
            tools: vec![],
            tools_enumerated: enumerated,
        };
        let card = view(false);
        let mut with = inputs(&cat, &base, Some(&snap));
        with.card = Some(&card);
        let findings = check(&with);
        assert!(
            findings.iter().all(|f| f.kind != "card-divergence"),
            "an unenumerated card must not diverge on tools, findings were {findings:?}"
        );

        let enumerated = view(true);
        let mut with = inputs(&cat, &base, Some(&snap));
        with.card = Some(&enumerated);
        let findings = check(&with);
        assert!(
            findings.iter().any(|f| f.kind == "card-divergence"),
            "an enumerated card with no tools must diverge from the recorded tool, findings were {findings:?}"
        );
    }

    #[test]
    fn a_credentialed_snapshot_is_compared_live() {
        // A protected surface walked live on an accepted
        // credential: the baseline recorded the tool, the
        // walked surface no longer serves it, and the live
        // comparison must run on the walked lists even though
        // the snapshot's auth is Protected. The same snapshot
        // without the accepted credential is declared
        // evidence, and with no card in the inputs it compares
        // nothing, so the credential is what this test pins.
        let cat = catalog();
        let mut recorded = snapshot(vec![ping()]);
        recorded.auth = AuthSurface::Protected(Box::new(crate::snapshot::ProtectedMetadata {
            resource_metadata: None,
            authorization_servers: vec![],
        }));
        let base = baseline_for(&recorded);

        let mut walked = snapshot(vec![]);
        walked.auth = recorded.auth.clone();
        walked.credential = CredentialOutcome::Accepted;
        let with = inputs(&cat, &base, Some(&walked));
        let findings = check(&with);
        assert!(
            findings.iter().any(|f| f.kind == "announced-tool-missing"),
            "a credentialed walk must be compared live, findings were {findings:?}"
        );

        walked.credential = CredentialOutcome::NotPresented;
        let with = inputs(&cat, &base, Some(&walked));
        let findings = check(&with);
        assert!(
            findings.iter().all(|f| f.kind != "announced-tool-missing"),
            "an uncredentialed protected snapshot must not be compared live, findings were {findings:?}"
        );
    }

    #[test]
    fn a_card_recorded_protocol_version_is_not_compared() {
        // A baseline recorded from a server card holds the
        // card's declared protocol version, not a
        // negotiation. A credentialed walk negotiating a
        // different version must not read as identity
        // drift. The same pair recorded live must, so the
        // provenance gate is what this test pins.
        let cat = catalog();
        let mut recorded = snapshot(vec![ping()]);
        recorded.protocol_version = "2025-06-18".into();
        recorded.auth = AuthSurface::Protected(Box::new(crate::snapshot::ProtectedMetadata {
            resource_metadata: None,
            authorization_servers: vec![],
        }));
        let mut base = baseline_for(&recorded);
        base.provenance = vec![crate::baseline::EntryProvenance {
            entry_id: "urn:air:example.com:mcp:demo".into(),
            produced_by: "server-card".into(),
            overruled: vec![],
        }];
        let mut walked = snapshot(vec![ping()]);
        walked.protocol_version = "2026-07-28".into();
        walked.auth = recorded.auth.clone();
        walked.credential = CredentialOutcome::Accepted;
        let with = inputs(&cat, &base, Some(&walked));
        let findings = check(&with);
        assert!(
            findings.iter().all(|f| f.kind != "server-version-changed"),
            "a declared protocol version must not compare against a negotiated one, findings were {findings:?}"
        );

        base.provenance[0].produced_by = "live-probe".into();
        let with = inputs(&cat, &base, Some(&walked));
        let findings = check(&with);
        assert!(
            findings.iter().any(|f| f.kind == "server-version-changed"),
            "two live records with different protocol versions must compare, findings were {findings:?}"
        );
    }

    #[test]
    fn an_unenumerated_baseline_record_diverges_from_nothing() {
        // The mirror image: a baseline recorded from a Final
        // shape card holds no tool record, while the catalog
        // announces a tool the publisher's migration carried
        // unverified. The catalog cannot diverge from a
        // record that was never made, and an enumerating
        // card view has nothing recorded to diverge from
        // either. With the baseline's flag set back to
        // enumerated, the same inputs must diverge, so the
        // baseline side of the guard is what this test pins.
        let cat = catalog();
        let mut snap = snapshot(vec![]);
        snap.capabilities_enumerated = false;
        snap.auth = AuthSurface::Protected(Box::new(crate::snapshot::ProtectedMetadata {
            resource_metadata: None,
            authorization_servers: vec![],
        }));
        let mut base = baseline_for(&snap);
        assert!(!base.mcp.as_ref().expect("mcp").capabilities_enumerated);
        let card = CardView {
            server_name: "demo".into(),
            server_version: "0.1.0".into(),
            tools: vec!["ping".into()],
            tools_enumerated: true,
        };
        let mut with = inputs(&cat, &base, Some(&snap));
        with.card = Some(&card);
        let findings = check(&with);
        assert!(
            findings
                .iter()
                .all(|f| f.kind != "catalog-baseline-divergence" && f.kind != "card-divergence"),
            "an unenumerated baseline record must not diverge, findings were {findings:?}"
        );

        base.mcp.as_mut().expect("mcp").capabilities_enumerated = true;
        let mut with = inputs(&cat, &base, Some(&snap));
        with.card = Some(&card);
        let findings = check(&with);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "catalog-baseline-divergence"),
            "an enumerated baseline with no tools must diverge from the announced tool, findings were {findings:?}"
        );
    }
}
