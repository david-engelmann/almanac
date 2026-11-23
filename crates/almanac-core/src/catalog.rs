//! The published catalog model.
//!
//! One internal model writes both discovery paths, the current
//! /.well-known/ard.json and the legacy /.well-known/ai-catalog.json,
//! so the pair can never quietly split. Field names follow the ARD
//! Internet Draft. An entry carries exactly one of `url` or `data`,
//! its identifier is a domain anchored URN, and every entry that
//! references an artifact by url carries the digest of that
//! artifact as served.

use serde::{Deserialize, Serialize};

/// The catalog file, one host block and one entries array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub host: Host,
    pub entries: Vec<Entry>,
}

/// The publisher block both paths share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub domain: String,
}

/// One announced resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Domain anchored URN, urn:air:<domain>:<namespace>:<name>.
    pub identifier: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    /// IANA media type of the announced resource.
    #[serde(rename = "type")]
    pub media_type: String,
    /// Where the resource lives. Exactly one of url and data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The resource inlined. Exactly one of url and data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(
        rename = "representativeQueries",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub representative_queries: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `sha256:` prefixed digest of the referenced artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// The trust manifest, carried as the draft defines it.
    /// Almanac validates its shape and expiry only, per
    /// Amendment D. It never verifies the signature in version 1
    /// and holds no key material.
    #[serde(rename = "trustManifest", skip_serializing_if = "Option::is_none")]
    pub trust_manifest: Option<serde_json::Value>,
}

impl Entry {
    /// True when the entry carries both url and data, or neither.
    pub fn has_shape_error(&self) -> bool {
        self.url.is_none() == self.data.is_none()
    }

    /// True when the URN's publisher segment is the given domain.
    /// Almanac generates single publisher catalogs, so every entry
    /// it generates or baselines must anchor to the host domain.
    pub fn anchored_to(&self, domain: &str) -> bool {
        // The domain is matched as a prefix segment, not by
        // splitting on every colon, because a domain carrying
        // a port is itself colon bearing and a split would
        // compare only its first half.
        self.identifier
            .strip_prefix("urn:air:")
            .map(|rest| rest == domain || rest.starts_with(&format!("{domain}:")))
            .unwrap_or(false)
    }
}

impl Catalog {
    /// The canonical file bytes. Keys sorted by serde_json's map
    /// order, two space indent, one trailing newline. Same model,
    /// same bytes, on every machine.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let value = serde_json::to_value(self).expect("the catalog model always serializes");
        let mut text =
            serde_json::to_string_pretty(&value).expect("a JSON value always serializes");
        text.push('\n');
        text.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry {
            identifier: "urn:air:example.com:mcp:demo".into(),
            display_name: "Demo".into(),
            media_type: "application/mcp-server-card+json".into(),
            url: Some("https://example.com/mcp".into()),
            data: None,
            description: "A demo server.".into(),
            capabilities: vec![],
            representative_queries: vec![],
            tags: vec![],
            version: None,
            digest: None,
            updated_at: None,
            trust_manifest: None,
        }
    }

    #[test]
    fn shape_error_needs_exactly_one_of_url_and_data() {
        assert!(!entry().has_shape_error());
        let mut both = entry();
        both.data = Some(serde_json::json!({}));
        assert!(both.has_shape_error());
        let mut neither = entry();
        neither.url = None;
        assert!(neither.has_shape_error());
    }

    #[test]
    fn urn_anchoring_reads_the_publisher_segment() {
        assert!(entry().anchored_to("example.com"));
        assert!(!entry().anchored_to("other.example"));
    }

    #[test]
    fn canonical_bytes_end_with_one_newline() {
        let catalog = Catalog {
            host: Host {
                display_name: "Example".into(),
                domain: "example.com".into(),
            },
            entries: vec![entry()],
        };
        let bytes = catalog.canonical_bytes();
        assert!(bytes.ends_with(b"\n"));
        assert!(!bytes.ends_with(b"\n\n"));
    }
}
