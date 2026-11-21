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

almanac generates a publisher catalog from the artifacts a project
actually ships, its MCP server card, its OpenAPI document, its skills
directories and its live server surface, and writes both paths from
one model so the pair can never quietly split. That generation run
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
- validate checks a catalog beyond schema, covering URN anchoring,
  exactly one of url or data per entry, representative query counts
  and dual path coherence.
- check is the drift gate for CI, in an offline repo mode and a live
  mode that probes the deployed surface.
- migrate takes a publisher stranded on the legacy path, verifies
  their ai-catalog.json against the live surface instead of copying
  it, and emits the dual path pair, the baseline and the gate.

## State

The design gate was confirmed on 2026-11-20 after a cheapest failure
test against a real shipping MCP server, where the clean check
passed with zero findings and six planted breaks were caught at
their expected severities. The design record lives in docs/DESIGN.md
and the locked decisions in DECISIONS.md. The first build milestone
is the Streamable HTTP probe transport and the authenticated surface
path, with a source level rename test owed as the stronger form of
the wire level breaks used in the test. No release exists yet.

## License

Dual licensed under MIT or Apache-2.0, at your option.
