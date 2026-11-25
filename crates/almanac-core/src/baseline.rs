//! The baseline, the record a generation run leaves behind.
//!
//! The baseline is publisher property. It records what the server
//! served at generation time, it lives in the publisher's repo like
//! a lockfile, and it is the evidence every later check stands on.
//! It is never published as part of the catalog. The catalog stays
//! spec clean, the baseline carries the provenance and the hashes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::snapshot::{AuthSurface, SurfaceSnapshot};

/// The baseline file, written beside the catalog pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    /// The ARD spec version the catalog was generated against.
    pub spec_version: String,
    /// The MCP surface record, when the catalog announces one.
    pub mcp: Option<McpBaseline>,
    /// Skill entries by identifier, with the SKILL.md hash.
    pub skills: Vec<SkillBaseline>,
    /// Referenced artifacts by entry identifier, with the digest
    /// the catalog entry carries, for digest drift checks that run
    /// from the catalog alone.
    pub artifacts: Vec<ArtifactBaseline>,
    /// OpenAPI documents by entry identifier, with the operation
    /// facts recorded at generation. Baselines written before
    /// the generation roadmap's OpenAPI slice carry no such
    /// list, so the field defaults empty.
    #[serde(default)]
    pub openapi: Vec<OpenApiBaseline>,
    /// A2A agent cards by entry identifier, with the card
    /// facts recorded at generation. Baselines written before
    /// the generation roadmap's A2A slice carry no such list,
    /// so the field defaults empty.
    #[serde(default)]
    pub a2a: Vec<A2aBaseline>,
    /// Per entry provenance. Which input produced the entry, and
    /// which inputs it overruled on the way.
    pub provenance: Vec<EntryProvenance>,
    /// Trust manifest records by entry identifier, the
    /// envelope the catalog carried at generation time.
    /// Baselines written before the check integration
    /// slice carry no such list, so the field defaults
    /// empty, and their catalogs are judged without the
    /// removal class.
    #[serde(default)]
    pub trust_manifests: Vec<TrustManifestBaseline>,
}

/// What generation recorded about one entry's trust
/// manifests: the sha256 of each manifest's JCS
/// canonical bytes. The record exists so a later
/// check can see a manifest that was there and is
/// gone, the downgrade no content comparison catches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustManifestBaseline {
    /// The catalog entry the envelopes ride on.
    pub entry_id: String,
    /// The sha256 of each recorded envelope's JCS
    /// canonical bytes, identifying the envelope the
    /// baseline claims in the record.
    pub manifest_sha256: Vec<String>,
}

/// Which input produced one entry, and which inputs were
/// overruled. The design record's precedence rules make some
/// inputs win over others, and the baseline remembers the result
/// of each contest, so a later reader can see why an entry says
/// what it says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryProvenance {
    pub entry_id: String,
    /// The producing input, for example "live-probe" or
    /// "skill-file".
    pub produced_by: String,
    /// Inputs whose values for this entry lost to the winner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overruled: Vec<String>,
}

/// A server card's display record: the fields a reader
/// sees, kept apart from the card's substance so a check
/// can tell an editorial edit from a substantive one.
/// The remainder hash covers the card with the display
/// fields removed and its remotes order-normalized, so
/// two cards with the same remainder differ, if at all,
/// only in display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardDisplay {
    pub title: Option<String>,
    pub description: Option<String>,
    pub icons: Vec<String>,
    pub website_url: Option<String>,
    /// The card's remote URLs, in the order it lists
    /// them. Order is display here only while the
    /// primary endpoint below is unchanged; a remote's
    /// own content rides in the remainder.
    pub remotes: Vec<String>,
    /// The endpoint the card names as primary, the one
    /// the card reading resolves. A change here is
    /// substance, never display, however it is reached.
    pub endpoint: Option<String>,
    pub remainder_sha256: String,
}

