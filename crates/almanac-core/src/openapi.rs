//! The OpenAPI reduction. An OpenAPI document is a declared
//! surface, and the drift gate compares it the way it compares
//! every surface: fact by fact, never as one blob of bytes.
//!
//! The reduction keeps, per operation, three independent
//! channels of fact: its required parameters, its description,
//! and a hash of the rest of the operation object (its
//! contract: request body, responses, security, and everything
//! else the object carries). The channels are kept apart so a
//! change in one can never mask a change in another. At the
//! document level it keeps the info version, the servers list
//! and whether the document demands security, plus a hash of
//! the components object.
//!
//! Posture on malformed input: the document's frame (the
//! openapi field, the info block, paths) must be sound or the
//! document is refused with the reason named. Inside a sound
//! frame, entries the reduction cannot read as operations (a
//! path item or operation that is not an object, a method key
//! outside the eight the spec defines, a parameter with no
//! name) are skipped, because real documents carry vendor
//! extensions and partial entries the gate has no class for.
//! Webhooks are not reduced; they are callbacks a publisher
//! receives, not operations it serves.
//!
//! Parameters and schemas referenced by $ref are not
//! resolved. Resolving references means following pointers
//! into other documents, which is a fetch this crate never
//! does. A parameter carried by reference does not appear in
//! the required list, and a schema carried by reference is
//! covered only by the components hash, which reports that
//! the shared pool changed without naming which operation it
//! touches. The record in DECISIONS.md names both limits.

use std::collections::HashSet;

use serde_json::Value;

use crate::hash;

/// One operation, reduced to the facts the gate compares.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationView {
    /// The operationId when the document carries one, else
    /// METHOD path with the method in upper case.
    pub id: String,
    /// Required parameters as "in:name", the identity the
    /// OpenAPI spec gives a parameter, sorted, deduplicated.
    /// Path item parameters and operation parameters together.
    pub required: Vec<String>,
    /// The operation description, else its summary, else empty.
    pub description: String,
    /// Hash of the operation object with its description,
    /// summary and parameters removed: the contract channel.
    pub contract_sha256: String,
}

