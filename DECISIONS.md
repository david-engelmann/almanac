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

## Generation roadmap, first slice: OpenAPI input (2026-11-23)

Main ordered the generation roadmap opened the night v1
closed, OpenAPI first. DESIGN.md section 4 names the OpenAPI
document both a live probe source and a repo input, and Rule
3 gives it ownership of paths, operations and the servers
URL for an API entry. Section 6 already assigns its drift
classes: an operation removed or renamed is Major, a new
required parameter on an existing operation is Major, and a
new OpenAPI operation is Critical, an unannounced capability.
This slice builds exactly that surface, generate and check.

Generate gains a repeatable --openapi flag naming a JSON
OpenAPI document. A document outside the site is copied to
the site root under its file name; a document inside the
site is used in place. The entry's namespace segment is the
document's file stem, validated as a plain name: the stem is
stable under content edits, where a title derived segment
would churn the identifier every time a title is touched.
The entry type is application/openapi+json, display name
and version come from the info block, capabilities are the
operation identifiers sorted, representative queries are
templated per Amendment E, and the digest binds the
document bytes as served, per the dual path contract.
Editorial Rule 4 makes the info description the entry
description; a document with no info description generates
with its title as the description and a warning on stderr,
the least invention available, rather than refusing a
spec-valid document.

The core reduction records per operation: its identifier
(the operationId, else METHOD path with the method upper
case), its required parameter names sorted (operation
parameters plus path item parameters, required true only),
its description, and a hash of the whole operation object.
The baseline gains an openapi list of these records beside
the skills and artifacts lists, with serde default so
baselines written before this slice still parse.

Check compares the served document against the baseline
operation by operation, never byte by byte. The published
entry carries the digest for external readers, but this
gate does not byte-bind the document: section 6 assigns
operation level classes to this surface, and a byte binding
would make every one of them unreachable, the same reason
the MCP surface is compared fact by fact rather than as one
blob. The classes: an operation in the baseline the
document no longer serves is Major openapi-operation-
removed (a rename lands here plus the addition below), an
operation the document serves that the baseline never
recorded is Critical openapi-operation-unannounced, a new
required parameter on a surviving operation is Major
openapi-required-parameter-added, an operation description
change is Minor openapi-operation-description-changed, and
any other change to an operation object is Major
openapi-operation-changed, mirroring the tool schema hash
class. A baselined document that is missing or no longer
parses is Critical openapi-document-unreadable with the
reason named. The catalog against baseline pass also
compares an OpenAPI entry's announced capabilities against
the baseline operation identifiers, so a hand edited
catalog announcing an operation generation never recorded
is Critical catalog-baseline-divergence, the phantom rule
the MCP surface already lives under. A required parameter
that is dropped is a relaxation and not drift, per section
6 naming only additions.

JSON documents only. YAML is the format half the world's
OpenAPI files are written in, and it is recorded here as
the slice's known limit rather than silently unsupported:
a YAML document fails generation with its reason named.
Generate still requires a surface target in this slice, so
an OpenAPI document composes with an MCP surface the way a
skill file does; generation for a REST only site, with no
MCP surface at all, is a named question for the roadmap
rather than a silent gap, because generate's probe first
shape predates the roadmap and lifting it is its own
slice. Migrate records no OpenAPI baseline for the entries
it carries; its verification stays at the level the migrate
record states, and deepening it rides the same roadmap.
A2A cards, repo metadata, publisher config input and the
full generation report remain on the roadmap behind this
slice.

### OpenAPI slice checkpoint fix pass (2026-11-23)

The three checkpoint reviews (red team, spec fidelity,
standards) landed with no Critical findings and five Major
between the red team and spec axes, all fixed here, plus
the standards axis's two Major test gaps. This section
supersedes the slice text above wherever the two disagree.

The operation comparison is three independent channels,
not a precedence ladder. The reduction records, per
operation, its required parameters, its description (else
its summary, a fallback the original text omitted), and a
contract hash of the operation object with its description,
summary and parameters removed. A change in one channel
fires its own class and can never mask a change in
another; the first build let a description edit hide a
contract change behind a minor finding, which the red team
caught. A dropped required parameter is Minor
openapi-required-parameter-dropped, a relaxation the
publisher hears about without failing the gate; the
original text called it not drift at all, which the
catch-all contradicted, as the spec review caught.
Parameters are identified by location and name ("in:name"),
the identity the spec gives them, not by name alone.