/// What generation recorded about one MCP server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpBaseline {
    /// The catalog entry that announces this server.
    pub entry_id: String,
    pub server_name: String,
    pub server_version: String,
    pub protocol_version: String,
    /// Tool name to its recorded hashes.
    pub tools: BTreeMap<String, ToolBaseline>,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
    /// True when the recorded tool, resource and prompt
    /// lists are the surface's own enumeration. A baseline
    /// recorded from a SEP-2127 Final shape server card
    /// enumerates nothing, because the card lists no
    /// primitives, and comparisons that would read the
    /// empty record as a removal must not run against it.
    /// Baselines written before this field existed were
    /// recorded from probes and enumerating cards, so the
    /// field defaults to true for them.
    #[serde(default = "enumerated_by_default")]
    pub capabilities_enumerated: bool,
    /// The server card's display record, when generation
    /// had the card document in hand. Baselines written
    /// before the display comparison existed record none,
    /// and their cards are judged by digest alone, as
    /// they always were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_display: Option<CardDisplay>,
    /// The auth posture at generation time, with metadata records.
    pub auth: AuthBaseline,
}

fn enumerated_by_default() -> bool {
    true
}

/// One tool's recorded hashes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolBaseline {
    pub description_sha256: String,
    pub schema_sha256: String,
}

/// The auth posture at generation time, in baseline form. It
/// carries the same metadata payload the snapshot does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthBaseline {
    Open,
    Protected(Box<crate::snapshot::ProtectedMetadata>),
}

impl From<&AuthSurface> for AuthBaseline {
    fn from(surface: &AuthSurface) -> Self {
        match surface {
            AuthSurface::Open => AuthBaseline::Open,
            AuthSurface::Protected(metadata) => AuthBaseline::Protected(metadata.clone()),
        }
    }
}

/// One skill entry's recorded hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillBaseline {
    pub entry_id: String,
    pub sha256: String,
}

/// One referenced artifact's recorded digest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactBaseline {
    pub entry_id: String,
    pub url: String,
    /// The sha256 hex, without the catalog field's prefix.
    pub sha256: String,
}

/// One OpenAPI document's recorded facts. The document is
/// compared fact by fact at check time, never byte by byte;
/// the hash is kept so the record shows exactly which bytes
/// the facts were reduced from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenApiBaseline {
    pub entry_id: String,
    pub url: String,
    /// The sha256 hex of the document at generation.
    pub sha256: String,
    /// The info block version at generation.
    pub version: String,
    /// The servers URLs at generation, sorted.
    pub servers: Vec<String>,
    /// Whether the document demanded security at generation.
    pub security_required: bool,
    pub components_sha256: String,
    pub operations: Vec<OperationBaseline>,
}

/// One operation's recorded facts, mirroring ToolBaseline.
/// The three channels are recorded apart so a change in one
/// can never mask a change in another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationBaseline {
    pub id: String,
    /// Required parameters as "in:name", sorted.
    pub required: Vec<String>,
    pub description_sha256: String,
    pub contract_sha256: String,
}

impl OpenApiBaseline {
    /// Record a reduced document as the baseline for one entry.
    pub fn from_view(
        entry_id: &str,
        url: &str,
        sha256: &str,
        view: &crate::openapi::OpenApiView,
    ) -> Self {
        OpenApiBaseline {
            entry_id: entry_id.to_string(),
            url: url.to_string(),
            sha256: sha256.to_string(),
            version: view.version.clone(),
            servers: view.servers.clone(),
            security_required: view.security_required,
            components_sha256: view.components_sha256.clone(),
            operations: view
                .operations
                .iter()
                .map(|op| OperationBaseline {
                    id: op.id.clone(),
                    required: op.required.clone(),
                    description_sha256: crate::hash::sha256_str(&op.description),
                    contract_sha256: op.contract_sha256.clone(),
                })
                .collect(),
        }
    }
}

