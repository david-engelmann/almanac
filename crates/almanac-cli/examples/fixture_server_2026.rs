//! Current-revision stdio fixture server. It speaks the
//! stateless 2026-07-28 shape only: server/discover is its
//! entry point and initialize is refused as method not found,
//! so a probe that still opens with the handshake fails
//! against it. See fixture_common.rs for the surface.

const RENDER_TOOL: &str = "render_scene";
const SPEAKS_DISCOVER: bool = true;

include!("shared/fixture_common.rs");
