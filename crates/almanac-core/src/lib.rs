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

pub mod baseline;
pub mod catalog;
pub mod drift;
pub mod hash;
pub mod openapi;
pub mod snapshot;
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