/// One A2A agent card's recorded facts, mirroring
/// OpenApiBaseline. The channels are recorded apart so a
/// change in one can never mask a change in another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A2aBaseline {
    pub entry_id: String,
    pub url: String,
    /// The sha256 hex of the card at generation.
    pub sha256: String,
    /// The agent's own version at generation.
    pub version: String,
    /// The agent's name at generation.
    pub name: String,
    /// Hash of the card's description at generation, the
    /// empty string's hash when it carried none.
    pub description_sha256: String,
    /// The interfaces at generation, in the card's
    /// preference order.
    pub interfaces: Vec<InterfaceBaseline>,
    /// The capability flags switched on at generation,
    /// sorted.
    pub capability_flags: Vec<String>,
    /// The extension identifiers declared at generation,
    /// sorted.
    pub extensions: Vec<String>,
    /// The identifiers among them marked required at
    /// generation, sorted. Baselines written before the
    /// twenty-second slice carry none, the serde
    /// default's empty set.
    #[serde(default)]
    pub required_extensions: Vec<String>,
    /// Whether the card demanded security at generation.
    pub security_required: bool,
    /// The card's default input modes at generation,
    /// sorted.
    pub default_input_modes: Vec<String>,
    /// The card's default output modes at generation,
    /// sorted.
    pub default_output_modes: Vec<String>,
    pub skills: Vec<A2aSkillBaseline>,
}

/// One interface's recorded facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceBaseline {
    pub url: String,
    pub protocol_binding: String,
    pub protocol_version: String,
    /// Hash of the rest of the interface object.
    pub rest_sha256: String,
}

impl InterfaceBaseline {
    /// Record one reduced interface. The single conversion
    /// from a view to a record, used by the baseline and
    /// by the drift pass alike.
    pub fn from_view(view: &crate::a2a::InterfaceView) -> Self {
        InterfaceBaseline {
            url: view.url.clone(),
            protocol_binding: view.protocol_binding.clone(),
            protocol_version: view.protocol_version.clone(),
            rest_sha256: view.rest_sha256.clone(),
        }
    }
}

/// One skill's recorded facts, in two channels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A2aSkillBaseline {
    pub id: String,
    pub description_sha256: String,
    pub contract_sha256: String,
}

impl A2aBaseline {
    /// Record a reduced agent card as the baseline for one
    /// entry.
    pub fn from_view(entry_id: &str, url: &str, sha256: &str, view: &crate::a2a::A2aView) -> Self {
        A2aBaseline {
            entry_id: entry_id.to_string(),
            url: url.to_string(),
            sha256: sha256.to_string(),
            version: view.version.clone(),
            name: view.name.clone(),
            description_sha256: crate::hash::sha256_str(
                view.description.as_deref().unwrap_or_default(),
            ),
            interfaces: view
                .interfaces
                .iter()
                .map(InterfaceBaseline::from_view)
                .collect(),
            capability_flags: view.capability_flags.clone(),
            extensions: view.extensions.clone(),
            required_extensions: view.required_extensions.clone(),
            security_required: view.security_required,
            default_input_modes: view.default_input_modes.clone(),
            default_output_modes: view.default_output_modes.clone(),
            skills: view
                .skills
                .iter()
                .map(|skill| A2aSkillBaseline {
                    id: skill.id.clone(),
                    description_sha256: crate::hash::sha256_str(&skill.description),
                    contract_sha256: skill.contract_sha256.clone(),
                })
                .collect(),
        }
    }
}

