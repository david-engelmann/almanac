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
  artifacts and the live surface. A protected surface is generated
  from its server card instead, with the authentication metadata
  recorded alongside, unless a publisher credential is available
  through the environment, in which case the surface is probed
  live and the baseline records what the walk actually served. OpenAPI documents the site serves are
  announced as API entries, and A2A agent cards as agent
  entries, and the baseline records their operations and
  skills so check can compare them fact by fact. The
  publisher's own almanac.toml speaks first for the host
  name and for an entry's description, tags, and queries,
  and can suppress an entry from the catalog outright with
  a recorded reason. Where no stronger source names the
  host or describes an entry, the repo's own manifests
  supply the words as the lowest confidence editorial
  source, flagged in the run's notes. The run ends by
  printing a generation report naming the entries it
  announced and which input produced each one, the files
  it wrote, the entries it suppressed with their
  reasons, and every note and warning the run produced.
- validate checks a catalog beyond schema, covering URN anchoring,
  exactly one of url or data per entry, representative query
  counts, dual path coherence and trust manifest shape and expiry.
- check is the drift gate for CI, in an offline repo mode and a live
  mode that probes the deployed surface. A protected surface is
  checked against its server card as declared evidence, or probed
  live when a publisher credential is available through the
  environment, and a refused credential is named in the run's
  output rather than silently falling back. It also cross checks the
  MCP endpoints a site's agents.json names against the ones the
  catalog announces, because two public claims about one surface
  should not disagree.
- migrate takes a publisher stranded on the legacy path, verifies
  their ai-catalog.json against the live surface and the site's
  own files instead of copying it, and emits the dual path pair,
  the baseline and the CI wiring for the entries that verify.
  Entries that fail verification are reported, never silently
  carried and never silently dropped.

## State

The design gate was confirmed on 2026-11-20 after a cheapest failure
test against a real shipping MCP server, where the clean check
passed with zero findings and six planted breaks were caught at
their expected severities. The first two build
milestones are built, the stdio and Streamable HTTP probes, the
authenticated surface path, the drift engine behind them,
generation from the server card for protected surfaces, and the
validate command, and the first milestone has passed through three
independent checkpoint reviews whose findings are fixed or recorded
as named work in DECISIONS.md. All four commands are now built,
migrate included, and the agents.json cross check completes the
five design amendments. The generation roadmap is under way,
with OpenAPI documents and A2A agent cards as its first
inputs beyond the probe and skill files. The design record
lives in docs/DESIGN.md. No release exists yet.

## License

Dual licensed under MIT or Apache-2.0, at your option.
