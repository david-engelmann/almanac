# almanac decisions

Locked decisions for this repo. Changes go through the owner seat
and Main, not through a lane's own initiative.

## Name and shape

- Name. almanac, locked by David on 2026-11-21, superseding beacon.
  Repository david-engelmann/almanac. The full name evidence for
  gazette, fingerpost, almanac and beacon is recorded in section 1
  of docs/DESIGN.md.
- Crates. almanac-core holds the catalog model, the generation
  rules, the baseline format and the drift logic, kept free of
  network and filesystem code. almanac-cli holds argument parsing,
  file access and the surface probes. The binary is named almanac.
  The bare almanac crate name belongs to a dormant calendar tool, so
  the suffixed crate names carry the build.
- License. MIT OR Apache-2.0.
- Rust 2024 edition, toolchain pinned in rust-toolchain.toml.

## Product rulings, Main, 2026-11-20

- Amendment A. The headline inverts. Drift gate first, generator
  second, in the README and the repo description.
- Amendment B. The MCP server card is a first class artifact,
  generated where missing, referenced from the catalog, hashed into
  the baseline.
- Amendment C. The agents.json cross check is a version 1 drift
  class at minor severity. agents.txt files are read and cross
  checked, never generated in version 1.
- Amendment D. Trust manifest signing is deferred. Version 1
  validates trust manifest shape and expiry only, with no JWS and
  no key management.
- Amendment E. Representative queries are templated or configured,
  never model generated. Determinism binds in full, canonical JSON
  output, sorted collections, no wall clock inside the core.

## First build milestone

The Streamable HTTP probe transport and the authenticated
degradation path, in that order, then the dusk.fm reference catalog.
A source level rename test is owed as the stronger form of the wire
level breaks used in the Phase 1 test. Both are named in
docs/DESIGN.md. The transport mechanism is designed in
docs/HTTP-TRANSPORT.md, written 2026-11-21 against the MCP
transport specification and dusk.fm's recorded shape. Its load
bearing choices are a stateful probe that negotiates and records
the protocol version, no GET stream, both response framings parsed,
the 401 branch treated as a ladder into RFC 9728 and RFC 8414
metadata that is hashed into the baseline, and declared evidence
only labelling for protected surfaces.

## Access

Git transport uses the per repo deploy key at ~/.ssh/deploy-almanac,
fingerprint SHA256:761GDSnV2XQHoXX9NjsMF5bEGdc+SuMc+o9By8wvVzA.
The repo was created on 2026-11-21 through the stored credential.
Key registration returned 403 on that day because the repo is not
yet in the token's repository access list, so registration holds
until David's credential scope act lands, in the same pattern as
the other seats.

## Dependencies beyond the starting set (2026-11-21)

The design record allows serde, serde_json and a small HTTP
client, and asks for a written reason for anything else. Three
additions carry theirs. sha2, in the core crate, because digest
binding is a requirement of the ARD draft itself and the hashes
are the baseline's evidence; a hand rolled SHA-256 would be the
riskiest code in the repo. ureq with rustls, in the CLI crate,
is the small HTTP client the design names, blocking and free of
an async runtime. toml, in the CLI crate, added at the milestone
one checkpoint, because suppressions live in almanac.toml by the
design record's own ruling, and a hand rolled parser for a file
publishers write would be a quiet source of misread
suppressions, which are exactly the findings a gate must never
hold back by accident.

## Milestone one checkpoint (2026-11-21)

Milestone one passed through the checkpoint rule's three reviews
on the day it landed. An independent red team re-audit, a spec
fidelity axis and a standards axis read the finished state
against docs/DESIGN.md, docs/HTTP-TRANSPORT.md and this file.
Between them they returned one critical, seventeen major and
thirty minor findings, with overlaps where two axes caught the
same defect. The critical finding was real. In
live mode the published catalog's announced tools were never
compared, so a catalog announcing a tool that does not exist
passed the gate. Every finding was either fixed in the fix pass
that followed or is recorded below as a named deferral. Nothing
was silently dropped, which was itself one of the findings.