/// One OpenAPI document, reduced.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenApiView {
    pub title: String,
    pub description: Option<String>,
    pub version: String,
    /// The servers URLs, sorted, deduplicated.
    pub servers: Vec<String>,
    /// True when the document's top level security array is
    /// present and non-empty, so the API demands credentials
    /// unless an operation says otherwise.
    pub security_required: bool,
    /// Hash of the components object, or of JSON null when
    /// the document carries none.
    pub components_sha256: String,
    /// Operations sorted by identifier.
    pub operations: Vec<OperationView>,
}

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Reduce a parsed JSON document to its view. An Err names the
/// reason the document cannot be read as an OpenAPI document,
/// in words a publisher can act on.
pub fn reduce_openapi(doc: &Value) -> Result<OpenApiView, String> {
    let doc = doc
        .as_object()
        .ok_or("the document is not a JSON object, so it is not an OpenAPI document")?;
    let version_field = doc.get("openapi").and_then(|v| v.as_str());
    match version_field {
        None => {
            return Err(
                "the document carries no openapi version field, so it is not an OpenAPI document"
                    .into(),
            );
        }
        Some(v) if !v.starts_with("3.") => {
            return Err(format!(
                "the document declares openapi version {v}, and this gate reads 3.x documents"
            ));
        }
        _ => {}
    }
    let info = doc
        .get("info")
        .and_then(|v| v.as_object())
        .ok_or("the document carries no info object")?;
    let title = info
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("the info block carries no title")?
        .to_string();
    let version = info
        .get("version")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("the info block carries no version")?
        .to_string();
    let description = info
        .get("description")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let paths = doc
        .get("paths")
        .and_then(|v| v.as_object())
        .ok_or("the document carries no paths object")?;

    let mut servers: Vec<String> = doc
        .get("servers")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|s| s.get("url").and_then(|u| u.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    servers.sort();
    servers.dedup();
    let security_required = doc
        .get("security")
        .and_then(|v| v.as_array())
        .is_some_and(|list| !list.is_empty());
    let components_sha256 = hash::sha256_json(doc.get("components").unwrap_or(&Value::Null));

    let mut seen: HashSet<String> = HashSet::new();
    let mut operations: Vec<OperationView> = Vec::new();
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        let path_params = item.get("parameters");
        for method in METHODS {
            let Some(operation) = item.get(method).and_then(|v| v.as_object()) else {
                continue;
            };
            let id = operation
                .get("operationId")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("{} {path}", method.to_uppercase()));
            if !seen.insert(id.clone()) {
                return Err(format!(
                    "operation identifier {id} appears more than once, and the gate cannot tell the operations apart"
                ));
            }
            let mut required: Vec<String> = Vec::new();
            for params in [path_params, operation.get("parameters")] {
                let Some(params) = params.and_then(|v| v.as_array()) else {
                    continue;
                };
                for param in params {
                    let required_flag = param
                        .get("required")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if required_flag && let Some(name) = param.get("name").and_then(|v| v.as_str())
                    {
                        let location = param.get("in").and_then(|v| v.as_str()).unwrap_or("");
                        let identity = format!("{location}:{name}");
                        if !required.contains(&identity) {
                            required.push(identity);
                        }
                    }
                }
            }
            required.sort();
            let description = operation
                .get("description")
                .and_then(|v| v.as_str())
                .or_else(|| operation.get("summary").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();
            let mut contract = operation.clone();
            contract.remove("description");
            contract.remove("summary");
            contract.remove("parameters");
            operations.push(OperationView {
                id,
                required,
                description,
                contract_sha256: hash::sha256_json(&Value::Object(contract)),
            });
        }
    }
    operations.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(OpenApiView {
        title,
        description,
        version,
        servers,
        security_required,
        components_sha256,
        operations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Billing", "version": "1.2.0", "description": "Charges cards."},
            "servers": [{"url": "https://api.example.com/v1"}],
            "paths": {
                "/charges": {
                    "parameters": [{"name": "tenant", "in": "header", "required": true}],
                    "post": {
                        "operationId": "createCharge",
                        "description": "Create a charge.",
                        "parameters": [
                            {"name": "key", "in": "header", "required": true},
                            {"name": "dry", "in": "query"}
                        ],
                        "responses": {"200": {"description": "ok"}}
                    },
                    "get": {"summary": "List charges."}
                }
            }
        })
    }

    #[test]
    fn reduction_reads_info_and_operations() {
        let view = reduce_openapi(&doc()).expect("reduces");
        assert_eq!(view.title, "Billing");
        assert_eq!(view.version, "1.2.0");
        assert_eq!(view.description.as_deref(), Some("Charges cards."));
        assert_eq!(view.servers, vec!["https://api.example.com/v1".to_string()]);
        assert!(!view.security_required);
        assert_eq!(view.operations.len(), 2);
        // Sorted by identifier, the fallback id sorts before the
        // operationId here (upper case before lower case).
        assert_eq!(view.operations[0].id, "GET /charges");
        assert_eq!(view.operations[0].description, "List charges.");
        assert_eq!(
            view.operations[0].required,
            vec!["header:tenant".to_string()]
        );
        let create = &view.operations[1];
        assert_eq!(create.id, "createCharge");
        assert_eq!(create.description, "Create a charge.");
        // Path item parameters join operation parameters, each
        // identified by location and name.
        assert_eq!(
            create.required,
            vec!["header:key".to_string(), "header:tenant".to_string()]
        );
    }

    #[test]
    fn contract_hash_ignores_description_and_parameters_only() {
        let base = reduce_openapi(&doc()).expect("reduces");
        let create = base
            .operations
            .iter()
            .find(|op| op.id == "createCharge")
            .expect("operation")
            .clone();

        let mut reworded = doc();
        reworded["paths"]["/charges"]["post"]["description"] = serde_json::json!("Make a charge.");
        reworded["paths"]["/charges"]["post"]["parameters"]
            .as_array_mut()
            .expect("params")
            .push(serde_json::json!({"name": "trace", "in": "header"}));
        let view = reduce_openapi(&reworded).expect("reduces");
        let changed = view
            .operations
            .iter()
            .find(|op| op.id == "createCharge")
            .expect("operation");
        assert_eq!(
            changed.contract_sha256, create.contract_sha256,
            "description and optional parameter edits stay out of the contract channel"
        );

        let mut recontracted = doc();
        recontracted["paths"]["/charges"]["post"]["responses"] =
            serde_json::json!({"201": {"description": "made"}});
        let view = reduce_openapi(&recontracted).expect("reduces");
        let changed = view
            .operations
            .iter()
            .find(|op| op.id == "createCharge")
            .expect("operation");
        assert_ne!(
            changed.contract_sha256, create.contract_sha256,
            "a responses change is a contract change"
        );
    }

    #[test]
    fn reduction_rejects_documents_outside_the_frame() {
        assert!(reduce_openapi(&serde_json::json!({"hello": "world"})).is_err());
        assert!(reduce_openapi(&serde_json::json!([1, 2])).is_err());
        let mut no_title = doc();
        no_title["info"]
            .as_object_mut()
            .expect("info")
            .remove("title");
        assert!(reduce_openapi(&no_title).is_err());
        let mut no_version = doc();
        no_version["info"]
            .as_object_mut()
            .expect("info")
            .remove("version");
        assert!(reduce_openapi(&no_version).is_err());
        let mut no_info = doc();
        no_info.as_object_mut().expect("doc").remove("info");
        assert!(reduce_openapi(&no_info).is_err());
        let mut no_paths = doc();
        no_paths.as_object_mut().expect("doc").remove("paths");
        assert!(reduce_openapi(&no_paths).is_err());
        let mut wrong_version = doc();
        wrong_version["openapi"] = serde_json::json!("2.0");
        let err = reduce_openapi(&wrong_version).expect_err("2.0 refuses");
        assert!(err.contains("2.0"), "error was {err}");
    }

    #[test]
    fn reduction_rejects_duplicate_operation_ids() {
        let mut dup = doc();
        dup["paths"]["/charges"]["get"]["operationId"] = serde_json::json!("createCharge");
        let err = reduce_openapi(&dup).expect_err("duplicates refuse");
        assert!(err.contains("createCharge"), "error was {err}");
    }
}
