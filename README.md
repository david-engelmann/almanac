# almanac

almanac is a drift gate for agent discovery catalogs. A publisher's
catalog is the listing agents read before they ever reach the server,
published in the Agentic Resource Discovery format at
/.well-known/ard.json and on the legacy path at
/.well-known/ai-catalog.json. When the catalog and the server
disagree, agents get sent to tools that no longer exist and never
learn about the ones that do. almanac exists to catch that the moment
it happens.

## What it does

almanac generates a publisher catalog from a project's live server
surface and its skill files, writing the MCP server card as it goes,
and writes both paths from one model so the pair can never quietly
split. That generation run
also records a baseline of what the server served. Every later check
compares the published catalog, the baseline and the live surface,
and fails in CI when they diverge. A renamed tool, a tool the catalog
never announced, a changed input schema, a skill file edited after
generation, a catalog entry whose referenced artifact no longer
matches its recorded digest, each one is a named finding with a
severity, and the check exits nonzero at or above the severity the
publisher sets.

Generation is the on-ramp. The gate is the product.

## Commands

- generate writes the dual path catalog and the baseline from repo
  artifacts and the live surface.
- validate, not yet built, checks a catalog beyond schema, covering
  URN anchoring, exactly one of url or data per entry, representative
  query counts and dual path coherence.
- check is the drift gate for CI, in an offline repo mode and a live
  mode that probes the deployed surface.
- migrate, not yet built, takes a publisher stranded on the legacy
  path, verifies their ai-catalog.json against the live surface
  instead of copying it, and emits the dual path pair, the baseline
  and the gate.

## State

The design gate was confirmed on 2026-11-20 after a cheapest failure
test against a real shipping MCP server, where the clean check
passed with zero findings and six planted breaks were caught at
their expected severities. The first build milestone is built, the
stdio and Streamable HTTP probes, the authenticated surface path and
the drift engine behind them, and it has passed through three
independent checkpoint reviews whose findings are fixed or recorded
as named work in DECISIONS.md. The design record lives in
docs/DESIGN.md. No release exists yet.

## License

Dual licensed under MIT or Apache-2.0, at your option.
