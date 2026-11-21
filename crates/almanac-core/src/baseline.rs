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
    /// Per entry provenance. Which input produced the entry, and
    /// which inputs it overruled on the way.
    pub provenance: Vec<EntryProvenance>,
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
    /// The auth posture at generation time, with metadata records.
    pub auth: AuthBaseline,
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
            auth: AuthBaseline::from(&snapshot.auth),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::ToolSurface;

    #[test]
    fn baseline_records_tool_hashes() {
        let snapshot = SurfaceSnapshot {
            transport: "stdio".into(),
            server_name: "demo".into(),
            server_version: "0.1.0".into(),
            protocol_version: "2025-06-18".into(),
            stateless_shape: false,
            tools: vec![ToolSurface {
                name: "ping".into(),
                description: "Ping the server.".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            resources: vec![],
            prompts: vec![],
            auth: AuthSurface::Open,
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
}
