//! The repo metadata reduction. A repo's own manifests
//! carry a name and a description for the project, and
//! DESIGN rule 4 makes those words the last editorial
//! source a generated entry may draw on, flagged low
//! confidence, after config, the server card, the OpenAPI
//! info block, and SKILL.md. Rule 2 binds the other side:
//! repo metadata never wins a capability fact, so the
//! reduction reads name and description only, and the
//! manifests' version fields are deliberately not read.
//!
//! Three manifest shapes are read, each presented to the
//! reduction as a kind selecting its field paths, a label
//! naming where the manifest was found, and its parsed
//! document: package.json (top-level name and
//! description), Cargo.toml (package.name and
//! package.description), and pyproject.toml (the PEP 621
//! project table). Fields resolve per field in the order
//! the sources are presented, the first non-empty value
//! wins, and the reduction records which source supplied
//! each field so the generation report can name it.

use serde_json::Value;

/// A repo's editorial words, reduced from its manifests.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RepoMetadata {
    pub name: Option<String>,
    /// The label of the manifest the name came from.
    pub name_source: Option<String>,
    pub description: Option<String>,
    /// The label of the manifest the description came from.
    pub description_source: Option<String>,
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Reduce parsed manifests, in search order, to the repo's
/// editorial words. Each source is a manifest kind (which
/// selects the field paths), a display label naming where
/// the manifest was found (which the reduction records as
/// the winning field's source), and its parsed document;
/// an unrecognized kind contributes nothing.
pub fn reduce_repo_metadata(sources: &[(&str, &str, &Value)]) -> RepoMetadata {
    let mut metadata = RepoMetadata::default();
    for (kind, label, doc) in sources {
        let (name, description) = match *kind {
            "package.json" => (text(doc.get("name")), text(doc.get("description"))),
            "Cargo.toml" => (
                text(doc.get("package").and_then(|p| p.get("name"))),
                text(doc.get("package").and_then(|p| p.get("description"))),
            ),
            "pyproject.toml" => (
                text(doc.get("project").and_then(|p| p.get("name"))),
                text(doc.get("project").and_then(|p| p.get("description"))),
            ),
            _ => (None, None),
        };
        if metadata.name.is_none() && name.is_some() {
            metadata.name = name;
            metadata.name_source = Some(label.to_string());
        }
        if metadata.description.is_none() && description.is_some() {
            metadata.description = description;
            metadata.description_source = Some(label.to_string());
        }
        if metadata.name.is_some() && metadata.description.is_some() {
            break;
        }
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_manifest_shape_yields_its_words() {
        let doc =
            serde_json::json!({"name": "dusk", "description": "Books music.", "version": "9.9.9"});
        let metadata = reduce_repo_metadata(&[("package.json", "package.json", &doc)]);
        assert_eq!(metadata.name.as_deref(), Some("dusk"));
        assert_eq!(metadata.description.as_deref(), Some("Books music."));
        assert_eq!(metadata.name_source.as_deref(), Some("package.json"));

        let doc = serde_json::json!({"package": {"name": "almanac", "description": "Drift gate."}});
        let metadata = reduce_repo_metadata(&[("Cargo.toml", "Cargo.toml", &doc)]);
        assert_eq!(metadata.name.as_deref(), Some("almanac"));
        assert_eq!(metadata.description.as_deref(), Some("Drift gate."));

        let doc = serde_json::json!({"project": {"name": "peira", "description": "Benchmarks."}});
        let metadata = reduce_repo_metadata(&[("pyproject.toml", "pyproject.toml", &doc)]);
        assert_eq!(metadata.name.as_deref(), Some("peira"));
        // The poetry table is not read.
        let doc = serde_json::json!({"tool": {"poetry": {"name": "old", "description": "Old."}}});
        let metadata = reduce_repo_metadata(&[("pyproject.toml", "pyproject.toml", &doc)]);
        assert_eq!(metadata, RepoMetadata::default());
    }

    #[test]
    fn fields_resolve_per_field_in_search_order() {
        let first = serde_json::json!({"name": "first", "description": "  "});
        let second =
            serde_json::json!({"package": {"name": "second", "description": "From cargo."}});
        let metadata = reduce_repo_metadata(&[
            ("package.json", "package.json in the site directory", &first),
            ("Cargo.toml", "Cargo.toml in the parent directory", &second),
        ]);
        // The empty description does not count, so the
        // second manifest supplies it while the first
        // keeps the name, each with its own source label.
        assert_eq!(metadata.name.as_deref(), Some("first"));
        assert_eq!(
            metadata.name_source.as_deref(),
            Some("package.json in the site directory")
        );
        assert_eq!(metadata.description.as_deref(), Some("From cargo."));
        assert_eq!(
            metadata.description_source.as_deref(),
            Some("Cargo.toml in the parent directory")
        );
    }
}