The baseline also records document level facts the first
build dropped: the info version (compared as Major
server-version-changed), the servers list (Major
openapi-servers-changed; DESIGN section 4 Rule 3 gives the
document ownership of the servers URL), the top level
security posture (Major authentication-changed), and a
hash of the components object (Major
openapi-components-changed). A schema change inside
components is reported at the entry, not attributed to one
operation, because attribution without resolving
references would be invented; unresolved $ref parameters
and schemas are the reduction's named limits.

The phantom rule now runs in both directions for this
surface: a baselined entry the catalog dropped, and a
catalog entry of this type with no baseline record of any
kind, are both Critical catalog-baseline-divergence, where
the first build only compared capabilities on a surviving
entry.

Generate plans every document before writing any of them.
Reads, reductions, identifier checks and placement all
happen first, so a refused run leaves the site untouched,
and a document can no longer overwrite another document's
source (or a pre-existing in-site document) before it is
read. A document named baseline.json is refused for the
name itself. Placement is resolved on canonical paths, so
an in-site document is announced at the URL the checker
will look for however its path was spelled.

Migrate now records an OpenAPI baseline for the entries it
carries, superseding the paragraph above: an entry of this
type is verified as the surface it is (the document is
reduced, its operation and version claims are judged the
way the MCP entry's are, and the entry carries reconciled),
so a migrated document lives under the same gate as a
generated one instead of being byte-bound by the generic
artifact path while generated documents are not.

## Applicability audit charges (2026-11-23)

David ordered an applicability audit of the fleet's format
repos against the current standards landscape (audit at
~/workspace/research/knowledge-formats-lane/applicability-audit-2026-11-23.md).
Almanac took three charges. The first two are builds; the
third is watch items only.

### Charge 1: the server card at both locations

SEP-2127 is Final, and its card location is
<endpoint>/server-card, explicitly not the well known path
this tool was built on. Major scanners still request
/.well-known/mcp/server-card.json, so one location or the
other alone loses a reader population, the same shape as
the dual path catalog contract. Almanac now serves both,
with the endpoint path normative:

- Generate and migrate write the card bytes to both site
  locations when the endpoint is an HTTP URL on the site's
  own origin, the normative copy at the endpoint's path
  plus /server-card, the legacy copy at the well known
  path. A stdio target, or an endpoint on another origin,
  gets the well known copy only, because the normative
  location belongs to the server's own origin and this
  site is not it.
- The catalog entry keeps naming the well known path,
  which exists for every transport. The baseline records
  the normative copy as a second artifact of the same
  entry with the same hash, so the existing digest binding
  covers both copies and a divergence between them is
  drift. The digest story is otherwise unchanged.
- The card fetch behind protected generation tries the
  normative location first and falls back to the well
  known path, so a publisher who serves only the Final
  location is read, and one who serves only the legacy
  path still is.
- The shared URL to path mapping accepts both URL
  schemes for the publisher's own host. Check uses it to
  resolve artifact URLs into the site, and generate and
  migrate use the same mapping to place the normative
  copy. It already bound the host; a local or staging
  surface served over plain http is the same publisher's
  site.

One gap is recorded here rather than built: the Final
card schema (a reverse-DNS name, a remotes list, a
description of at most 100 characters, no tool list)
differs from the card shape generate writes today, which
carries the probed tool surface and an endpoint field.
Reconciling the generated card with the Final schema is a
named follow-up for the roadmap, not part of this charge,
which was locations only.

### Charge 3: watch items, no builds

- The IETF DAWN discovery draft, draft-bu-dawn-ard-discovery,
  one of three -00 individual drafts presenting ARD to the
  DAWN working group on 2026-11-19. ARD itself is unchanged
  at v0.91, the drafts are not adopted working group
  documents, and the chairs kept catalog work out of DAWN
  scope for now. Watch: if the discovery draft is adopted
  or its manifest shape diverges from the v0.91 envelope,
  the catalog model gets a second look. Until then the
  baseline stands and nothing is rebuilt on a draft.
- A2A 1.1, a release candidate at this writing, defines
  the agent card media type application/a2a-agent-card+json.
  When the generation roadmap's A2A slice is built, its
  entries carry that media type (and the 1.0 card shape
  stays readable). Recorded now so the slice opens with
  the current type rather than the one current when the
  roadmap was written.

### Charge 1 checkpoint fix pass (2026-11-23)

The three checkpoint reviews found one Critical, one
Major and a set of Minors, all fixed before landing. The
Critical mattered most, and it was in the gate itself:
the digest binding keyed each artifact's current digest
by entry identifier and matched the first entry with that
identifier, so with two artifacts on one entry the second
copy's bytes were read, hashed and silently discarded.
Tampering with or deleting the normative copy alone would
have passed check clean, and tampering with the legacy
copy accused the normative one in its place. The binding
is now keyed by artifact URL end to end, with a core test
that checks each copy at its own URL, and the slice's
integration tests now exercise the tamper in both
directions for generate and for migrate. The reviews also
found migrate's dual write untested (now covered, which
needed the URN anchor check to match the domain as a
prefix segment, because a domain carrying a port is colon
bearing and the old split-on-colons comparison could
never anchor it), the fetch's fallback keyed on transport
failure only (a normative answer now counts only when it
parses as a JSON object, so a catch-all front end's 200
page cannot shadow a valid legacy card), the normative
URL built twice in two files (one constructor in the
probe module now serves fetch and publish, and it cuts a
query string or fragment before appending the card path),
a displaced doc comment (restored to the function it
describes), and the fetch order unpinned when both
locations serve (the fixture now serves different cards
at the two locations in one mode, and a test records
which one generation read). Two Minors stand as recorded
limits: the host comparison in the URL mapping is case
sensitive, as it was before this charge, so an endpoint
spelled with a differently cased host gets the well known
copy only, and generate and migrate now say so in their
output when an HTTP endpoint does not map into the site;
and the card copies are written before the baseline, the
ordering the legacy card always had, so a run killed
mid-write can leave a copy no baseline records yet.

