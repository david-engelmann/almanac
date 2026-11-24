//! The surface snapshot, the one value every probe produces.
//!
//! A probe, stdio or Streamable HTTP, walks a live MCP surface and
//! returns this value. The drift engine compares snapshots against
//! the baseline and the published catalog without knowing which
//! transport produced them, and every comparison stays in this
//! crate, testable without a network.
//!
//! The authenticated path from docs/HTTP-TRANSPORT.md lands here as
//! data. A protected surface classifies as [`AuthSurface::Protected`]
//! and carries the OAuth metadata documents it published, reduced
//! to the fields the drift model compares plus a hash over each
//! document's canonical bytes.

use serde::{Deserialize, Serialize};

/// One probed MCP surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSnapshot {
    /// "stdio" or "streamable-http".
    pub transport: String,
    pub server_name: String,
    pub server_version: String,
    /// The negotiated MCP protocol version, recorded because a
    /// server that renegotiates its protocol has changed its
    /// surface.
    pub protocol_version: String,
    /// True when the negotiated revision is the stateless shape
    /// of the protocol, the 2026-07-28 revision. Recorded as a
    /// fact about the surface. Full support for that revision is
    /// a later addition, named in docs/HTTP-TRANSPORT.md so its
    /// absence is a decision and not a surprise.
    pub stateless_shape: bool,
    /// True when this snapshot's tool, resource and prompt
    /// lists are the surface's own enumeration. A completed
    /// live walk enumerates; a probe that stopped early, or
    /// one answered with an authorization challenge, does
    /// not. A snapshot built from a server card enumerates
    /// only when the card carries a tools field, because a
    /// card in the SEP-2127 Final shape lists no primitives
    /// at all, and an unenumerated list is a different fact
    /// from an empty one: comparisons that would read
    /// absence as removal must not run on it.
    pub capabilities_enumerated: bool,
    pub tools: Vec<ToolSurface>,
    /// Resource URIs, sorted by the probe.
    pub resources: Vec<String>,
    /// Prompt names, sorted by the probe.
    pub prompts: Vec<String>,
    pub auth: AuthSurface,
    /// How a publisher credential figured in this probe. A
    /// surface's auth posture and a run's credential are
    /// different facts: the posture is the surface's, recorded
    /// in the baseline and compared across runs, while the
    /// credential is the run's, and decides only which evidence
    /// the run gathered.
    pub credential: CredentialOutcome,
    /// Set when the probe could not complete. The step names where
    /// it stopped. A snapshot with a failure still carries whatever
    /// the probe learned before it, and the drift engine decides
    /// what the failure means.
    pub failure: Option<ProbeFailure>,
}

/// A publisher supplied credential, presented by the probe
/// when a protected surface challenges it. Almanac runs no
/// authorization flow; the token is the publisher's own,
/// read from the environment, and presented only to the MCP
/// endpoint itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CredentialOutcome {
    /// No credential was presented. The walk ran open, or the
    /// surface challenged and the run holds no token.
    NotPresented,
    /// A credential was presented and the surface accepted
    /// it: this snapshot's lists were walked live on a
    /// protected surface, and its auth still records the
    /// declared posture the metadata ladder read.
    Accepted,
    /// A credential was presented and the surface answered
    /// 401 at the named step. The snapshot is the declared
    /// mode one, and the refusal is a fact of the run the
    /// report names.
    Refused(String),
}

impl SurfaceSnapshot {
    /// True when this snapshot is live evidence from a
    /// protected surface: the surface's posture is Protected
    /// and the run walked it on a credential the surface
    /// accepted. The one place this slice's central predicate
    /// is stated, so the drift dispatch and the three
    /// commands cannot disagree about the same snapshot.
    pub fn walked_live(&self) -> bool {
        matches!(self.auth, AuthSurface::Protected(_))
            && self.credential == CredentialOutcome::Accepted
    }
}

/// One tool as served.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSurface {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Where a probe stopped and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeFailure {
    /// The probe step, for example "initialize" or "tools/list".
    pub step: String,
    /// What happened, an HTTP status, a JSON-RPC error, a timeout.
    pub detail: String,
}

/// The authentication posture of a probed surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthSurface {
    /// The handshake completed without a credential.
    Open,
    /// The handshake was answered 401. The metadata is what the
    /// surface published about its authorization, fetched by the
    /// probe's metadata ladder, and boxed because a protected
    /// surface is the cold path of a probe.
    Protected(Box<ProtectedMetadata>),
}

/// The OAuth metadata behind a protected surface. Either document
/// may be absent, and that absence is itself recorded, because a
/// server that gains or loses its metadata has changed its auth
/// surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtectedMetadata {
    pub resource_metadata: Option<MetadataRecord>,
    pub authorization_servers: Vec<MetadataRecord>,
}

/// One OAuth metadata document, reduced to the compared fields.
/// The hash runs over the document's canonical bytes, so a key
/// reordering is not drift and any real change is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataRecord {
    /// The URL the document was fetched from.
    pub url: String,
    pub sha256: String,
    pub issuer: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub jwks_uri: Option<String>,
    /// Sorted. Empty when the document names no scopes.
    pub scopes_supported: Vec<String>,
    /// The protected resource value, on resource metadata.
    pub resource: Option<String>,
}

impl MetadataRecord {
    /// Reduce a fetched metadata document to a record. The CLI
    /// fetches the document, this crate owns the reduction, so the
    /// field choices live with the drift model that reads them.
    pub fn from_document(url: &str, document: &serde_json::Value) -> Self {
        let text = |key: &str| {
            document
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        let mut scopes: Vec<String> = document
            .get("scopes_supported")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        scopes.sort();
        MetadataRecord {
            url: url.to_string(),
            sha256: crate::hash::sha256_json(document),
            issuer: text("issuer"),
            authorization_endpoint: text("authorization_endpoint"),
            token_endpoint: text("token_endpoint"),
            jwks_uri: text("jwks_uri"),
            scopes_supported: scopes,
            resource: text("resource"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_reduction_extracts_and_hashes() {
        let doc = serde_json::json!({
            "issuer": "https://auth.example.com",
            "authorization_endpoint": "https://auth.example.com/authorize",
            "token_endpoint": "https://auth.example.com/token",
            "jwks_uri": "https://auth.example.com/jwks",
            "scopes_supported": ["write", "read"],
            "resource": "https://api.example.com/mcp"
        });
        let record = MetadataRecord::from_document("https://auth.example.com/meta", &doc);
        assert_eq!(record.issuer.as_deref(), Some("https://auth.example.com"));
        assert_eq!(
            record.authorization_endpoint.as_deref(),
            Some("https://auth.example.com/authorize")
        );
        assert_eq!(
            record.token_endpoint.as_deref(),
            Some("https://auth.example.com/token")
        );
        assert_eq!(
            record.jwks_uri.as_deref(),
            Some("https://auth.example.com/jwks")
        );
        assert_eq!(
            record.resource.as_deref(),
            Some("https://api.example.com/mcp")
        );
        assert_eq!(record.scopes_supported, vec!["read", "write"]);
        assert_eq!(record.sha256, crate::hash::sha256_json(&doc));
    }

    #[test]
    fn metadata_key_order_is_not_drift() {
        let a = serde_json::json!({"issuer": "https://a.example", "resource": "https://r.example"});
        let b = serde_json::json!({"resource": "https://r.example", "issuer": "https://a.example"});
        let ra = MetadataRecord::from_document("https://a.example/m", &a);
        let rb = MetadataRecord::from_document("https://a.example/m", &b);
        assert_eq!(ra.sha256, rb.sha256);
    }
}