Fixed in the fix pass. The catalog against baseline comparison
now runs in every mode. A legacy only site gets its major class
instead of a could not run exit. The card is compared in
declared evidence mode. The digest check is three sided, served
bytes against the entry digest and against the baseline hash.
Suppressions work, with reasons required and expiry enforced.
Repo mode exists. A new skill file is an unannounced capability.
Provenance is recorded per entry. The probe resolves its
endpoint from the catalog entry's card when no target is named.
Pagination is capped on both transports, the SSE reader takes
CRLF framing, the stdio request timeout is a deadline, the
handshake no longer swallows method not found, a failed probe no
longer reports a phantom authentication change, metadata fetches
follow redirects, redirect answers are recorded with their
target, and catalog controlled paths can no longer escape the
site directory or match a look-alike host. Grading decision
recorded here. A missing ai-catalog.json beside ard.json is
major path divergence, the mirror of the legacy only class. A
missing ard.json with a legacy catalog present is that major
class. Entries that differ between the paths stay critical.

Named deferrals, each with its reason.

- Generation for protected surfaces, from the server card. The
  error message at the point of refusal has named this since
  milestone one, and it is milestone two work, because the card
  reader it needs is the same one the declared evidence check
  now uses.
- The agents.json cross check, Amendment C. An open version 1
  item, blocked on a firsthand read of the agents.json schema,
  the same evidence standard the ARD work used. It lands once
  the schema has been read, not from a guess at its shape.
- Generation inputs beyond the live probe and one skill file.
  OpenAPI documents, A2A cards, repo metadata, publisher config
  and the full generation report are the generation roadmap.
  The README now says plainly what generation reads today.
- Three minor classes. Display, tag and ordering changes inside
  a declared artifact, the served media type of the catalog, and
  trust manifest expiry. The first needs field level card
  comparison beyond the digest. The second needs the checker to
  observe a served catalog, which local mode never does. The
  third lands with the validate command, where Amendment D puts
  trust manifest handling.
- The HTTP source level rename. The stdio rename is a source
  level rebuild, as owed. The HTTP rename remains a fixture mode
  toggle, because the HTTP fixture is test harness code rather
  than a shipped server binary, and the stronger form is still
  owed if the fixture ever becomes a binary.

## Milestone two (2026-11-21)

Main accepted milestone one complete the same day and ordered
milestone two on the standing press. Two pieces, and the first
two deferrals above are discharged by it.

Protected generation. Generate no longer refuses a protected
surface. It reads the surface from the server card instead,
fetched from the well known card path on the endpoint's origin,
with a card already on the site as the fallback when the fetch
finds nothing. The baseline records the auth metadata the
ladder fetched, the catalog entry takes the card's own
description when it carries one, the entry's provenance reads
server-card, and the card written to the site is the card as
read, with the probed endpoint recorded on it when it names
none. A protected surface with no readable card is still an
honest failure, exit 2 with the reason named. The milestone
one auth tests patched a baseline by hand because this path
did not exist; the milestone two suite generates against the
auth fixture end to end, and the fixture now serves a card.

Validate. The validate command checks a catalog beyond schema.
Entry shape and URN anchoring at critical, empty display name
or media type and malformed digests at major, empty
descriptions, missing digests and representative query counts
outside the draft's two to five at minor, the dual path pair
compared when both files are present, and the trust manifest
judged for shape and expiry under Amendment D. The manifest
expiry is read where version 1 knows to look, an expiresAt
date on the manifest or on its inner manifest document, or an
exp claim in seconds, and the record says so rather than
pretending a wider reading. Exit 1 at major or above, warnings
alone pass, an unreadable catalog is exit 2. Validate reads a
whole site or one catalog file with --file. The trust manifest
expiry minor class deferred at the milestone one checkpoint
lands here with it, as that record said it would.

## Migrate (2026-11-21)

Main ordered migrate the same day it accepted milestone two,
the last unbuilt command. The legacy catalog is read as a
claim and verified entry by entry. An MCP entry verifies
against the probed surface, or the server card when the
surface is protected, and its announced capabilities
reconcile to the served tools with every difference named in
the report, a tool the surface dropped, a tool the catalog
never announced. A skill entry verifies when the file it
announces is in the site and its frontmatter parses. Any
other url entry verifies when the artifact it announces is
in the site. An inlined entry fails, because migration
carries only what it can check against the surface or the
site, and it is reported with that reason rather than dropped
in silence. Carried entries keep the publisher's own fields,
digests are recorded afresh from the artifacts as written,
and the baseline is recorded from what verification saw, the
same construction generate uses. The pair, the baseline and
the CI wiring are written only when at least one entry
verifies. Exit 0 when every entry verified, 1 when some
failed or none did, 2 when migration could not run, no
legacy catalog, an ard.json already present, no target named
for a server claim, or a probe that could not read the
surface. The CI wiring is a workflow that installs almanac
from its repository and runs repo mode on pull requests and
the live check weekly when the surface was probed over HTTP,
and an existing workflow file is never overwritten.