### Charge 2: the transport re-baseline to MCP 2026-07-28

Core MCP is now revision 2026-07-28, frozen until the
next revision targeted 2027-03-30. The revision is
stateless: no initialize handshake, no sessions, identity
carried per request in a _meta block, and server/discover
mandatory as the entry point. The transport work targeted
the 2025 revisions and called the stateless shape a later
addition; the later addition is now the baseline, and
both probes are re-baselined to it:

- Discover first, on both transports. The request carries
  _meta with io.modelcontextprotocol/protocolVersion
  naming 2026-07-28, the client identity and the client
  capabilities under the keys the revision names. A result
  is read for the same fields the initialize result
  carried, serverInfo and protocolVersion (one snapshot
  construction serves both ladders), and a result that
  omits the version is recorded under the offered
  revision. The lists then walk statelessly: no session
  header, no protocol version header, no initialized
  notification, no DELETE courtesy.
- The fallback is the ladder as built, and its trigger is
  evidence based. Only an answer that says the method is
  unknown, a JSON-RPC method not found error or an HTTP
  404 or 405, proves the server predates discover, and
  only that answer is walked through the initialize
  handshake offering 2025-06-18, with session echo, the
  version header, the notification and the DELETE,
  unchanged. Every other discover failure is a probe
  failure at the server/discover step (see the fix pass
  below for why the trigger narrowed at the checkpoint).
  Every pre-existing fixture exercises the fallback path,
  because none of them knows discover.
- A 401 at either entry, or mid walk under either shape,
  routes to the RFC 9728 and RFC 8414 metadata ladder,
  whose shape is unchanged. A protected current-revision
  server is therefore found at its discover call, not by
  probing a handshake it does not speak.
- The snapshot records which shape the surface took, and
  the protocol version goes into the baseline either way,
  so a server migrating between shapes between generation
  and check fires the server version class, as designed.
- The behavior is pinned, not just described. An HTTP
  fixture mode speaks only the current revision (discover
  answered, initialize refused as method not found, lists
  served with no session) and logs every method it
  receives; the test asserts discover was first and
  initialize was never sent. A stdio fixture binary speaks
  discover only, from the same shared fixture body with
  the handshake arms switched off, and a test generates
  and checks against it.
- docs/HTTP-TRANSPORT.md is re-baselined to match, its
  protocol posture and probe sequence rewritten around
  discover first, and DESIGN.md's generation input line
  names the discover entry beside the handshake.

