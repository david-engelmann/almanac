//! Renamed stdio fixture server. Identical to fixture_server.rs
//! except the render tool is renamed in this source, and the
//! binary is rebuilt from it. The drift gate must catch the
//! rename as critical in both directions.

const RENDER_TOOL: &str = "render_scenes";

include!("shared/fixture_common.rs");
