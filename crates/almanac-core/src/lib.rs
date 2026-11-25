//! Core library for almanac, the drift gate for agent discovery
//! catalogs.
//!
//! This crate holds the catalog model, the generation rules, the
//! baseline format and the drift logic. It is kept free of network
//! and filesystem code so every rule is testable as a pure function.
//! The CLI crate owns argument parsing, file access and the surface
//! probes, and calls into this crate to decide.
//!
//! The design record that fixes this crate's behaviour lives in
//! docs/DESIGN.md in the repository. The drift model compares three
//! sides, the published catalog, the baseline recorded at generation
//! time and the live surface probed at check time, and classifies
//! every divergence by the severity defined here.

pub mod a2a;
pub mod baseline;
pub mod catalog;
pub mod drift;
pub mod hash;
pub mod jcs;
pub mod metadata;
pub mod openapi;
pub mod snapshot;
pub mod statement;
pub mod trust;
pub mod validate;

use serde::{Deserialize, Serialize};

/// How serious a drift finding is.
///
/// The check exits nonzero when any finding sits at or above the
/// publisher's configured fail severity, which defaults to
/// [`Severity::Major`]. `Critical` findings always fail and cannot
/// be lowered by configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    /// Report only. The publisher hears about it and nothing fails.
    Minor,
    /// Fails by default. The publisher can lower it in configuration.
    Major,
    /// Always fails. The catalog promises what the server no longer
    /// serves, or the server serves what the catalog never announced.
    Critical,
}

/// Read a boolean where a document means one (the
/// twenty-first slice's design in DECISIONS.md). A
/// JSON boolean reads as itself. A string carrying
/// one of the YAML 1.1 boolean spellings, the spec's
/// case forms of yes, no, on, off, true, and false,
/// reads as that boolean: serde_yaml reads YAML 1.2
/// core scalars, so those spellings arrive as
/// strings from YAML documents that mean them as
/// booleans, and a required parameter marked yes is
/// a required parameter. The single letters y and n
/// stay out, likelier stray data than intent. Any
/// other value reads as no boolean. The helper is
/// consulted only where a boolean is being read;
/// strings anywhere else are never converted.
pub fn read_bool(value: &serde_json::Value) -> Option<bool> {
    if let Some(flag) = value.as_bool() {
        return Some(flag);
    }
    match value.as_str()? {
        "true" | "True" | "TRUE" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" => Some(true),
        "false" | "False" | "FALSE" | "no" | "No" | "NO" | "off" | "Off" | "OFF" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::read_bool;

    #[test]
    fn read_bool_reads_the_full_spelling_table() {
        // Every form the YAML 1.1 word set admits, in
        // both directions, plus the JSON passthrough and
        // the values that read as no boolean. A
        // transposed or dropped row fails here even
        // where the call-site tests sample only some
        // forms.
        for word in [
            "true", "True", "TRUE", "yes", "Yes", "YES", "on", "On", "ON",
        ] {
            assert_eq!(
                read_bool(&serde_json::Value::String(word.into())),
                Some(true),
                "{word}"
            );
        }
        for word in [
            "false", "False", "FALSE", "no", "No", "NO", "off", "Off", "OFF",
        ] {
            assert_eq!(
                read_bool(&serde_json::Value::String(word.into())),
                Some(false),
                "{word}"
            );
        }
        assert_eq!(read_bool(&serde_json::json!(true)), Some(true));
        assert_eq!(read_bool(&serde_json::json!(false)), Some(false));
        for value in [
            serde_json::json!("y"),
            serde_json::json!("n"),
            serde_json::json!("Y"),
            serde_json::json!("N"),
            serde_json::json!("yEs"),
            serde_json::json!("perhaps"),
            serde_json::json!(""),
            serde_json::json!(1),
            serde_json::json!(0),
            serde_json::Value::Null,
        ] {
            assert_eq!(read_bool(&value), None, "{value}");
        }
    }
}
