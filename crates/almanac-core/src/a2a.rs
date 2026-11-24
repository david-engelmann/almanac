//! The A2A agent card reduction. An agent card is a declared
//! surface, and the drift gate compares it the way it compares
//! every surface: fact by fact, never as one blob of bytes.
//!
//! The reduction keeps the agent's name and version, its
//! interfaces in the order the card prefers them (each an
//! url, protocol binding, and protocol version triple,
//! plus a hash of the rest of the interface object, where
//! fields like a multi-tenant marker live), its capability
//! flags and declared extension identifiers, whether it
//! demands security, its default input and output modes,
//! the skills' own example utterances in card order, and
//! its skills by identifier. Each skill keeps two
//! independent channels: a hash of its description, and a
//! hash of the rest of the skill object (its name, tags,
//! examples, and modes), so a description edit can never
//! mask a change to what the skill claims to do.
//!
//! Two card shapes are read. The 1.0 shape carries its
//! endpoints in supportedInterfaces. The 0.3 shape still
//! circulating carries a top-level url, preferredTransport,
//! and protocolVersion instead, and the reduction
//! synthesizes its single interface from those fields, the
//! way the MCP server card reduction reads both of its
//! shapes. The interface protocol versions the card declares
//! are recorded as declared; the gate does not negotiate.
//!
//! Posture on malformed input: the card's frame (an object
//! carrying a name, a version, and at least one interface
//! naming a url) must be sound or the card is refused with
//! the reason named. Inside a sound frame, a skill entry
//! that is not an object or carries no identifier is
//! skipped, because real cards carry partial entries the
//! gate has no class for; a repeated skill identifier is
//! refused, because the gate cannot tell the skills apart.

use std::collections::HashSet;

use serde_json::Value;

use crate::hash;

/// One interface, reduced to the facts the gate compares.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceView {
    pub url: String,
    /// The protocol binding, JSONRPC, GRPC, HTTP+JSON, or a
    /// custom value the card declares. Empty when the card
    /// names none.
    pub protocol_binding: String,
    /// The A2A protocol version this interface exposes, as
    /// declared. Empty when the card names none.
    pub protocol_version: String,
    /// Hash of the interface object with the three named
    /// fields removed, so a change to any other field the
    /// interface carries is gated too.
    pub rest_sha256: String,
}

/// One skill, reduced to two independent channels.
#[derive(Debug, Clone, PartialEq)]
pub struct A2aSkillView {
    pub id: String,
    pub description: String,
    /// Hash of the skill object with its description
    /// removed: the contract channel.
    pub contract_sha256: String,
}

/// One agent card, reduced.
#[derive(Debug, Clone, PartialEq)]
pub struct A2aView {
    pub name: String,
    /// The card's description, when it carries one.
    pub description: Option<String>,
    /// The agent's own version, not the protocol's.
    pub version: String,
    /// The interfaces in the card's preference order.
    pub interfaces: Vec<InterfaceView>,
    /// The capability flags as a sorted set of the flags
    /// that are switched on, drawn from streaming,
    /// pushNotifications, and extendedAgentCard.
    pub capability_flags: Vec<String>,
    /// The extension identifiers the card declares under
    /// capabilities.extensions, sorted. A required
    /// extension changes what a client must implement, so
    /// the set is gated beside the flags.
    pub extensions: Vec<String>,
    /// True when the card demands security: a non-empty
    /// securityRequirements list or a non-empty
    /// securitySchemes map.
    pub security_required: bool,
    /// The card's default input modes, sorted.
    pub default_input_modes: Vec<String>,
    /// The card's default output modes, sorted.
    pub default_output_modes: Vec<String>,
    /// The example utterances of the skills the reduction
    /// kept, in card order: the publisher's own words for
    /// what the agent does, harvested for the catalog
    /// entry's representative queries.
    pub examples: Vec<String>,
    /// Skills sorted by identifier.
    pub skills: Vec<A2aSkillView>,
}