### Charge 2 checkpoint fix pass (2026-11-23)

The three checkpoint reviews found no Critical and three
Majors, all on the fallback's shape, all fixed before
landing. As first built, any discover failure fell back to
the initialize ladder, silently. The reviews showed what
that buys: a dual-stack server whose discover 500s at
check time gets walked down its legacy ladder, and every
difference between the two surfaces reports as drift that
never happened; a generate during a discover outage
records the legacy surface as the baseline of record with
nothing saying so; and a server answering initialize with
the current revision's string could pass fully clean while
its announced discover surface was never probed. The
fallback is now evidence based, on both transports: only
a method not found answer (the JSON-RPC error, or HTTP
404 or 405) proves the server predates discover. A 500, a
different error, a transport failure, a result that is
not an object, or a result naming no server is a probe
failure at the server/discover step, with the break
named. The same pass closed the transports' divergence
on a null discover result (HTTP accepted it, stdio fell
back; both now fail the step), put the _meta block on the
stdio stateless list calls (only the discover call had
carried it, against the revision's per-request identity
and this record's own words), and corrected the
snapshot's stateless flag to describe the walk's shape
rather than the negotiated string, since the baseline
never records it and a statefully walked 2026-negotiating
server was mislabelled. The fixtures now enforce the
stateless shape instead of assuming it: the HTTP 2026
mode refuses session and version headers and requires
_meta on discover and the lists, and the discover
speaking stdio fixture requires _meta on its list calls,
so a probe that drifts from the shape fails the suite
instead of passing on code reading. Two supports the
narrowed trigger needed: the HTTP round trip now parses a
JSON-RPC message on any status, because servers commonly
carry the method not found error in a 400 body and the
fallback decision reads it, and the drift engine's
unreachable mapping names the entry step of either
revision, discover or initialize, since a failure there
means the announced server could not be reached at all.
The HTTP fixture's session refusal now guards only the
methods the fixture serves, because a method it does not
implement is answered method not found whatever the
session state, the way a real framework dispatches; the
list calls it exists to police are guarded exactly as
before.

## Card schema reconciliation (2026-11-24)

Charge 1 put the card at both locations and recorded the
shape as a follow-up. This slice closes it: the card
almanac writes is now the SEP-2127 Final document, and
every reader takes both shapes, because the cards already
published across the ecosystem, including almanac's own
earlier output, predate the Final.

The Final shape, from the extension's schema and the
cards publishers serve today: a $schema field naming
https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json,
a name in reverse DNS form with exactly one slash (the
publisher's domain reversed, a slash, the server's name),
a title, a version, a description of at most 100
characters, and for a remote server a remotes array whose
entries carry a type (streamable-http or sse), a url, and
the supported protocol versions. No tools, no resources,
no prompts: the Final leaves primitives out on purpose,
because a static document cannot represent a set that
changes with the caller. The schema leaves additional
properties open, so a card could carry the old fields
beside the new ones and still validate, but the text's
must not is normative even where the schema is permissive,
so almanac does not smuggle its legacy fields into a Final
card.

What generate and migrate write: the Final shape, from
the probed snapshot. The name is the site domain reversed
plus the server name, the description is the synthesized
"The <name> MCP server." cut at 100 characters, and an
HTTP target adds one remote naming the endpoint and the
one protocol version the probe negotiated, because the
card claims only what the probe observed. A stdio target
has no remote to name, so its card carries identity alone.

What the readers accept, in one shared reduction: a card
is read as Final shaped when it carries the Final's own
fields, a $schema field or a remotes array. The server
name is the card's name field: for a Final card, what
follows the first slash, since the form is a reversed
domain, one slash, and then the server's name with any
slashes of its own kept; for an earlier card, the name
plainly, exactly as written. One remote is primary, the
first streamable-http remote that names a url, else the
first remote that names one, so the endpoint and the
protocol version come from the same statement: the
endpoint is the primary remote's url on a Final card,
where remotes are authoritative, and the legacy endpoint
field on an earlier one, each falling back to the other;
the protocol version is the legacy protocolVersion field,
or the primary remote's first supported version. Tools
are read only when the card carries a tools field, and
their absence is a fact of a different kind than an
empty list.

That last distinction is the slice's real consequence,
and it is recorded plainly: a Final card declares identity
and endpoints, never capabilities. Declared evidence mode
therefore compares a Final card at identity level only;
its tool comparisons are skipped, not run against an
empty set, because running them would report every
baselined tool as vanished from a card that never claimed
to list tools. Protected generation from a Final card
records the server's identity and announces no
capabilities, and migration carries a protected entry's
tool claims unverified, saying so, when the only card is
a Final one. Capability evidence for protected surfaces
is live evidence, which is the credential-aware probe's
job, already named as the first post-launch enhancement.
Legacy cards keep the full declared comparison, so
publishers whose cards predate the Final lose nothing.

### Card schema reconciliation checkpoint fix pass (2026-11-24)

The three checkpoint reviews found no Critical and three
Majors, all in the reduction's edges, all fixed before
landing. The largest was in migration: an earlier gate
that fails an MCP entry whose surface serves no tools ran
before the gated judgment and was itself ungated, so a
Final card's empty list failed the entry outright and
dropped it, the exact outcome this slice exists to
prevent; the gate now asks whether the surface was
enumerated first. Beside it, enumeration itself was
redefined on the field's presence rather than its shape:
a tools field present but not an array is a corrupted
statement and reduces to an empty list the comparisons
fire on, where only a card with no tools field at all is
unenumerated. The name reduction is keyed on the card's
shape, because a legacy plain name containing a slash is
the server's whole name while a Final name's first slash
is the separator; the endpoint and protocol version now
come from one primary remote, the first streamable-http
remote naming a url, so a card listing an sse remote
first no longer sends the probe to the wrong transport;
the endpoint injection appends its remote instead of
replacing the publisher's array; the card-versus-card
disagreement warning compares tools against the live
snapshot again, its original subject, since the new card
lists none; and the writer strips a port or a trailing
dot from the domain before reversing it into the
published name. The guard the slice stands on is now
pinned where it lives: a core test holds a baseline and
a catalog that record a tool against an unenumerated
card view and requires silence, then requires divergence
from the same view marked enumerated, and the
integration suite migrates against a Final card and
regenerates a declared check over a tool-recording
baseline, so deleting the guard or the migrate gate
fails the suite.

### Hosted gate fix pass (2026-11-24)

The first hosted run of this slice's pull request failed
three integration tests, and the failures were the
slice's own blind spot: the enumeration distinction was
recorded on the snapshot and the card view, but not on
the baseline. A baseline recorded from a Final card
holds no tool record, yet the catalog against baseline
comparison, the card against baseline comparison, and
the live primitive set comparisons all read its empty
record as a removal, so a site generated or migrated
from a Final card failed its own check. McpBaseline now
records capabilities_enumerated from the snapshot that
produced it, defaulting to true for baselines written
before the field existed, and those three comparisons
run only against a record that enumerated; identity and
authentication compare either way. Two of the slice's
tests also repaired their own premises: the declared
mode divergence test now diverges an enumerating card in
the earlier shape, since the published card no longer
carries a tools array to edit, and the Final generation
test reads an entry whose capabilities field is omitted
when empty, as the catalog model has always written it.

## Generation roadmap, second slice: A2A input (2026-11-24)

The second generation input is the A2A agent card, the
second slice of the generation roadmap the applicability
audit's third charge pointed at: A2A core stands at 1.0.1,
and the 1.1 release candidate adds the media type this
slice announces, application/a2a-agent-card+json, which
the ARD entry-type registry already names. The card shape
was read from the specification's field table as carried
in the audit report and two independent schema guides. A
1.0 card is a JSON document at /.well-known/agent-card.json
whose required fields are name, description, version (the
agent's own version, not the protocol's), an ordered
supportedInterfaces array whose entries carry url,
protocolBinding, and protocolVersion, a capabilities
object (streaming, pushNotifications, extensions,
extendedAgentCard), default input and output modes, and a
skills array whose entries carry id, name, description,
and tags, with examples and modes optional. Cards from
the 0.3 line still circulate with a top-level url,
preferredTransport, and protocolVersion instead of the
interfaces array, and the reduction accepts that shape
too, the way the card reduction accepts both MCP card
shapes. An agent card is a declared artifact, like an
OpenAPI document, and not a live probe: almanac speaks
no A2A RPC in this slice, and the card is compared fact
by fact against the baseline the way the OpenAPI document
is compared operation by operation, never byte by byte.

Generate takes a repeatable --agent-card flag naming a
card file, with the OpenAPI slice's placement rules: a
file outside the site is copied to the site root under
its file name, a file inside is used in place. The entry
is typed application/a2a-agent-card+json with identifier
urn:air:<domain>:a2a:<stem>, the stem being the file stem
validated as a plain name. The display name and version
come from the card, the description from the card's
description per Rule 4, and a card with no description
generates with its name as the description plus a warning
on stderr, mirroring the OpenAPI title fallback.
Capabilities are the skill identifiers, sorted.
Representative queries are the skills' own examples where
the card carries them, the publisher's words rather than
anything generated, and the two standing templates
otherwise. The digest is the digest of the card bytes as
served, and the provenance is agent-card-file.

The reduction refuses a document that is not a sound
frame, naming the reason: not a JSON object, no name, no
version, or no interface naming a url. From a sound frame
it records the agent name and version, the interfaces in
order as url, binding, and protocol version triples, the
skills by identifier with a hash of each skill's
description and a hash of the rest of the skill object,
the capability flags, and whether the card requires
security, judged from a non-empty securityRequirements
list or a non-empty securitySchemes map. A 0.3 card's
top-level url and protocolVersion synthesize its single
interface. The baseline record, A2aBaseline, sits beside
the OpenAPI one with the same serde default, so baselines
written before this slice still parse.

Check compares the served card against the baseline on
independent channels, so no change masks another. A
baselined card that is missing or will not reduce is
Critical, kind a2a-card-unreadable. A recorded skill no
longer served is Major, a2a-skill-removed; a served skill
never recorded is Critical, a2a-skill-unannounced, the
unannounced-capability rule of DESIGN section 6. An
agent version change is Major under the existing
server-version-changed kind; an interface change, the
ordered triples differing, is Major, a2a-interfaces-changed;
a security posture change is Major under the existing
authentication-changed kind; a capability flags change is
Major, a2a-capabilities-changed; a skill description
change is Minor, a2a-skill-description-changed, and any
other change inside a skill object is Major,
a2a-skill-changed. The phantom rule runs in both
directions exactly as it does for OpenAPI: a baselined
A2A entry the catalog dropped, or a published
application/a2a-agent-card+json entry with no baseline
record, or a catalog whose capabilities disagree with the
baseline's skill identifiers, is Critical
catalog-baseline-divergence.

Migration judges an A2A entry the way it judges an
OpenAPI one: the card in the site is reduced, its
version and skill claims are judged as claims, false
claims are reported failures that force exit 1, the entry
carries reconciled to the card, and an A2aBaseline is
recorded. The card's name is not judged: the entry's
identifier segment is a file stem or the publisher's own
choice, not a claim about the card's display name. A card
that will not reduce fails that entry with the reason.

### A2A slice checkpoint fix pass (2026-11-24)

The three reviews landed with no Critical findings; the
red team carried three Majors and the standards axis one,
and the fix pass closed all of them. The reduction drops
nothing the card declares any more: capabilities
extensions are recorded as a sorted identifier set and
compared under a2a-capabilities-changed beside the
flags, and the default input and output modes are
recorded and compared as a2a-modes-changed at Major. An
interface record is a named InterfaceBaseline carrying a
hash of the rest of the interface object beside its
triple, so a field like a multi-tenant marker is gated;
the baseline and the drift pass share the one conversion.
The published entry's own url and version are claims
too: check_a2a compares both against the baseline
record, and the same comparison was added to
check_openapi, whose pass shared the gap (an entry
edited to point at another document, or to announce a
version the baseline never recorded, now diverges at
Critical in both passes). The agent's name and
description are gated, one step past the OpenAPI
posture, because generate takes the entry's display name
and description from the card: a name change fires
server-version-changed at Major with the identity named,
a description change a2a-description-changed at Minor;
provider, documentation and icon links stay editorial
and ungated.

Generation plans both document kinds before it writes
anything: copies are deferred until every input is read
and planned, and every placed path (a copy destination
or an in-place source) is claimed across both kinds, so
an OpenAPI document and an agent card with the same file
name refuse the run instead of overwriting each other or
a publisher's in-site file. The representative queries
are stated plainly: the kept skills' examples in card
order, harvested by the reduction into the view, capped
at five, filled out to at least two with the standing
templates. The frame tightened: an interface url that is
empty names nothing in either shape, and a declared
interface list with no usable url no longer falls back
to a stale top-level one. A skill description that is
not a string now hashes as its JSON text, so a change
between two non-string values fires. check_a2a gained
its core unit test family beside check_openapi's,
covering agreement, both skill directions, the channel
split, the document-level classes, the unreadable class,
and the phantom rule in both directions including the
entry url and version claims.

### A2A hosted gate fix pass (2026-11-24)

PR #3's hosted gate failed two core tests that the local
combined tree had reported green, on a stale test binary
the write fault had left behind; the pushed tree was the
verdict, as with the card slice. Both failures were in
the tests, not the code. The duplicate skill test set
the second skill's identifier to the value it already
carried, a no-op since this slice's fix pass reordered
the fixture skills so the sort assertion discriminates;
it now sets the first skill's identifier, and the
duplicate exists by construction. The masking test
expected the two findings in emission order, where the
recorded contract is the report sort, severity then
kind then entry then detail (DESIGN section 6, and the
check function's own doc comment); its expectation now
stands in sorted order, the major channel first, with
the test's intent, that a description edit cannot mask
a contract change, unchanged.

## Generation roadmap, third slice: repo metadata input (2026-11-24)

Repo metadata is the last editorial source in DESIGN
rule 4's order, after config, the server card, the
OpenAPI info block, and SKILL.md, and it never wins a
capability fact (rule 2). This slice wires it into
generate as a fallback only: it creates no entries,
changes no versions, and touches neither check nor
migrate, because migration reconciles a catalog's own
announced claims and metadata is a generation input.

The sources are the three manifest files a repo carries
its name and description in: package.json (top-level
name and description), Cargo.toml (package.name and
package.description), and pyproject.toml (the PEP 621
project table's name and description; the older poetry
table is not read). The search order is the site
directory, then its parent, because the web root is
commonly a child of the repo root; within a directory
the order above holds. Fields resolve per field, the
first source carrying a non-empty value wins it, and the
reduction records which file supplied each field. A
manifest that cannot be read or parsed is skipped
without noise: a fallback source must never be able to
kill a generation run. The manifest version fields are
deliberately not read; versions are capability facts
and come from the surfaces and artifacts themselves.

Consumption in generate, each use flagged in the
generation report (the run's stderr notes, in the
existing note form) as low confidence, naming the entry
and the file the words came from. The host display name
resolves operator flag first, then the metadata name,
then the domain, as it resolved flag then domain before.
The MCP entry's description resolves server card
description, then metadata description, then the
standing template. An OpenAPI entry's description
resolves its info description, then metadata, then the
title with its warning. An A2A entry's resolves its card
description, then metadata, then the name with its
warning. Skill entries never take metadata words: the
frontmatter parser requires a description, and rule 5
makes the file the truth. The reduction lives in the
core as metadata.rs over parsed documents, so the field
paths are unit tested; the CLI reads and parses the
files (JSON directly, TOML through the toml crate it
already carries) and presents them in search order.

### Repo metadata checkpoint fix pass (2026-11-24)

The three reviews landed with no Criticals. The
standards axis found one Major: the helper had been
inserted between generate's doc comment and its clippy
allow attribute and generate itself, leaving the
eight-parameter generate without the allow its lint
needs; the helper now sits above the doc comment, which
decorates generate again. The red team found four
Minors, all closed. A stronger source's whitespace-only
description now counts as no description at all three
entry resolutions, the trim discipline the metadata
reduction already applied to its own source. The
generated server card takes the description resolved
for the catalog entry, so the two artifacts a run
writes describe the server in the same words; migrate
passes no description and keeps the standing template.
The metadata read only opens regular files of at most
one mebibyte, so a pipe or a huge file carrying a
manifest's name can neither hang nor exhaust a run.
And the low confidence notes are collected during the
run and printed only when the catalog is about to be
written, so a run that refuses never claims words for
a catalog that does not exist; the fallback warnings
still print where they fire. Two integration tests
join the slice: a parent Cargo.toml supplies the host
name and the entry and card descriptions end to end
through the TOML path, and an OpenAPI document with no
info description takes the metadata description
without the title warning. The note text is built by
one helper, the metadata imports follow the file's use
convention, and the source labels are matched with
their values rather than defaulted.
