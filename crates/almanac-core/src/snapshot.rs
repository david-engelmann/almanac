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
use std::collections::BTreeMap;

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
    /// The server's declared capabilities object, as
    /// served at the entry step beside serverInfo: the
    /// contract envelope the baseline hashes. Some
    /// whenever the entry step was reached and answered,
    /// with a served absence of the member recorded as
    /// JSON null, so an absence hashes distinctly from
    /// an empty object; None when the entry step was
    /// never reached, or the snapshot came from declared
    /// evidence (a server card, a protected surface),
    /// where capabilities evidence does not exist.
    pub capabilities: Option<serde_json::Value>,
    /// True when this snapshot's tool, resource, prompt
    /// and template lists are the surface's own
    /// enumeration. A completed live walk enumerates; a
    /// probe that stopped early, or
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
    /// One surface per resource, sorted by uri by the
    /// probe: the editorial fields a client renders
    /// and the mimeType the content is read through.
    /// The twenty-fifth slice's addition; the uri list
    /// above stays the membership record.
    pub resource_surfaces: Vec<ResourceSurface>,
    /// Resource template uriTemplates, sorted by the
    /// probe. The twenty-sixth slice's addition.
    pub resource_templates: Vec<String>,
    /// One surface per resource template, sorted by
    /// uriTemplate by the probe: the editorial fields
    /// and the mimeType reads through the template
    /// produce. The template list above stays the
    /// membership record.
    pub template_surfaces: Vec<TemplateSurface>,
    /// Contents hashes by resource uri, one per
    /// resource the probe read successfully after the
    /// list walk. A resource whose read failed carries
    /// no entry, which the baseline records as no
    /// contents hash and the gate reads, against a
    /// recorded hash, as a failed read.
    pub resource_contents: BTreeMap<String, String>,
    /// Prompt names, sorted by the probe.
    pub prompts: Vec<String>,
    /// One surface per prompt, sorted by name by the
    /// probe: the prompt's description and arguments,
    /// the contract the name alone cannot carry. The
    /// twenty-fourth slice's addition; the name list
    /// above stays the membership record.
    pub prompt_surfaces: Vec<PromptSurface>,
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

/// One prompt as served: its name, its description,
/// and its arguments array as served, the prompt's
/// input contract in the shape prompts/list carries
/// it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptSurface {
    pub name: String,
    pub description: String,
    pub arguments: serde_json::Value,
}

/// One resource as served: its uri, the editorial
/// fields a client renders (name, title,
/// description), and the mimeType the content at
/// the uri is read through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceSurface {
    pub uri: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub mime_type: String,
}

impl ResourceSurface {
    /// The editorial fields as one canonical record,
    /// the value the baseline's metadata hash reads.
    /// One construction, shared by generation and
    /// check, so the two sides cannot drift apart.
    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
        })
    }
}

/// One resource template as served: its uriTemplate,
/// the editorial fields a client renders (name,
/// title, description), and the mimeType reads
/// through the template produce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSurface {
    pub uri_template: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub mime_type: String,
}

impl TemplateSurface {
    /// The editorial fields as one canonical record,
    /// the value the baseline's metadata hash reads,
    /// in the ResourceSurface shape.
    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
        })
    }
}

/// Hash one resources/read result's contents, the
/// canonical record the baseline stores and the gate
/// recomputes: the items in served order, a text item
/// by its text as served, a blob item by the hash of
/// its decoded bytes, because the content is the fact
/// and base64 is a transport encoding whose wrapping
/// is not. None when the result carries no contents
/// array or an item is neither text nor blob shaped,
/// which the probe records as a failed read.
pub fn contents_hash(result: &serde_json::Value) -> Option<String> {
    let items = result.get("contents")?.as_array()?;
    let mut record = Vec::with_capacity(items.len());
    for item in items {
        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
            record.push(serde_json::json!({"text": text}));
        } else {
            let blob = item.get("blob").and_then(|v| v.as_str())?;
            let bytes = crate::statement::decode_base64(blob)?;
            record.push(serde_json::json!({
                "blob_sha256": crate::hash::sha256_hex(&bytes)
            }));
        }
    }
    Some(crate::hash::sha256_json(&serde_json::Value::Array(record)))
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
    fn contents_hash_reads_text_and_blob_items() {
        // Text hashes as served; a blob hashes its
        // decoded bytes, so the same bytes under a
        // different base64 wrapping hash identically,
        // and item order is part of the record.
        let text = serde_json::json!({"contents": [
            {"uri": "docs://a", "mimeType": "text/plain", "text": "hello"}
        ]});
        assert_eq!(
            contents_hash(&text),
            Some(crate::hash::sha256_json(&serde_json::json!([
                {"text": "hello"}
            ])))
        );
        let blob_a = serde_json::json!({"contents": [
            {"uri": "docs://b", "blob": "aGVsbG8="}
        ]});
        let blob_b = serde_json::json!({"contents": [
            {"uri": "docs://b", "blob": "aGVs bG8="}
        ]});
        assert_eq!(contents_hash(&blob_a), contents_hash(&blob_b));
        assert_ne!(contents_hash(&text), contents_hash(&blob_a));
        let two = serde_json::json!({"contents": [
            {"uri": "docs://a", "text": "one"},
            {"uri": "docs://a", "text": "two"}
        ]});
        let swapped = serde_json::json!({"contents": [
            {"uri": "docs://a", "text": "two"},
            {"uri": "docs://a", "text": "one"}
        ]});
        assert_ne!(contents_hash(&two), contents_hash(&swapped));
        assert_eq!(
            contents_hash(&serde_json::json!({"contents": []})),
            Some(crate::hash::sha256_json(&serde_json::json!([])))
        );
        assert_eq!(contents_hash(&serde_json::json!({})), None);
        assert_eq!(
            contents_hash(&serde_json::json!({"contents": [{"uri": "docs://c"}]})),
            None
        );
        assert_eq!(
            contents_hash(&serde_json::json!({"contents": [{"blob": "!!!"}]})),
            None
        );
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