/// Reduce a parsed JSON document to its view. An Err names
/// the reason the document cannot be read as an agent card,
/// in words a publisher can act on.
pub fn reduce_agent_card(doc: &Value) -> Result<A2aView, String> {
    let doc = doc
        .as_object()
        .ok_or("the document is not a JSON object, so it is not an agent card")?;
    let name = doc
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("the card carries no name")?
        .to_string();
    let version = doc
        .get("version")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("the card carries no version")?
        .to_string();
    let description = doc
        .get("description")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut interfaces: Vec<InterfaceView> = Vec::new();
    let declared = doc.get("supportedInterfaces").and_then(|v| v.as_array());
    if let Some(list) = declared {
        for item in list {
            let Some(url) = item
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let mut rest = item.as_object().cloned().unwrap_or_default();
            rest.remove("url");
            rest.remove("protocolBinding");
            rest.remove("protocolVersion");
            interfaces.push(InterfaceView {
                url: url.to_string(),
                protocol_binding: item
                    .get("protocolBinding")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                protocol_version: item
                    .get("protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                rest_sha256: hash::sha256_json(&Value::Object(rest)),
            });
        }
    }
    // The 0.3 fallback runs only when the 1.0 field is
    // absent or an empty list. A card that declares
    // interfaces but names no usable url in them has no
    // interface; falling back to a stale top-level url
    // would announce an endpoint the card no longer
    // prefers, silently.
    let declared_none = declared.is_none_or(|list| list.is_empty());
    if interfaces.is_empty() && declared_none {
        // The 0.3 shape: a top-level url, with the transport
        // and the protocol version beside it.
        if let Some(url) = doc
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            interfaces.push(InterfaceView {
                url: url.to_string(),
                protocol_binding: doc
                    .get("preferredTransport")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                protocol_version: doc
                    .get("protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                rest_sha256: hash::sha256_json(&Value::Object(Default::default())),
            });
        }
    }
    if interfaces.is_empty() {
        return Err(
            "the card names no interface with a url, in supportedInterfaces or at the top level"
                .into(),
        );
    }

    let mut capability_flags: Vec<String> = Vec::new();
    let mut extensions: Vec<String> = Vec::new();
    if let Some(capabilities) = doc.get("capabilities").and_then(|v| v.as_object()) {
        for flag in ["streaming", "pushNotifications", "extendedAgentCard"] {
            if capabilities.get(flag).and_then(|v| v.as_bool()) == Some(true) {
                capability_flags.push(flag.to_string());
            }
        }
        if let Some(list) = capabilities.get("extensions").and_then(|v| v.as_array()) {
            for item in list {
                if let Some(uri) = item
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    extensions.push(uri.to_string());
                }
            }
        }
    }
    capability_flags.sort();
    extensions.sort();
    extensions.dedup();
    let modes = |key: &str| -> Vec<String> {
        let mut values: Vec<String> = doc
            .get(key)
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        values.sort();
        values.dedup();
        values
    };
    let default_input_modes = modes("defaultInputModes");
    let default_output_modes = modes("defaultOutputModes");
    let security_required = doc
        .get("securityRequirements")
        .and_then(|v| v.as_array())
        .is_some_and(|list| !list.is_empty())
        || doc
            .get("securitySchemes")
            .and_then(|v| v.as_object())
            .is_some_and(|map| !map.is_empty());

    let mut seen: HashSet<String> = HashSet::new();
    let mut skills: Vec<A2aSkillView> = Vec::new();
    let mut examples: Vec<String> = Vec::new();
    if let Some(list) = doc.get("skills").and_then(|v| v.as_array()) {
        for item in list {
            let Some(object) = item.as_object() else {
                continue;
            };
            let Some(id) = object
                .get("id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                return Err(format!(
                    "skill identifier {id} appears more than once, and the gate cannot tell the skills apart"
                ));
            }
            // A description that is not a string still
            // changes the channel when it changes: its JSON
            // text stands in for the string, so 42 and 43 do
            // not hash alike.
            let description = match object.get("description") {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            if let Some(list) = object.get("examples").and_then(|v| v.as_array()) {
                for example in list {
                    if let Some(text) = example.as_str().filter(|s| !s.is_empty()) {
                        examples.push(text.to_string());
                    }
                }
            }
            let mut contract = object.clone();
            contract.remove("description");
            skills.push(A2aSkillView {
                id: id.to_string(),
                description,
                contract_sha256: hash::sha256_json(&Value::Object(contract)),
            });
        }
    }
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(A2aView {
        name,
        description,
        version,
        interfaces,
        capability_flags,
        extensions,
        security_required,
        default_input_modes,
        default_output_modes,
        examples,
        skills,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> Value {
        serde_json::json!({
            "name": "Code Review Agent",
            "description": "Reviews code changes.",
            "version": "1.2.0",
            "supportedInterfaces": [
                {"url": "https://agents.example.com/review", "protocolBinding": "JSONRPC", "protocolVersion": "1.0"},
                {"url": "https://agents.example.com/review-grpc", "protocolBinding": "GRPC", "protocolVersion": "1.0"}
            ],
            "capabilities": {"streaming": true, "pushNotifications": false},
            "defaultInputModes": ["text/plain"],
            "defaultOutputModes": ["text/plain"],
            "skills": [
                {"id": "summarize-repo", "name": "Summarize", "description": "Summarizes a repo.", "tags": ["code"]},
                {"id": "review-diff", "name": "Review a diff", "description": "Reviews a diff.", "tags": ["code"], "examples": ["Review PR 12"]}
            ]
        })
    }

    #[test]
    fn reduction_reads_the_card() {
        let view = reduce_agent_card(&card()).expect("reduces");
        assert_eq!(view.name, "Code Review Agent");
        assert_eq!(view.version, "1.2.0");
        assert_eq!(view.description.as_deref(), Some("Reviews code changes."));
        assert_eq!(view.interfaces.len(), 2);
        assert_eq!(view.interfaces[0].protocol_binding, "JSONRPC");
        assert_eq!(view.capability_flags, vec!["streaming".to_string()]);
        assert!(!view.security_required);
        // Skills come back sorted by identifier.
        assert_eq!(view.skills[0].id, "review-diff");
        assert_eq!(view.skills[1].id, "summarize-repo");
    }

    #[test]
    fn reduction_reads_the_03_shape() {
        let legacy = serde_json::json!({
            "name": "Old Agent",
            "version": "0.9.0",
            "url": "https://agents.example.com/old",
            "preferredTransport": "JSONRPC",
            "protocolVersion": "0.3",
            "skills": [{"id": "act", "description": "Acts."}]
        });
        let view = reduce_agent_card(&legacy).expect("reduces");
        assert_eq!(view.interfaces.len(), 1);
        assert_eq!(view.interfaces[0].url, "https://agents.example.com/old");
        assert_eq!(view.interfaces[0].protocol_version, "0.3");
        assert_eq!(view.skills.len(), 1);
    }

    #[test]
    fn security_is_required_when_schemes_or_requirements_are_named() {
        let mut secured = card();
        secured["securitySchemes"] =
            serde_json::json!({"bearer": {"type": "http", "scheme": "bearer"}});
        let view = reduce_agent_card(&secured).expect("reduces");
        assert!(view.security_required);
        let mut required = card();
        required["securityRequirements"] = serde_json::json!([{"bearer": []}]);
        let view = reduce_agent_card(&required).expect("reduces");
        assert!(view.security_required);
    }

    #[test]
    fn reduction_records_extensions_modes_and_examples() {
        let mut rich = card();
        rich["capabilities"]["extensions"] =
            serde_json::json!([{"uri": "https://example.com/ext/pay", "required": true}]);
        rich["defaultInputModes"] = serde_json::json!(["text/plain", "application/json"]);
        let view = reduce_agent_card(&rich).expect("reduces");
        assert_eq!(
            view.extensions,
            vec!["https://example.com/ext/pay".to_string()]
        );
        assert_eq!(
            view.default_input_modes,
            vec!["application/json".to_string(), "text/plain".to_string()]
        );
        assert_eq!(view.examples, vec!["Review PR 12".to_string()]);
    }

    #[test]
    fn an_interface_naming_no_url_does_not_satisfy_the_frame() {
        // An empty url names nothing, in either shape, and a
        // declared list with no usable url does not fall
        // back to a stale top-level one.
        let empty = serde_json::json!({
            "name": "A", "version": "1",
            "supportedInterfaces": [{"url": ""}],
            "url": "https://example.com/stale"
        });
        assert!(reduce_agent_card(&empty).is_err());
        let typo = serde_json::json!({
            "name": "A", "version": "1",
            "supportedInterfaces": [{"uri": "https://example.com/a"}],
            "url": "https://example.com/stale"
        });
        assert!(reduce_agent_card(&typo).is_err());
    }

    #[test]
    fn contract_hash_ignores_the_description_only() {
        let base = reduce_agent_card(&card()).expect("reduces");
        let skill = &base.skills[0];
        // review-diff is the fixture's second skill; the
        // reduction sorts it to the front.
        let mut reworded = card();
        reworded["skills"][1]["description"] = serde_json::json!("Looks at a diff.");
        let view = reduce_agent_card(&reworded).expect("reduces");
        assert_eq!(view.skills[0].contract_sha256, skill.contract_sha256);
        assert_ne!(view.skills[0].description, skill.description);
        let mut retagged = card();
        retagged["skills"][1]["tags"] = serde_json::json!(["code", "review"]);
        let view = reduce_agent_card(&retagged).expect("reduces");
        assert_ne!(view.skills[0].contract_sha256, skill.contract_sha256);
    }

    #[test]
    fn reduction_rejects_documents_outside_the_frame() {
        assert!(reduce_agent_card(&serde_json::json!({"hello": "world"})).is_err());
        assert!(reduce_agent_card(&serde_json::json!([1, 2])).is_err());
        let mut no_name = card();
        no_name.as_object_mut().expect("card").remove("name");
        assert!(reduce_agent_card(&no_name).is_err());
        let mut no_interfaces = card();
        no_interfaces
            .as_object_mut()
            .expect("card")
            .remove("supportedInterfaces");
        let err = reduce_agent_card(&no_interfaces).expect_err("no interface refuses");
        assert!(err.contains("no interface"), "error was {err}");
    }

    #[test]
    fn reduction_rejects_duplicate_skill_ids() {
        let mut dup = card();
        dup["skills"][1]["id"] = serde_json::json!("review-diff");
        let err = reduce_agent_card(&dup).expect_err("duplicates refuse");
        assert!(err.contains("review-diff"), "error was {err}");
    }
}