## Migrate checkpoint fix pass (2026-11-21)

The three checkpoint reviews of migrate returned no criticals
and six majors between the spec axis and the red team, and
the fix pass tightened what verification means, in four ways.
Capability and version claims are now judged, not just noted.
A tool the entry announced that the surface no longer serves,
a tool the surface serves that the entry never announced, and
a version the surface does not match are failures of the
entry's claim in the report, and the migration exits 1, while
the entry itself still carries in reconciled form, because
the server is real and dropping its entry would lose the
listing the wedge exists to save. Announced digests are
verified. A skill or artifact whose bytes no longer match the
digest the catalog announced fails, and so does an MCP entry
whose announced digest no longer matches the server card in
the site. A surface that cannot be probed no longer stops the
run. The server entries fail with the probe's reason and the
entries that verify against the site are still judged, on the
design's own principle that an unreachable announced server
is a fact about the catalog. And identity is checked. The
probed surface's name must match the MCP entry's name, a
skill file's frontmatter name must match its entry, a second
MCP entry fails because the baseline records one server, and
a duplicated identifier fails after its first occurrence.

Two readings from the first pass stand, recorded as the
version 1 level rather than silently. A non-MCP, non-skill url
entry verifies at the level of the artifact it announces
being in the site with a matching digest when one was
announced. Its content, an OpenAPI document's operations
above all, is the generation roadmap's deeper verification,
not migration's. And inlined entries fail, because an inlined
claim has no external referent this build can check it
against, and the failure is reported with that reason.

The reviews' smaller findings landed too. The legacy catalog
parses entry by entry, so one malformed entry is a reported
failure rather than a dead end. A card already in the site
earns the same disagreement warnings generate gives. Writes
land baseline first, then the pair, and replacements of an
existing card or baseline are noted in the report. Read
failures name themselves instead of reporting absence. The CI
workflow relativizes an absolute site path before writing it
down. Provenance on carried skill and artifact entries reads
migration, and the version an MCP entry gains from the
surface is noted in the report. The red team's dynamic pass
was blocked by the write fault and is owed against the fixed
build, its cases prebuilt under the audit's own directory.
(That pass was discharged firsthand on 2026-11-21 against
the fixed build, all ten prebuilt cases behaving as designed,
and reported to Main with the migrate completion report.)

## Amendment C, the agents.json cross check (2026-11-22)

The last of the five approved amendments is built. The
blocker on record was a firsthand schema read, and it is now
done: the agents.txt standard v1.0, section 5, in the
agents-txt/agents-txt repository, read 2026-11-22. agents.json
sits at the site root, a sibling of agents.txt, and its MCP
endpoints are the `url` of each object in the top level `mcp`
array, beside a `type` that is always streamable-http and an
optional description. Every block is optional and omitted
when the capability is not configured.

The check compares endpoint against endpoint. The catalog's
MCP entry names the server card, and the card carries the
endpoint it describes, so the CLI resolves each catalog entry
through the card file the site serves and the core compares
the two endpoint sets as a pure function. A disagreement in
either direction is one minor finding, kind
agents-json-mismatch. The design text names repo mode; the
comparison involves no surface, so it runs in the one shared
check path in every mode, and this record notes the reading.

Three judgement calls, recorded. Trailing slashes normalize
away, because the two files are written by different tools
and a slash is not a claim. An entry whose endpoint cannot
be determined, a card with no endpoint field, takes no part,
and the agents.json direction is judged only when the
catalog announces no MCP entry at all or at least one
endpoint could be determined, so a stdio card never produces
a phantom mismatch. And an agents.json that is present but
cannot be read as one, bad JSON, an `mcp` that is not an
array, an entry without a string `url`, is a broken public
claim: one minor finding, kind agents-json-unreadable,
reported rather than compared against. Absence of the file
is not a finding; the neighbour format is a should, not a
requirement this gate enforces.

The pointer snippet from DESIGN.md section 7 stays unbuilt.
The amendment approves the cross check, and the snippet is a
permissive "can" in the design text, not a version 1
commitment. agents.txt and agents.json are never generated,
per Rule 6.