impl McpBaseline {
    /// Record a probed surface as the baseline for one entry.
    pub fn from_snapshot(entry_id: &str, snapshot: &SurfaceSnapshot) -> Self {
        let tools = snapshot
            .tools
            .iter()
            .map(|tool| {
                (
                    tool.name.clone(),
                    ToolBaseline {
                        description_sha256: crate::hash::sha256_str(&tool.description),
                        schema_sha256: crate::hash::sha256_json(&tool.input_schema),
                    },
                )
            })
            .collect();
        McpBaseline {
            entry_id: entry_id.to_string(),
            server_name: snapshot.server_name.clone(),
            server_version: snapshot.server_version.clone(),
            protocol_version: snapshot.protocol_version.clone(),
            tools,
            resources: snapshot.resources.clone(),
            prompts: snapshot.prompts.clone(),
            capabilities_enumerated: snapshot.capabilities_enumerated,
            card_display: None,
            auth: AuthBaseline::from(&snapshot.auth),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{CredentialOutcome, ToolSurface};

    #[test]
    fn baseline_records_tool_hashes() {
        let snapshot = SurfaceSnapshot {
            transport: "stdio".into(),
            server_name: "demo".into(),
            server_version: "0.1.0".into(),
            protocol_version: "2025-06-18".into(),
            stateless_shape: false,
            capabilities_enumerated: true,
            tools: vec![ToolSurface {
                name: "ping".into(),
                description: "Ping the server.".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            resources: vec![],
            prompts: vec![],
            auth: AuthSurface::Open,
            credential: CredentialOutcome::NotPresented,
            failure: None,
        };
        let baseline = McpBaseline::from_snapshot("urn:air:example.com:mcp:demo", &snapshot);
        let tool = baseline.tools.get("ping").expect("tool recorded");
        assert_eq!(
            tool.description_sha256,
            crate::hash::sha256_str("Ping the server.")
        );
        assert_eq!(
            tool.schema_sha256,
            crate::hash::sha256_json(&serde_json::json!({"type": "object"}))
        );
        assert_eq!(baseline.server_name, "demo");
    }

    #[test]
    fn a_baseline_written_before_the_openapi_slice_still_parses() {
        // The OpenAPI slice added the openapi list with a serde
        // default. A publisher's committed baseline from before
        // the slice has no such key, and it must still load, or
        // every existing baseline breaks at upgrade.
        let text = r#"{
            "spec_version": "ard/0.91",
            "mcp": null,
            "skills": [],
            "artifacts": [],
            "provenance": []
        }"#;
        let baseline: Baseline = serde_json::from_str(text).expect("old baseline parses");
        assert!(baseline.openapi.is_empty());
        // The same document predates the trust records
        // too, and their default is empty as well.
        assert!(baseline.trust_manifests.is_empty());
    }

    #[test]
    fn a_baseline_written_before_the_required_extensions_slice_still_parses() {
        // The twenty-second slice added required_extensions
        // to the A2A record with a serde default. A committed
        // baseline from before the slice has an A2A record
        // with no such key, and it must still load with the
        // set empty.
        let text = r#"{
            "spec_version": "ard/0.91",
            "mcp": null,
            "skills": [],
            "artifacts": [],
            "provenance": [],
            "a2a": [{
                "entry_id": "urn:air:example.com:a2a:review",
                "url": "https://example.com/review-agent.json",
                "sha256": "00",
                "version": "1.2.0",
                "name": "Code Review Agent",
                "description_sha256": "00",
                "interfaces": [],
                "capability_flags": [],
                "extensions": ["https://example.com/ext/pay"],
                "security_required": false,
                "default_input_modes": [],
                "default_output_modes": [],
                "skills": []
            }]
        }"#;
        let baseline: Baseline = serde_json::from_str(text).expect("old baseline parses");
        assert_eq!(baseline.a2a.len(), 1);
        assert!(baseline.a2a[0].required_extensions.is_empty());
        assert_eq!(
            baseline.a2a[0].extensions,
            vec!["https://example.com/ext/pay".to_string()]
        );
    }

    #[test]
    fn a_baseline_written_before_the_display_slice_still_parses() {
        // The display slice added card_display to the MCP
        // record with a serde default. A committed baseline
        // from before the slice has an MCP record with no
        // such key, and it must still load with the field
        // absent, or every existing baseline breaks.
        let text = r#"{
            "spec_version": "ard/0.91",
            "mcp": {
                "entry_id": "urn:air:example.com:mcp:demo",
                "server_name": "demo",
                "server_version": "0.1.0",
                "protocol_version": "2025-06-18",
                "tools": {},
                "resources": [],
                "prompts": [],
                "auth": "Open"
            },
            "skills": [],
            "artifacts": [],
            "provenance": []
        }"#;
        let baseline: Baseline = serde_json::from_str(text).expect("old baseline parses");
        let mcp = baseline.mcp.expect("mcp record");
        assert!(mcp.card_display.is_none());
    }
}
