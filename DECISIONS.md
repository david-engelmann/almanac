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

## Generation roadmap, fourth slice: publisher config inputs (2026-11-24)

DESIGN rule 4 puts publisher config first among the
editorial sources: display name overrides, descriptions,
tags, and representative queries come from config when
present, and the other sources derive only what config
does not say. Until this slice almanac.toml carried
finding suppressions for check and nothing for generate.
The file now also carries a [host] table with a
display_name, and [[entries]] blocks keyed by the
entry's identifier, each optionally naming display_name,
description, tags, and queries. The parse is one loader
shared with check, so a malformed file fails generate
and check the same way, and every table in the file
denies unknown fields, because a mistyped key silently
doing nothing is the failure mode config exists to
prevent. The editorial blocks' semantic validation is
generate's own, since only generate consumes them.
Duplicate identifiers, a block with no identifier, an
empty queries list, empty configured words, and a
block that both suppresses and overrides are
configuration errors and the run does not start.

Rule 1's other half lands with it: an entries block may
carry suppress with a reason, and generate then does
not announce that entry at all. A suppression without
a reason is refused, the same discipline the finding
suppressions already keep. The suppressed entry leaves
no baseline records behind, so the gate never sees a
phantom, and the run notes the suppression and its
reason. Suppressing the MCP entry also skips writing
its server card. A block naming an identifier the run
does not generate is a warning naming it: config drift
is surfaced, and an entry is never invented to satisfy
config.

### Publisher config checkpoint fix pass (2026-11-24)

The checkpoint's red team found the slice's first form
wanting in exactly the places its own discipline
pointed: suppressions were applied in the post-pass,
after the artifacts they govern were already written.
Three closures, each moving a decision to the point of
construction. A suppressed skill left its SKILL.md
served in the site, and check's walk of the skills
directory read it as an unannounced skill at Critical;
skill suppression is now decided before the file is
written, and a stale in-site copy an earlier run placed
is removed (never the file the operator named, when
that file is the in-site copy itself). A suppressed
OpenAPI document or agent card was still copied into
the site by the deferred copies; document suppression
is now decided at the top of each apply loop, before
the copy is queued. A mistyped key in a finding
suppression block silently widened it from one entry
to a whole kind; the Suppression struct and the outer
AlmanacConfig now deny unknown fields alongside the
new tables, closing the same hole one level up, where
a top-level typo previously vanished without even the
unmatched-block warning.

Around those: configured words that are empty or
whitespace are configuration errors, the trim
discipline every other source already kept; the MCP
entry's display name resolves config first and the
generated card's title carries it, so the entry and
its card cannot name the server differently; on a
protected surface, configured editorial words for the
MCP entry draw a warning that the existing card keeps
its own words, and suppressing a protected server
warns that its card remains published, while on an
open surface suppression removes the card copies
earlier runs wrote; document descriptions resolve
config first inside their build loops, so no metadata
note or title warning survives for words config
replaced; query counts outside two to five and
duplicate queries or tags warn at generation time
rather than passing silently. The post-pass now
applies overrides only.

Application is one post-pass over the built entries for
display name, description, tags, and queries, with two
parts resolved earlier for cause. The host display name
resolves flag, then config, then repo metadata, then
the domain. The MCP entry's description resolves
config first in its existing chain, before the card is
written, so the generated card keeps carrying the
entry's own words. Configured queries replace an
entry's representative queries wholesale, including the
MCP entry's: rule 4 names config first with no flag
exception, and the --queries flag remains the source
when config names no queries for the entry. Config use
is not flagged low confidence; config is the
publisher's own voice, not a fallback.

### Publisher config hosted gate fix pass (2026-11-24)

Two hosted rounds, two test-side repairs, no product
change. The first red was a helper that never existed:
the skill suppression case called a name invented at
test-writing time from the file's other helpers, and it
now builds its SKILL.md source inline, the file's
established construction. The second was a premise:
the overrides test expected the config file's host
name to win while running through a shared helper that
passes --name "Example", and rule 4's order, recorded
in the metadata slice and this one, ranks the explicit
flag first, the config second. The product code already
implements that order; the test now runs without the
flag, with a comment saying why, so the headline
behavior is proven rather than masked.

## Post-launch enhancement: the credential-aware probe (design, 2026-11-24)

DESIGN section 11 flags this as the first post-launch
enhancement, and HTTP-TRANSPORT.md states its shape in
one sentence: a publisher supplied token from the CI
secret store. The design here settles the details
before the build, in the same record-first discipline.

The credential is a bearer token the publisher already
holds, presented as an Authorization header. Almanac
runs no authorization flow: no client registration, no
authorization code exchange, no refresh, no client
credentials grant. Acquiring and rotating the token is
the publisher's arrangement with their own server; the
probe's whole part is presenting it correctly and
saying honestly what it bought.

The token reaches the probe from the environment only.
Generate, check, and migrate gain a --token-env flag
naming the environment variable to read, defaulting to
ALMANAC_TOKEN, which is the shape a CI secret store
already produces. The token is never a flag value (argv
is world-visible and CI echoes commands), never read
from almanac.toml (a committed file must not carry a
secret), and never written to the baseline, the
catalog, or the report. When the named variable is
unset or empty, the run is exactly today's run.

The header is attached only to requests to the MCP
endpoint itself, the URL the catalog names. The OAuth
discovery documents are public and may live on another
origin; they are fetched without the header, so the
token never crosses to a host that did not challenge
for it. The endpoint's redirects are not followed, as
before, so there is no redirect path for the header to
leak along either.

The ladder gains one rung. The walk runs unauthenticated
first, as today; a 401 from the endpoint, with a token
available, retries the whole walk (discover, then the
lists) with the header. A credentialed walk that
completes is a live pass: the snapshot records that a
credential was presented and accepted, and the report
labels the run a live pass without the declared
evidence qualification. A 401 anywhere in the
credentialed walk falls back to the declared path, and
the report says the presented credential was refused,
naming the step. The baseline records the surface's
declared auth posture (the protected resource and
authorization server metadata, compared as today)
whether or not a credential was presented: holding a
token is a property of the run, not of the surface, so
authentication-changed keeps its meaning across runs
with and without one.

What the credential buys is rung by rung, and the
record says so plainly. Against a baseline recorded
from a Final card, a credentialed check verifies
identity and endpoint liveness live (the announced
server actually answers and handshakes) but compares
no capabilities, because none were recorded; the full
capability comparison needs a credentialed generate or
migrate to record the live baseline first, after which
credentialed checks compare live against live. Generate
with a token on a protected surface takes the open
path: live snapshot, enumerated capabilities,
live-probe provenance, the generated card written from
the live surface.

Stdio targets are untouched: a stdio server is a local
process in the operator's own environment, and the
flag has no effect there.

Tests ride the existing auth fixture, which gains a
mode that answers 401 without the token and serves the
discover and list walk with it. The pins: a credentialed
check on a protected surface is a live pass; a
credentialed generate records the tools; a refused
token degrades to declared mode with the refusal
reported; and no token changes nothing, which the
existing protected-surface tests already pin.

### The credential-aware probe, as built (2026-11-24)

The snapshot records the run's credential as a
CredentialOutcome: NotPresented, Accepted, or
Refused(step). It is a fact of the run beside the
auth posture, which stays the surface's: a credentialed
snapshot's auth is still Protected with the metadata
the first challenge's ladder read, so the baseline
records the posture and authentication-changed keeps
comparing it across runs with and without a token.

The HTTP probe walks unauthenticated first, exactly as
before; a challenge hands back the step and the
declared snapshot, and only then, with a token in hand,
retries the whole walk once with the Authorization
header on the endpoint requests. The header value is
built by one helper and set only on the endpoint's own
requests, the rpc posts and the session DELETE; the
metadata ladder and the card fetch go through the
plain document fetch and never see it. A
walked retry returns the live snapshot with its auth
replaced by the declared posture and its credential
marked Accepted. A refused retry returns the declared
snapshot with the refusal step recorded. A failure in
the credentialed walk that is not a 401 is a probe
failure as usual; only the challenge degrades.

The drift dispatch reads the outcome: a Protected
snapshot with an accepted credential runs the live
surface comparison (the auth comparison above it runs
either way), and any other Protected snapshot runs
the card comparison as before. Generate and migrate
take the card path only when the credential was not
accepted, so a credentialed run records the live
baseline with live-probe provenance. Check labels a
credentialed run live, and a refused run declared,
with stderr naming the refusal and its step.

The CLI takes --token-env naming the variable to
read, defaulting to ALMANAC_TOKEN; an unset or empty
variable is no credential. The fixture gained an
auth-token mode serving the walk only to the token
bearer, and the pins are an end to end test (generate
records the seven walked tools with live-probe
provenance, check is live with the token, declared
without it, declared with the refusal named on a
wrong one) plus a core test pinning the dispatch on
the credential outcome alone.

### Credential-aware probe checkpoint fix pass (2026-11-24)

The red team and the spec review each found the same
two Majors, and both sat on the slice's headline
claims. First, the core dispatch test asserted a
finding kind nothing emits (tool-removed for the
live comparison's announced-tool-missing), so the
pin was void in both directions; it now asserts the
real kind. Second, the live identity comparison
compared protocol versions ungated, and a baseline
recorded from a Final card holds the card's first
declared version, not a negotiation, so a
credentialed check against such a baseline would
have reported identity drift that never happened.
Protocol now compares only between two live records,
which the entry's provenance distinguishes, matching
the card comparison's longstanding omission of the
field. Further closures: migrate names a refused
credential in its report, as generate and check
already did; a credentialed walk that fails partway
no longer earns the Accepted mark, since its lists
are partial; the token value is trimmed against the
secret store's trailing newline; a refusal at the
initialized notification names that step rather than
the ladder's; the walked-live predicate is stated
once on the snapshot instead of re-derived at five
sites; the Authorization value is built by one
helper shared by the rpc posts and the session
DELETE; and the tokenless run's stderr assertion
now also proves no refusal was reported. The
standards axis's data clump note (the request
context threading four layers) is answered in the
hosted gate fix pass below.

### Credential-aware probe hosted gate fix pass (2026-11-24)

Two hosted rounds, both in the lint layer, no
product behavior changed. The first was an unused
import: the walked-live predicate moved the
credential comparison into the snapshot, so drift's
library code no longer names the credential type
and its crate level import moved into the tests
module that names it. The second was the argument
lint: threading the credential pushed the request
helper and the list walk past seven arguments. The
standards axis had flagged the same clump as a
judgment call; the lint settled it, and the four
riders every probe request carries (the endpoint
URL, the session, the protocol version, the token)
are now one Conversation value threaded through the
entry walk, the ladder, the list walk, and each
request, which is the bundling the review named as
the natural fix.

## Generation roadmap, fifth slice: the full generation report (2026-11-24)

The roadmap's last generation item. What the record
has called the generation report so far is the run's
stderr notes, in the metadata slice's own words, a
partial thing: the notes were collected, but the
warnings printed at the point of occurrence, the
suppressions rode inside note strings, and the run
named neither the entries it wrote nor the files it
touched. Check prints a structured report and
migrate prints a structured report; generate printed
one stdout line and scattered the rest across
stderr. The full report completes the pattern.

Generate now ends a successful run by printing one
JSON report on stdout, in the same outer shape as
check's (the mode first, then the fields). It
carries the host display name and the domain; the
entries written, each with its identifier, its media
type, and the provenance the baseline records for it
(produced_by); the files written, as site relative
paths in write order (the server card copies, a
skill file copy, the catalog pair, any document
copies, the baseline); the files a suppression
removed; the suppressed entries as identifier and
reason pairs; and the run's notes and warnings as
two lists. Every note and warning the
run produces is collected into the report, including
the ones that used to print at the point of
occurrence (the protected card keeping its own
words, the off origin endpoint's well known only
card, the unmatched config blocks, the document
description fallbacks), and a successful run prints
nothing to stderr. The suppression note strings are
retired; the structured pairs carry the same facts.
The main function's own written-under line is
retired with them; the report is the run's output.

Two boundaries. A refused run prints no report, as
before: the refusal is the error, on stderr, exit 2.
And the report is a run record, not a site artifact:
it is printed, never written into the site, because
the baseline is the committed record of a generation
and a second record beside it would be a second
source of truth. Tests that asserted on generate's
stderr now read the report's fields, which is the
sharper assertion in each case: a suppressed entry's
reason is a field, not a substring of prose.

As built: the card disagreement helper returns its
warnings instead of printing them, so generate
collects them and migrate, its other caller, prints
them as it always did. The note strings lose their
presentation prefix in the report, since the field
is the framing now. A new end to end test pins the
report's shape on a run with a skill: the mode, the
domain, both entries with their provenance, the five
files, the empty suppression list, the legacy path
note, and an empty stderr.

### Generation report checkpoint fix pass (2026-11-24)

All three reviews found the same Major, and it was
the slice's headline claim: the config validator's
two warnings (a query count outside two to five,
duplicate values in a block) still printed to stderr
and never reached the report, because the validator
had no channel to it. The validator now returns its
warnings and generate seeds the report's list with
them, pinned by a test whose run carries such a
config. Further closures: the report also records
the files a suppression removed (a suppressed
server's card copies, a suppressed skill's stale
served copy), each recorded only when the removal
actually happened; the written paths are derived
from the write sites' own path values rather than
repeated as literals; the report test asserts the
host and the exact files list in write order; a
missing provenance record would read "unrecorded"
rather than an empty string; and the report's print
follows the sibling reports' serialization idiom.

## Generation roadmap, sixth slice: REST-only generation (2026-11-24)

The OpenAPI slice recorded the named question: generate
required a surface target, so an OpenAPI document could
only compose with an MCP surface the way a skill file
does, and a REST only site, with no MCP surface at all,
could not be generated for. This slice lifts it.
Generate's target becomes optional. With neither --url
nor --command, the run probes nothing: no MCP entry, no
server card, and the baseline records no MCP surface.
The documents, agent cards, and skill a run names are
announced exactly as before, under the same editorial
precedence, and the generation report describes the run
the same way.

One refusal stands in place of the target requirement:
a run that names no target and no inputs at all refuses,
because a catalog with nothing behind it is a mistake
rather than a publication. The refusal covers naming
nothing, not announcing nothing: a run that names
inputs and suppresses every one publishes an empty
catalog with each absence reasoned in the report, as a
targeted run suppressing its only entry already did.
The refusal comes after config validation, so a broken
config still refuses first, as it does today. Check
needs no change: a baseline with no MCP surface is a
shape migrate already produces for a catalog that
announced none, and check reads it today.

As built: the MCP part of generate sits in one
conditional region and the run's collections are
declared before it, so a REST only run and a targeted
run share every line after it. The local gate over
this slice and the generation report slice together
ran green firsthand (clippy clean, 107 tests), in a
window the write fault opened; the two new tests pin
the REST only catalog's shape and the refusal.

### REST-only checkpoint fix pass (2026-11-24)

The three reviews landed with no Critical and no
Major findings. Closures: the target helper's doc
comment no longer says generate requires a target;
generate's doc comment and the usage text state the
nothing-named refusal; the --queries flag in a REST
only run warns that it had no effect, since it names
the MCP entry's queries and the run has none (its
silence was newly reachable with this slice); the
refusal's record is narrowed to what the code
enforces (naming nothing, not announcing nothing);
and two test pins were added, a skill only REST run
and the unmatched MCP block warning in a REST run.

## Generation roadmap, seventh slice: YAML OpenAPI documents (design, 2026-11-24)

The OpenAPI slice recorded its known limit: JSON
documents only, though YAML is the format half the
world's OpenAPI files are written in, and a YAML
document failed generation with its reason named.
This slice removes the limit. One parse helper reads
a document as JSON first and then as YAML, converting
YAML to the same JSON value the reduction consumes,
so every downstream fact (operations, contract
hashes, the baseline, the drift classes) is format
blind. A document that parses as neither refuses with
both reasons named. Detection is by content, not by
file name: a YAML document named .json and a JSON
document named .yaml both read as what they are.

Three boundaries. The comparison story is unchanged
from the first slice: the gate compares an OpenAPI
document operation by operation, never byte by byte,
and that is the whole promise for these documents.
The entry digest and the baseline record the
document's bytes as served at generation, in either
format, but no check path compares them, so replacing
a YAML document with its JSON twin (or any byte
change the reduction does not cover) passes check
silently, exactly as a reduction preserving edit to a
JSON document always has. The entry's media type
stays application/openapi+json: the catalog announces
one API entry kind for one surface kind, and the
format is a parse detail. And A2A agent cards stay
JSON only: the agent card is a JSON document by its
own specification, with no YAML form in circulation.
The parse dependency is serde_yaml, in the CLI crate
beside the other format readers, because YAML's
anchors, aliases and scalar rules make a hand rolled
parser a quiet source of misread documents, the same
reasoning that put a real TOML parser in the tree;
the core reduction stays untouched.

As built: the one helper, parse_api_document, serves
all three OpenAPI parse points (generate's planning,
check's surface read, migrate's typed arm), and each
site phrases the shared neither-format reason in its
own sentence. The identifier stem drops whichever
document suffix the file carries (.json, .yaml,
.yml), so a document's identifier is format blind
too: billing.json and billing.yaml are one API,
announced once. A test generates from a YAML billing
document (carrying an unquoted numeric response key
and an anchored, aliased parameter, the two shapes
real YAML documents have) and reads an operation
removal back through the YAML parse at Major.

### YAML checkpoint fix pass (2026-11-24)

The three reviews landed clean on the plumbing (no
Critical on any axis; the helper, its three routings
and the A2A boundary verified) and found the record
and two bounds wanting. The spec axis and the red
team independently found the Major: the slice's
digest boundary sentence claimed a gate report on a
format swap that no check path delivers, because
OpenAPI documents are deliberately not artifacts and
their recorded hash is compared nowhere. The code is
continuous with the first slice's recorded design, so
the sentence is corrected above rather than the
comparison wired. Also closed: the identifier stem
now drops the YAML suffixes as well as .json (the
spec axis caught the slice's own test asserting the
format blind identifier the code did not yet
produce); the helper caps the bytes at the probe's
16 MiB and refuses a file holding more than one YAML
document instead of reading its first document alone
(the red team's truncation and expansion findings);
the stale "not JSON" narration at check's read site
is corrected. Recorded limits, accepted plainly:
serde_yaml reads YAML 1.2 core scalars, so the YAML
1.1 boolean spellings (yes, no, on, off) arrive as
strings, and a parameter marked required: yes records
as optional until the document is corrected to true,
which the gate then reports as a required parameter
added; unquoted numeric versions (openapi: 3.1,
version: 1.0) refuse as unreadable fields rather than
versions, so YAML authors quote version strings; and
deeply nested YAML can still exhaust the reader
inside the byte cap, a robustness limit that must be
settled before remote artifact fetching makes these
bytes network supplied.

## Generation roadmap, eighth slice: remote artifact fetching (design, 2026-11-24)

Check today reads only what the site directory holds.
An entry URL on the publisher's own origin maps back
to a site file; anything else is outside the gate's
reach, in two inconsistent ways. A baselined OpenAPI
document or agent card whose URL is off origin is
reported broken with "its URL does not map into the
site" and fails at Critical, while a baselined
artifact (a server card copy above all) whose URL is
off origin is silently never digest checked at all:
the digest list simply omits it and the core loop
skips what it is not given. A publisher who hosts a
document on a docs domain or a card on a CDN gets a
false failure for the document and no verification
for the artifact.

The slice teaches check to fetch what it cannot map.
The fetch set is the baselined things check already
reads when they are local: artifacts, OpenAPI
documents, and A2A agent cards. Skills stay file
truth: a skill is a file in the site by construction,
and the directory walk that finds unannounced skills
has no remote form. Fetching happens in a live run,
which is a run that already does network work: a
target is named, or the operator passes
--fetch-remote for a targetless run (a REST only
site's whole surface can be remote). Repo mode
without either stays offline, as the design promises
per pull request runs, and keeps today's behavior.
(The checkpoint fix pass below narrows the trigger:
the target that counts is an HTTP one, named or
resolved from the catalog; a stdio target is local
work and does not fetch.)

The fetcher is one helper in the CLI beside the
probe's own reads: blocking, no credential ever (the
publisher token is scoped to the MCP endpoint by the
credential slice's rule and never rides to an artifact
URL; announced artifacts are public documents), the
probe's 30 second timeout and 16 MiB cap, and up to
three redirects followed, as the metadata fetches do.
A fetch that fails (unreachable, a non success status,
over the cap) takes the channel the local read already
uses: a document or card goes to its broken list with
the fetch's reason and fails at Critical unreadable;
an artifact's digest reads as missing where its entry
points, the existing Critical. Fetched bytes flow into
exactly the values local bytes flow into today (the
digest list, the document views), so the core and
every drift class stay unchanged and network free.

One debt comes due in this slice, recorded at the
YAML fix pass: fetched YAML is network supplied, so
the reader's nesting limit must be settled, not just
capped. The build measures the real failure depth
first; if deep nesting aborts the reader, the parse
helper gains a depth guard at the JSON reader's own
128, counted over flow brackets and block indentation,
refusing deeper documents with the reason named.

As built: check takes a fetch_remote flag beside the
target; a run fetches when it has an HTTP target
(named or resolved from the catalog) or the flag is
passed. One helper (announced_bytes) serves
both document readers with the caller's missing words,
and the artifact digest loop fetches what it cannot
map. The fetcher (fetch_document in the probe module)
follows three redirects by hand, caps at 16 MiB as a
refusal, names every failure, and carries no
credential. Fetched bytes enter exactly the values
local bytes enter, so the core is untouched. A test
repoints a generated site at a local HTTP server and
walks the whole story: offline refusal naming the
unmapped URL, a clean fetching run, an operation
removed remotely at Major, a card tampered remotely
breaking its digest. The nesting debt is settled by
measurement, not by a guard: against the built binary,
a 100,000 deep flow document and a 3,000 deep block
document both refuse cleanly at the YAML reader's own
recursion limit (the error names it, at the same depth
as the JSON reader's), and a nine level alias bomb
refuses at its repetition limit. serde_yaml 0.9.34
carries both guards itself, so the parse helper needs
none of its own; the 16 MiB cap stands as the outer
bound.

### Remote fetching checkpoint fix pass (2026-11-24)

The three reviews landed with no Critical on any
axis. The red team's Major was the fetch target set:
baseline URLs were fetched with no restriction, and a
crafted baseline in a contributor's pull request plus
a fetching run against the unreviewed tree could make
CI read internal addresses, with the report's named
reasons and any reduction leaking what answered. The
fetcher now refuses, before any request and again at
each redirect target, a URL naming a literal
non-public address (link local, where the cloud
metadata family lives; unique local; unspecified).
Names are deliberately not judged: a name resolving
privately is recorded as the accepted residual,
bounded by the posture that fetching is a live run's
opt-in behavior against the publisher's own committed
baseline, response bodies are never printed, and live
runs belong on reviewed trees in the design's own
mode split. Also closed: a stdio target no longer
turns fetching on (a local child process is not
network work; the condition is an HTTP target, named
or resolved, or the flag), which also corrects the
record's "a target is named" to the code's actual
rule; the artifact loop routes through announced_bytes
and names a failed fetch on stderr, since the digest
channel carries no reason and a remote outage must
not read as a deletion with nothing to act on; the
fetcher is one DocumentFetcher built once per run,
matching the module's build-once convention; check's
doc comment, displaced onto the helper at insertion,
is restored; USAGE names the full fetch set; and the
test gains a thread-per-connection server, the
artifact-count assumption stated, a run where an HTTP
target alone turns fetching on, and a dead-host case
pinning the named fetch failure. Recorded and
accepted: a run's fetch time is bounded per document
(30 seconds, three redirects) but not in aggregate,
so a baseline with many slow remote records can hold
a scheduled run; and the A2A fetch arm shares the one
helper with the OpenAPI arm rather than carrying its
own test, the two call sites being the same code
path.

### Remote fetching hosted gate fix pass (2026-11-24)

The hosted gate's one failure was the slice test's
own HTTP target run: it checked a site generated from
the stdio fixture against the HTTP fixture, whose
tool input schemas differ in bytes from the stdio
fixture's, so the live comparison legitimately
reported schema drift at Major. The product was
right and the premise was wrong. The run now lives
in a dedicated test that generates its site from the
HTTP fixture itself, so the targeted run compares
against the surface its baseline recorded. No
product code changed.

### Remote fetching hosted gate closure (2026-11-24)

Four hosted rounds failed on this slice, every one
of them test side; the product substance was never
the problem in any round. The first was the slice
test's own premise (a stdio generated site checked
against the HTTP fixture, whose schema bytes differ);
the second was a format slip at the fix's insertion
seam; the third and fourth were the dedicated HTTP
target test written to replace the first round's run,
whose hand written calls kept assuming contracts the
helpers around them supply. Under the stop condition
stated before the fourth round, the targeted-run
coverage is pulled from this slice: the slice lands
on the flag-trigger test, which passes. The target
trigger itself is product code the reviews verified
and the flag test's sibling paths exercise; proving
it end to end is owed as its own slice, with a local
gate behind the test before it stages.

## Ninth slice: display changes in a declared artifact (design, 2026-11-24)

The last items on the recorded post-v1 list are the
two Minor drift classes deferred at the milestone one
checkpoint. The first, in the design's own words
(section 6, Minor): display text, tags or ordering
changes in a declared artifact. The deferral's gloss:
the first needs field level card comparison beyond
the digest. Today the server card is bound by digest
alone, so a publisher who fixes a typo in the card's
description, swaps an icon, or reorders its protocol
versions fails the gate at Critical, the same as a
publisher whose card was replaced wholesale. The
digest cannot tell an editorial edit from a
substantive one; fields can.

The slice teaches check to tell them apart, for the
server card specifically (the auth metadata documents
are declared artifacts too, but their comparison is
already field level for the fields that carry
meaning, and their display fields stay unrecorded;
that boundary is stated here rather than discovered
later). The baseline's MCP record gains the card's
display record: its title and description, its icons
and website in order, its remote URLs in order, and
a remainder hash, the card with those display fields
removed, canonically hashed. A baseline written
before this slice records none of it and keeps
today's behavior exactly, the digest alone.

At check, when a card artifact's served bytes miss
the recorded digest, the served card is reduced the
same way. If its remainder hash matches the recorded
one, nothing substantive moved: the byte change is
wholly display, so the run reports Minor
declared-display-changed naming the fields that
differ, and the digest Critical does not fire for
that artifact. If the remainder differs, or the
served card will not parse, or no display record
exists, the digest Critical stands as today. Both
card copies are judged independently, as the digest
loop already judges them. The reduction lives where
the card reduction already lives, the CLI side; the
core receives the display record and the served
reduction as plain values and stays network free,
and the comparison joins the existing artifact pass.

As built: the core gains a CardDisplay record
(title, description, icons, website, remote URLs in
order, remainder hash) on the MCP baseline, absent
in baselines written before the slice. Generate and
migrate reduce it from the card bytes in hand at the
baseline's construction. The CLI's reduction removes
the five display keys (title, displayName,
description, icons, websiteUrl), sorts the remotes
by canonical form so their order alone cannot move
the remainder, and hashes what is left in a canonical
JSON form (object keys sorted, arrays in order).
Check reduces every served card artifact the same
way and hands the reductions to the core beside the
digests; the artifact pass reports Minor
declared-display-changed, naming the fields that
differ, in place of the digest Criticals whenever
the remainder matches and at least one recorded
display field moved, and keeps the Criticals
otherwise. A core test walks the postures, and an integration
test edits a generated card's description (passes,
one Minor, no digest finding) and then its version
(fails at Critical).

### Display changes checkpoint fix pass (2026-11-24)

The three reviews landed with no Critical on any
axis. The standards Major was a duplicated canonical
form: the CLI hand rolled a canonical JSON serializer
for the remainder hash when the core hash module's
canonical bytes are the standing definition every
baseline hash uses. The hand rolled function is
deleted; the remainder is hashed with the core's
sha256_json, and the sort keys ride serde's own text
form. The red team's endpoint point narrowed the
class honestly: a remote reorder that moves the
primary endpoint is substance, not display, because
the card reading resolves the first streamable-http
remote. The display record now carries the resolved
primary endpoint, and an endpoint change defeats the
display judgment outright, so remote order is
display only while the endpoint it names is
unchanged. Three record sentences the reviews found
missing are stated here: the card carries no tags
field in either shape the tool reads or writes, so
the design's word tags has nothing to attach to
inside a declared artifact (catalog entry tags are a
different surface and stay unrecorded); display keys
are read at the top level only, so display text
nested inside another object rides in the remainder;
and a remote's protocol versions past the first are
a preference list normalized like the remote order,
while the first names the version the card reading
takes and stays substantive. Also closed: the
changed list names the card's fields in the card's
own words; the core
test's Critical postures pin their finding counts;
a parse pin covers a pre-slice baseline whose MCP
record lacks the new field; the two recording sites
share one helper; and the display key list carries a
comment naming the extraction reads as its other
half.

### Display changes hosted gate fix pass (2026-11-24)

The hosted gate caught a collision all three
reviews missed: the milestone one test
skill_digest_and_path_breaks plants its digest break
by appending one newline to the served card, and
the slice as staged read that as a serialization
only change and passed it at Minor. The standing
test is right and the slice's reach was wrong: the
class covers display FIELD changes, and a byte
change no recorded field explains (a trailing
newline, a key reordering in the file) leaves the
parsed card identical, with nothing a reader sees
changed to excuse the bytes. The judgment now
requires at least one recorded display field to
differ before the Minor replaces the Criticals; a
core posture pins the identical-reduction case at
Critical. The record's as built wording is
corrected to match.

## Tenth slice: the served media type (design, 2026-11-24)

The last deferred Minor class, in the design's own
words (section 6, Minor): the catalog served with a
generic JSON media type rather than
application/ard+json. The deferral's gloss: the
second needs the checker to observe a served
catalog, which local mode never does. The fetching
slice gave check a live posture and a fetcher; this
slice spends them on the one observation the class
needs.

In a fetching run (an HTTP target, named or
resolved, or --fetch-remote), check also requests
the catalog itself, at the well-known address on the
publisher's own origin, and reads the Content-Type
of the answer. When the observed media type is
anything but application/ard+json (parameters such
as a charset are the same type wearing a parameter),
the run reports Minor catalog-media-type naming what
was served. The observation is best effort by
design: a run that cannot observe (no fetching, the
request fails, the answer is not a success, the
address names a non-public literal, which the
fetching slice's refusal already covers) reports
nothing for this class, because an unobserved
serving is not evidence about the serving. The
catalog's bytes are not compared here; the site
files remain the content the gate judges, and a
served-versus-filed divergence class is not in the
record. The request tries the https address the
catalog's own URLs use, and falls back to http when
the request itself cannot be made, the two schemes
the checker's own origin mapping has always
accepted. The core receives the observed type as a
plain value and stays network free, as every slice
before this one kept it.

As built: the fetcher gains fetch_media_type, with
four outcomes kept apart: an observation (a success
answer, its Content-Type the value, an absent
header observing as empty), an answer that
observed nothing (a non-success, a redirect chain
that went nowhere usable, a Content-Type in
unreadable bytes), a request that could not be
made at all, and a refusal (an address, or a
redirect target, naming a non-public literal).
Check tries the https address first and falls back
to http only when the request could not be made;
a refusal is final, and a non-success https answer
ends the observation without trying http, an
asymmetry the fallback rule's own words carry (a
request that was answered was made). The core
receives the observed type as a plain value and
reports Minor catalog-media-type when the type
before any parameter, compared case-insensitively,
is not application/ard+json, naming what was
served. Success means status 200, the fetcher's
standing convention. A legacy only site is not
observed, because its catalog does not live at the
ard.json address and its path already reports at
Major; a domain carrying characters that would
change the address's shape is not observed either.
The observation sends no Accept header by design:
the class measures the default serving at the
well-known address, which is the serving the
specification names. Tests: a core test walks the
postures (a generic type warns, the catalog's own
type with a charset parameter is clean, an
uppercase spelling is clean, an empty observation
warns as no media type, no observation is silent),
and an integration test serves a catalog under a
changeable media type at the site domain's own
authority, where a fetching run warns on
application/json and is silent under
application/ard+json, and a repo mode run observes
nothing. With this slice the last deferred Minor
class from the milestone one checkpoint is built,
and the deferred list is empty.

### Media type checkpoint fix pass (2026-11-24)

The reviews caught the slice's worst fault before
any gate did: the observation address interpolated
a domain binding check does not have (the domain
is the published catalog's host domain), so the
tree did not compile. Also closed: the shared
non-public guard now judges an IPv4-mapped IPv6
literal as the IPv4 address it names, closing a
way to the metadata family that predates this
slice and that the catalog-supplied domain made
newly reachable; the fetcher's outcome is the
four-way enum above rather than a two-way result
with refusals folded into failures, so the scheme
fallback fires only where the design puts it; the
class is factored beside its sibling classes in
the core; the case-insensitive and empty
observation postures gained tests; and the record
now states the success reading, the legacy only
skip, the domain shape guard, the fallback
asymmetry, and the no-Accept posture. The redirect
walk's duplication beside fetch is recorded as
accumulated duplication for a future factoring,
not closed here.

## Eleventh slice: the target trigger, re-earned (2026-11-24)

The fetching slice's closure note owes this slice:
the HTTP target trigger (a check run with an HTTP
target fetches remote artifacts without the flag)
proved end to end as its own slice, with a local
gate behind the test before it stages. The slice is
one integration test and no product code; the
trigger itself is the fetching slice's product,
reviewed there. The test generates its site from
the HTTP fixture itself, so the live comparison
runs against the surface its baseline recorded;
every contract it touches was read from the source
this time (generate's required domain flag in
main.rs, the fixture's default mode, the fetching
derivation in check) rather than taken from a
sibling test's shape, which is the habit the four
hosted rounds indicted. It is recorded here after
the tenth slice's design section because that
design was written first; the build order follows
the closure note, this slice first.

As built, the local gate earned its place before
any hosted round was spent: its first full run
failed the test on a fifth assumption, that a run
naming no target stays in repo mode. On a site
generated over HTTP the catalog resolves its own
target from the card, so the plain run probed the
fixture live and fetched on its own. The test now
removes the served card file after generation, so
resolution finds no target and the plain run stays
in repo mode for a reason the test itself states;
the named run is then the only difference. With
that shape the full local gate passes over the
slice (format, clippy, and the whole suite), and
the slice stages on that evidence.

## Twelfth slice: local $ref parameters resolved (design, 2026-11-25)

The OpenAPI reduction's named limit, recorded when
the slice landed, is that parameters and schemas
referenced by $ref are not resolved. For schemas
the limit stands: the components hash reports the
shared pool changed without attributing it, and
attribution without resolution would be invented.
For parameters the limit costs the gate a real
class: shared parameters live under components in
real documents and operations reference them, so a
required parameter carried by reference never
enters the operation's required list, and its
arrival or departure is invisible.

The refusal's reasoning was that resolving
references means following pointers into other
documents, which is a fetch the core never does.
That is true of external references and stays
refused. A local reference, a fragment pointer
into the same document (#/components/parameters/
Name and any other local pointer), resolves
against the document already in hand. This slice
resolves local $ref parameters during reduction:
the pointed object's name, location, and required
flag count exactly as an inline parameter's do.
Reference chains are followed with a visited set,
so a cycle leaves the parameter invisible rather
than spinning; a chain of distinct references
resolves however long it runs, and a finite
document holds finitely many reference strings,
so the walk always ends. A
reference pointing outside the document, or at a
place the document does not hold, leaves the
parameter invisible, as today.

One consequence is named plainly: a baseline
recorded before this slice holds required lists
without referenced parameters, so the first check
of such a site reports the newly visible
parameters as required parameters added, at Major,
until the publisher regenerates and records the
fuller baseline. That is the honest price of the
checker seeing more of the same document, and it
is a one-time re-baseline, not a standing noise.

As built: resolve_parameter in the core reduction
follows a parameter's $ref through
resolve_local_pointer, which percent-decodes the
fragment as RFC 6901 section 6 prescribes, walks
object keys with the standard ~ escapes, and
crosses arrays by index; the visited set bounds
the walk, and resolutions are cached by reference
for one reduction, so a chain shared by many
parameters is walked once. The drift classes do
not change: a referenced parameter enters the
same required list the baseline records and the
check compares, at generate and at check alike,
because both run the one reduction. A core test
pins the postures in one document, each supplied
by exactly the route it names: a direct reference,
an alias chain, a path level reference, a
percent-encoded pointer, an array-crossing
pointer, an optional reference staying out, and
cyclic, missing, external, non-object, and
sibling-carrying references staying invisible.

### $ref parameters checkpoint fix pass (2026-11-25)

The standards review caught the first test draft
arranging two of its postures so they could not
fail: the alias pointed at a parameter the direct
reference already supplied, and the external
reference's fragment named the same parameter, so
deleting chain following, or resolving external
fragments locally, left the expected list
unchanged. The test was rebuilt so every posture
is supplied by exactly its own route, and it grew
the red team's unpinned postures (a two-node
cycle, a path level reference, a pointer landing
on a non-object, a non-string $ref read as an
ordinary parameter, and a reference whose siblings
are ignored, as OpenAPI's rule requires). The
reviews' remaining points closed in code rather
than in the record: the pointer walker now
percent-decodes its fragment and crosses arrays
by index, so the design's "any other local
pointer" is true as written, and resolutions are
cached per reduction, closing the quadratic
re-walk of shared chains the red team sized. The
design section's chain sentence was corrected to
the mechanism the code implements. The first
compile of the repaired tree then caught two real
errors in the repair itself, which the gate
exists to catch: the call site passed the
reduction's shadowed object view where the
resolver takes the whole document (the reduction
now keeps the document beside the view as root),
and the cached walk carried a dead first
assignment (the walk now binds each hop
directly). Both are fixed in the slice as staged,
and the hosted gate carries the verdict on the
staged tree.

## Thirteenth slice: one redirect walk (design, 2026-11-25)

Two reviews banked the same debt: the media type
slice's redirect walk was the third copy of one hop
loop (the metadata walk in get_bytes, the fetcher's
fetch, and the observation walk), recorded as
accumulated duplication for a future factoring.
This slice pays it. One follow function performs
the hop machinery for all three: request, follow a
redirect by hand, at most three, and refuse a
literal non-public address before every request on
a guarded walk.

The factoring unifies the machinery and nothing
else. The guard stays a per-walk choice, because
the postures differ on the record: the fetcher's
walks are guarded (a fetching run must not become
a way to make CI read a metadata endpoint), and
the metadata walk is not (it reads the surface's
own OAuth documents, where a privately resolving
name is the operator's infrastructure question).
Each consumer keeps its failure vocabulary exactly:
fetch's named reasons ride inside the walk's
outcomes, the media observation maps them onto its
four cases one for one, and get_bytes still reads
every non-success as absence. Terminal handling
stays with the consumers: fetch enforces its size
cap as a refusal on the returned response, the
metadata walk truncates silently as it always has,
and the observation reads only the Content-Type.
Behavior is unchanged, and the existing suite,
which pins all three walks end to end, is the
proof.

As built: follow and its FollowOutcome enum sit
above get_bytes in probe_http.rs, and the file now
holds exactly one hop loop where it held three.
get_bytes rides it unguarded, fetch and
fetch_media_type guarded, and each consumer's
terminal handling and failure strings are
character for character what they were. The
standards review's two Minors closed in the fix
pass: the shared walk reports dead ends as neutral
kinds (no location, unresolvable, too many
redirects) and fetch words them, so no consumer's
prose lives in the shared machinery, and the
outcome type takes the file's Outcome naming
beside RpcOutcome and MediaTypeOutcome.

## Trust manifest verification (design, 2026-11-25)

Main charged this item on 2026-11-25: research
first, the signature scheme and canonical form
pinned against the spec, a design record for
approval before any code. Amendment D still bars
signing and key management; this is the read side
only, verifying what publishers sign.

The spec, pinned firsthand. The normative text is
the IETF draft (draft-bu-dawn-ard-discovery-00,
2026-10-30), sections 3.5, 3.5.1, 3.5.2, the CDDL
in section 5, and section 6.2; the v0.91 site text
is looser (it delegates verification to the
manifest's declared trust framework) and the draft
is the pin where they differ. A trustManifest
carries identity (MUST, a signer identifier such
as did:web:example.com), subject (MUST in the
field list, a self-contained block holding the
entry's identifier, type, digest, and version when
present), issuedAt and expiresAt, optional
attestations and provenance, and signature (MUST
in the field list). The signature is a detached
JWS (RFC 7515) computed over the RFC 8785 JCS
serialization of the trustManifest object with
the signature member removed; the protected
header names alg (ES256 mandatory to implement)
and kid. The CDDL is more lenient than the field
list (subject and signature are optional there);
the design follows the field list and names the
discrepancy rather than resolving it silently.
Verification under the publisher profile
(section 3.5.1) is three checks: the subject
matches the parent entry, the identity binds to
the publisher domain in the entry's URN, and the
signature is valid over the canonicalized
manifest. Section 6.2 adds that an entry lacking
a verified signature bound to the publisher is an
unverified discovery hint, and a signed entry
whose proof or binding fails must be rejected.
Third-party manifests ride in a trustManifests
array under the signer profile: same subject,
their own identity domain, and they never
substitute for the publisher's manifest.

The build, in layers, each independently
reportable in validate, beside the shape and
expiry classes version 1 already has:

1. Draft shape. identity present as a string,
   subject present as an object carrying
   identifier and type, signature present as a
   string. Absences report at Major under the
   existing trust-manifest-shape kind, extended.
2. Subject binding, offline, needing no key:
   subject identifier, type, digest, and version
   against the parent entry's own fields, each
   compared where both sides carry it, and a
   subject digest absent while the entry carries
   one is itself a mismatch, because the digest is
   the binding section 3.5.2 exists to seal. A
   mismatch is Critical, kind
   trust-manifest-subject-mismatch: the draft's
   reject case, in the gate's vocabulary.
3. Identity binding, offline: under the
   publisher profile the identity must bind to
   the entry's publisher domain: a did:web
   identity naming that domain, an https identity
   on it, or a SPIFFE id whose trust domain is
   it. Anything else is Critical, kind
   trust-manifest-identity-unbound. Third-party
   manifests are exempt from this layer; their
   identity binds at key resolution instead.
4. Signature, needing a key: JCS-canonicalize
   the manifest with the signature member
   removed, split the detached JWS (three
   segments, the payload segment empty), read alg
   and kid from the protected header, resolve the
   key, and verify ES256 over the signing input.
   An invalid signature is Critical, kind
   trust-manifest-signature-invalid. A signature
   that cannot be attempted reports Minor, kind
   trust-manifest-signature-unverified, naming
   why (no key source, an algorithm this build
   does not verify, an identity form it does not
   resolve): the entry is never silently passed,
   because the draft's own words for it are an
   unverified hint, and the report says so.

Keys come from two sources, in this precedence:
a JWKS or JWK file named by a --trust-keys flag
(the offline CI shape, deterministic), matched
by kid; and, in a fetching run only, did:web
resolution for a did:web identity, fetching
https://<domain>/.well-known/did.json through
the existing guarded fetcher and reading its
verificationMethod publicKeyJwk entries, matched
by the kid's DID URL. No other identity form is
resolved in this build.

Two pieces of machinery are new. JCS
canonicalization lands in the core as its own
module over serde_json values (object keys
sorted by UTF-16 code units, the number
serialization RFC 8785 defers to ECMAScript,
the minimal string escapes), proven against the
RFC's own test vectors and the draft's example
manifest before any signature is judged with it;
number serialization is the known hard edge and
the vectors are its gate. ES256 verification
needs P-256: ring is already in the lock through
rustls and verifies the JWS raw R||S form
directly, so the proposal names ring as a direct
dependency with this paragraph as the written
reason DESIGN requires, and no new dependency
family enters the tree.

One modeling fix rides inside the scope: the
catalog model carries only the singular
trustManifest today, so a trustManifests array
is dropped on parse and lost on regeneration.
The model learns to carry it, because a signed
envelope the tool silently drops is a fact the
gate exists to catch, and third-party manifests
cannot be verified if they cannot be read.

Scope lines, stated plainly. Validate carries
the verification in this build; check does not
change (its drift model is baseline relative,
and signature validity is not), with check
integration named as a follow-up question, not
smuggled in. Attestation documents are not
fetched and their digests not re-verified
(section 6.4's pinning is a follow-up), the
cross-origin url binding of section 6.2 is a
follow-up, and signing stays barred under
Amendment D. Open questions for Main's approval:
the validate-only scope, ES256 as the one
verified algorithm in this build, the two key
sources and their precedence, and the
trustManifests modeling fix riding along.

### As built (2026-11-25)

The build follows the record, with the refinements and
rulings the checkpoint reviews produced, all recorded
here. The model carries both envelope forms: Entry
gains trust_manifests beside the singular
trust_manifest, with the serde posture of its sibling
fields, and the keyed pass expiry judges the plural
manifests the plain pass never read. The envelope
layers engage only when a manifest carries identity,
subject, or signature; a manifest carrying none of
them is the pre-draft expiry-only form and keeps the
version 1 judgment exactly, so the standing expiry
tests stand untouched and the plain pass, which check
runs, is behaviorally identical to before. The keyed
pass is a separate entry point, validate_catalog_with_keys,
and validate is its only caller.

Subject binding follows the record's comparison rule
as the spec review pinned it: identifier and type
absences are layer 1 shape findings at major, present
values that differ are layer 2 mismatches at critical,
digest and version compare where both sides carry
them, and a subject digest absent while the entry
carries one is itself a mismatch. Identity binds to
the publisher segment of the entry's own URN, the
draft's section 3.5.1 subject, with the catalog host
domain standing in only when the URN does not parse.
Keys from a resolved DID document bind to that
document's own subject at read time: a verification
method counts only when its id's DID part is the
document's id and its controller, when named, is the
subject too, and a relative method id reads as the
document's id with that fragment. A fetched document
therefore cannot register a key under another
identity's kid, the red team's one major finding,
closed at the reader rather than by ordering the
pooled list. The trust flags belong to validate
alone; generate, check, and migrate refuse them
outright rather than accept and ignore them. DID
document fetch failures print their reason on stderr,
the artifact fetch posture. DESIGN.md's Amendment D
now records the narrowing: verification landed,
signing and key management still deferred.

The JCS module is proven on the RFC's printed number
and string values and on the draft's own section 3.5
example manifest, canonicalized with its signature
member removed. The signed fixture is a real openssl
ES256 signature over the fixture manifest's canonical
bytes, exercised in the core tests and end to end
through the binary. ring is the one new dependency,
named in almanac-core with its written reason beside
it; the lock gains exactly that one edge, verified
byte for byte against the landed lock after the
write fault truncated the working lock to zero bytes
mid build and it was rebuilt by hand.

### Trust verification checkpoint fix pass (2026-11-25)

Three reviews ran over the built slice before
staging. The red team found no critical and one
major, the DID document kid shadowing closed above,
with eight minors. The spec axis passed the slice
with three minor deviations, the subject shape
ownership, the comparison rule, and the draft
example test, all closed above. The standards axis
found one major, six rustfmt hunks in slice code,
closed by hand application of the formatter's own
diff, and eight minors, of which the base64url
table and length discipline, the cached key sort,
the stderr note, and the test deduplication are
closed in code, and the rest stand recorded as
accepted: the did:web percent encoded port form
neither binds nor resolves in this build and fails
safe to unverified, aggregate key fetch time is
unbounded under the fetcher's recorded limit, and
the envelope function stays one linear function
whose doc comment names its layers. The local gate
stayed write fault refused through the whole build;
the slice stages on the hosted verdict under the
Actions-first rule, with the format check verified
clean firsthand before staging.

### Trust verification hosted gate fix pass (2026-11-25)

The first hosted attempt was the slice's first
compile, as the staging report allowed for. The
format step passed and the code compiled; clippy
under the hosted toolchain refused four lints, all
in slice code. A manual div_ceil in the base64url
encoder's capacity arithmetic now calls div_ceil.
The factored findings sort takes a slice rather
than a vector reference. The RFC's first number
example is parsed from the RFC's printed text in
both tests that carry it instead of standing as a
float literal, which keeps the text as the source
and answers the excessive precision lint without
an allowance. Nothing else changed. The second
attempt compiled the core clean and refused in the
CLI: the key resolution named the document fetcher
bare where the file's idiom is the qualified path,
one word, now qualified like its neighbours. The
third attempt reached the test step and the RFC
vector caught a real defect in the JCS number
digits, the hard edge the design named. The build
had taken its shortest digits from Rust's own float
formatting, and that formatting can run one digit
long: the RFC's first example, 333333333.33333329,
serialized as 333333333.33333325 where ECMAScript
prints 333333333.3333333. The other eleven values
matched; this one is the counterexample. The digits
now come from the ryu crate, the provably shortest
algorithm ECMAScript implementations use, already
in the tree through serde_json and now named in
almanac-core with its reason beside it, like ring.
The placement rules are unchanged and were never
the fault. A manifest whose numbers hit this class
would have failed verification against a correct
signer; the gate earned its keep on the exact case
the design worried about.

A correction to that account, earned by the fourth
attempt, which failed identically with ryu in
place. Two independent formatters agreeing was the
tell: both were faithfully printing the double they
were handed, and the double was wrong. serde_json's
default float parser is not correctly rounded; it
landed the RFC's number one ulp low, at the double
whose shortest form genuinely ends in 25. The
defect was the parse, not the digits, and the
operative fix is the workspace dependency turning
on serde_json's float_roundtrip, so every float
this build reads is the double its text names. The
ryu digits stay, on their own merits: the provably
shortest digits by construction, never a platform
formatter's choice. But the round three paragraph
above misassigns the fault, and this paragraph
supersedes its diagnosis. The lesson is the fleet's
own: when two witnesses agree, check what they were
both looking at.

## Check integration of trust verification (design, 2026-11-25)

Proposed, awaiting Main's ruling before any code.
The trust slice fenced this off by name: validate
carries the verification, check does not change.
This section proposes the follow-up.

The gap. Verification lives in a command a
publisher runs by hand. The drift gate is what runs
in CI on every pull request, and check today reads
nothing of a catalog's trust manifests: a manifest
swapped, weakened, or stripped from a persisting
entry passes the gate as long as the entries
themselves match the baseline, because the
three-sided comparison never looks at the envelope.
The manifest is part of the published claim. The
gate should judge the claim it exists to judge.

Layer 1, the offline envelope judgments join check.
Check already parses the published catalog on every
run, repo mode and live alike. The proposal: check
runs the envelope layers over the published catalog
as part of assembling its findings, the shape
judgments at major, subject binding and identity
binding at critical, and the expiry judgments, all
under the kinds the trust slice established, folded
into the same deterministic report order. No new
inputs, no network, no behavior change for
catalogs carrying no manifests. Mechanically this
is the keyed pass's envelope half, factored so
validate and check run the same code over the same
rules; validate keeps its own report and exit
shape, check keeps its drift report and severity
gate, and the findings are indistinguishable in
kind and wording wherever they surface.

Layer 2, signatures engage only when a run holds
keys. Check accepts --trust-keys and --fetch-keys
with validate's meanings and key sources, and the
refuse-other-commands guard moves accordingly.
When a check run holds keys, the signature layer
runs as validate's does, invalid at critical,
unattemptable at minor naming why. When it holds
none, check makes no signature claim at all, in
either direction. This departs deliberately from
validate's never-silent rule, and the reason is the
difference in the two commands: validate is the
verification command, run to answer the signature
question; check is a drift gate run unattended on
every pull request, and a standing minor on every
signed catalog trains its readers to ignore the
report. A check run that means to gate signatures
says so by naming keys. Open question 1 for Main:
this scoping, or validate's rule carried over whole.

Layer 3, the baseline remembers the envelope. The
baseline records, per entry, the trust manifest's
presence and its JCS hash at generation time, an
additive field on the entry's baseline record with
the serde default posture the card display field
established, so pre-slice baselines judge as
before. Check then sees the two downgrade shapes
layer 1 cannot: a manifest the baseline recorded
and the published catalog no longer carries, and a
manifest whose bytes changed under the same entry.
The changed case is largely re-judged by layers 1
and 2 on its new contents; the removal case is the
crisp one, a signed entry going silently unsigned,
and the proposal names it at critical under a new
kind, trust-manifest-removed, with a changed
manifest that still verifies drawing no finding of
its own. Open question 2: the removal severity,
critical as proposed or major. Open question 3:
whether generation, which does not author
manifests, records the envelope from the published
catalog it replaces, the only place it can see one,
or whether the baseline record is written by a
validate run instead. The design's instinct is
generation, keeping one baseline writer.

Scope lines. No signing, still, under Amendment D.
No attestation fetching (the section 6.4 follow-up
stands separately) and no cross-origin url binding
(the section 6.2 follow-up likewise). Live mode and
repo mode judge identically, one code path, as
check already runs. The dusk.fm catalog stays
parked. Nothing here needs David.

Approved by Main 2026-11-25 with the three questions
answered as this record frames them: the signature
layer engages only when a run names keys, removal
stands at critical, and generation writes the
baseline's envelope record.

### As built (2026-11-25)

The envelope layers run in check through one shared
pass, envelope_pass in validate.rs, which validate's
keyed pass and check's envelope_findings both call,
so a manifest is judged identically wherever it
surfaces; a manifest repeated across the slots or
twice in the array is judged once, in its first
slot. verify_envelope takes the run's keys as an
option, and the verdict is the only layer the
option gates: shape, subject binding, identity
binding and expiry judge in every check run, keyed
or not. Check assembles keys through the same
validation_keys validate uses, file first and
did:web resolution behind --fetch-keys, only when a
key source is named. The baseline's record is
written by generation from the catalog it replaces:
generation reads the predecessor (the current path,
then the legacy path behind it; one unreadable copy
does not strand the other's envelopes, and a
predecessor nothing can read carries nothing, with
the warning in the report), carries its envelopes
onto the entries that persist, and records each
entry's envelope hashes, the sha256 of the JCS
canonical bytes of envelopes only. An expiry-only
manifest is not an envelope and earns no record.

The removal judgment measures envelopes, not
manifests: a recorded entry still published and
carrying no envelope has lost its envelope at
critical under trust-manifest-removed, whether the
envelope was stripped outright or swapped for an
expiry-only stub. A different envelope in its
place, a re-signing, is re-judged on its contents
and draws no removal; the recorded hashes identify
the baseline's envelope in the record rather than
gating the judgment, because a hash gate would fire
on every legitimate re-signing. Migration records
the same records from the entries it carries, a
judgment beyond the design's letter, taken because
a migrated site is a baseline's first writing and
leaving the class inert there until a regeneration
would exempt exactly the adoption path; it was
flagged for Main's ratification in the staging
report, and Main ratified it on 2026-11-25: every
baseline carries its envelope record from birth,
written by the command that writes the baseline. Migrate's standing limit applies to the
record as to its entries: migration vouches for
what it ingested at the level it verified it.

Known postures the reviews surfaced and this slice
keeps, in the open. The plural slot's identity
exemption is the draft's own model of third-party
attestations, not this build's invention: a
publisher envelope relocated into the plural array
is judged as a third-party attestation, and in an
unkeyed run a third-party envelope's only remaining
judgment is its signature, which the keyed scoping
leaves unclaimed. Both follow the draft and the
approved scoping, and changing either is a design
change, not a repair. Trust findings pass through
check's suppressions like every other class, so a
reasoned, expiring suppression can hold one back;
that is the drift model's standing term, stated
here because the headline class inherits it. The
removal class protects the window between a strip
and the next generation, which is the CI shape; a
regeneration after a strip records the stripped
catalog, as generation must, and the evidence
retires with it.

### Check integration checkpoint fix pass (2026-11-25)

The three reviews ran before staging. Spec fidelity
passed the implementation on every layer, 0 critical,
1 major, 2 minor. Standards returned 1 critical,
3 major, 7 minor. The red team returned 1 critical,
2 major, 7 minor. Every critical and major is closed
in the tree; the dispositions follow.

The red team's critical converged with the spec
axis's second minor and outranked it: as first
built, the recorded hashes were written and never
read, and the removal test was presence of any
manifest, so an envelope swapped for an expiry-only
stub passed check green in every configuration, the
exact hole the slice exists to close. Closed by
measuring envelopes, as the as-built now records:
the writer records envelopes only, and the removal
judgment asks whether the entry carries any
envelope at all. The spec axis's other minor,
migration recording beyond the design's letter,
stands as built and is flagged for ratification in
the staging report, with the reasoning in the
as-built.

The standards critical was this seat's own error,
the invented-helper class again: the new core test
named helpers that do not exist, where the test
module's own are entry, signed_entry and
fixture_keys. The test is rewritten on the real
helpers. Its major on the end-to-end test converged
with the spec axis's major: the planted manifest
carried no readable expiry, so the envelope's own
expiry judgment correctly drew a minor the test's
blanket silence assertion denied; the plant now
carries a far-future expiry, and its planted
signature's payload segment is empty, so the keyed
phase reaches the ES256 verdict it claims to test.
The standards majors on structure are closed by the
envelope_pass factoring (the two loops are one, and
drift's claim of identical judgment now rests on
shared code) and by moving the records helper above
generate's documentation, which it had split from
its function and its clippy exemption. The minors
closed beside them: the baseline record's fields
documented, the pre-slice parse test now pinning
the trust default, envelope_findings returning the
deterministic order its siblings return, one
now_unix helper where check added a third copy of
the snippet, the drift test naming its types as its
file does, and a direct pin on the envelope hash
against a hand-written canonical text. Semicolons
in new comments stand, as the file's own precedent
from the fourteenth slice stands; the reviewer
named it standing debt, not a break.

The red team's two majors, the plural slot's
identity exemption and the unkeyed third-party
silence, are recorded in the as-built as the
draft's postures rather than repaired, with the
reasoning there. Its minors: the suppression
inheritance and the strip-to-regeneration window
are recorded in the as-built; the duplicate
manifest duplication is closed by the factored
pass judging each manifest once; the unreadable
predecessor is answered in the as-built (the
legacy path is tried behind the current one);
an envelope that cannot be canonicalized earns no
record and no note, reachable only through a
non-finite number, which parsed JSON cannot carry;
renaming an entry orphans its record silently,
accepted beside the entry classes that judge the
rename's other faces; --fetch-keys in check is
the operator's named choice, as in validate, and
the usage text says what it does.

### Check integration hosted gate fix pass (2026-11-25)

The first hosted attempt (run 45597261454) compiled
the slice whole, format and clippy green, the core
suites green, and 52 of 53 CLI tests green. The one
failure was the slice's own end-to-end test at its
keyed phase, and the defect was in the test's
invocation, not the product: it passed --trust-keys
after --command, and --command consumes the rest of
argv by the CLI's standing contract, so the flag
became part of the fixture's command line and the
run was unkeyed, clean, and correctly silent. The
flag now stands before --command in the invocation.
The discipline was already on this seat's record;
the test forgot it, and the hosted gate is what
caught it.

## Attestation pinning (design, 2026-11-25)

Main approved this design as written on
2026-11-25, and its one open question is answered
as the record recommends: the pin layer engages
when named through --fetch-attestations or when
the run is already a fetching run, and
non-engaged runs stay silent about pins in both
directions. The trust design named this follow-up:
the draft's section 6.4 attestation pinning,
deferred when the fourteenth slice built the
envelope layers.

The research, read firsthand from
draft-bu-dawn-ard-discovery-00 today. A trust
manifest may carry attestations, an array of items
shaped (the draft's section 3.5 example and its
CDDL alike) as type (a URI naming the statement
kind, required), uri (where the external document
lives, required), mediaType (optional), and digest
(optional, <alg>:<hex>, sha256 mandatory to
implement). Section 3.5.2 names two distinct
integrity bindings. The first binds the attestation
statement itself to the target artifact: the
statement must sign the artifact or carry a signed
subject claim naming the resource's identifier or
digest. The second pins the external document:
because attestation documents may live on external
servers in formats without self-contained
signatures, the digest inside each attestations
item records the hash of the external document
itself, sealed under the manifest's signature as a
subresource integrity pin, so the file at the uri
cannot be replaced or modified after signing.
Section 6.4 restates the second as a consumer duty:
when an attestations item includes digest, a
consumer fetching the document from the uri MUST
verify the fetched document's digest matches.

This slice builds the second binding, the pin. The
first binding, reading SLSA and in-toto statements
to confirm their internal subjects, is a deeper
water (foreign statement formats, each with its own
subject grammar) and stands as its own named
follow-up; the envelope's own subject binding,
landed in the fourteenth slice, already covers
transplantation at the envelope level, and the pin
covers substitution of the document itself.
Provenance links (sourceId and sourceDigest) carry
no uri and pin nothing fetchable; out of scope.

The judgments, inside the envelope pass both
commands already share, over every envelope an
entry carries, singular and plural alike. Item
shape first, at major under trust-manifest-shape:
attestations present but not an array, an item not
an object, an item missing its type or its uri, and
a digest present without a uri to fetch (a pin with
nothing pinned is a defect, not a claim). A
malformed digest, not <alg>:<hex>, is the same
major. The pin itself, when an item carries a
well-formed digest and the run engages the pin
layer: the fetched bytes' sha256 against the pin.
A mismatch is critical under a new kind,
trust-manifest-attestation-mismatch: the sealed
pin names one document and the uri serves another,
the section 6.4 substitution, at the severity of
the other sealed-claim failures. A pin the run
engaged but could not fetch, or whose algorithm is
not sha256 (this build computes sha256 only, the
draft's mandatory one), is a minor under
trust-manifest-attestation-unverified naming why,
never a silent pass in an engaged run. An item
with no digest carries no pin and is silent, as
unsigned content is.

Engagement is the design's one open question, and
the recommendation follows the fifteenth slice's
rule for network judgments: they engage when named
or when the run already fetches. A new flag,
--fetch-attestations, names it on validate and on
check; a check run that is already a fetching run
(an HTTP target, named or resolved, or
--fetch-remote) engages it without the flag, as
those runs already do network work and the pin is
part of the drift claim, the posture the media
type observation set. A run that does not engage
the layer is silent about pins in both directions,
so a repo-mode CI gate gains no standing minor from
a claim it never set out to check. Fetches ride
the guarded document fetcher, no credential ever,
and the guard matters more here than anywhere:
attestation uris are catalog content, and a literal
non-public address is refused before any request,
the refusal surfacing as the unverified minor's
reason. Fetched bytes are hashed raw, as served;
the draft pins the document itself, not a
canonical form.

Mechanically: the core stays network-free. The CLI
collects, per entry and attestation uri, an
observation (the fetched bytes' sha256, or the
failure's reason) in engaged runs, and the envelope
pass takes the observations as an input beside the
keys, judging shape offline in every run and pins
only from observations. Validate's refusal guard
moves again: --fetch-attestations belongs to
validate and check, and generate and migrate refuse
it with the trust flags.

Scope lines. No statement parsing (the first
binding stands as a named follow-up), no provenance
judgments, no local path resolution for attestation
uris in this slice (an own-origin document is
fetched over HTTP like any other; site-relative
reading is a refinement for a later pass), no
signing. The dusk.fm catalog stays parked. Nothing
here needs David.

### As built (2026-11-25)

The pin layer stands as designed. The core gained
PinObservation and PinOutcome in trust.rs, plain
values beside TrustKey, and judge_attestations in
validate.rs, called from verify_envelope between
the identity layer and the signature, so item
shape is judged even on an envelope whose
signature member is absent. The envelope pass and
both its entry points take the observations beside
the keys; drift's CheckInputs carries them the
same way. The CLI collects observations in
pin_observations through the guarded fetcher,
mirroring the judge's gates (envelopes only, pins
in <alg>:<hex> form), each (entry, uri) pair
fetched once. Check engages when it fetches or
when --fetch-attestations is named; validate
engages on the flag alone, the design's operative
sentence scoping the without-the-flag clause to
check runs, recorded here against the design's
more general summary sentence. Generate and
migrate refuse the flag with the trust flags.

Known postures, recorded rather than repaired.
Attestation shape is judged inside envelopes
only, the fourteenth slice's standing scoping:
an attestations-only manifest is not an envelope
and is silent in every run, as an expiry-only
stub is. Attestation fetches join the fetcher's
standing posture on aggregate time: each fetch is
bounded (30 seconds, 16 MiB, the non-public
guard), the count is deduped per (entry, uri) but
not capped, so a catalog carrying many distinct
pinned uris costs proportionally, the same
exposure artifact fetching already carries and
accepts. A uri's form beyond non-emptiness is
the fetch's to fail: a relative or padded uri is
no shape defect in this slice and surfaces in an
engaged run as the unverified minor its failed
fetch produces.

### Attestation pinning checkpoint fix pass (2026-11-25)

All three reviews ran before staging. Spec
fidelity FAIL on one critical, the product
conforming sentence by sentence; standards one
critical, the same defect from its own axis; red
team no critical, one major, four minor, and a
clean trace on every silent-pass route it tried.
Every critical and major is closed here.

The critical, owned plainly: the slice's
end-to-end test named helpers that do not exist
(check_stdio misused as a setup returning a site,
a fixture_path that is defined nowhere) and never
generated its catalog, so it could not compile,
the invented-helper class this seat has shipped
before and greps against. The test stands
rewritten on the file's real harness, the
fifteenth slice's pattern (temp_site,
fixture_binary, generate_stdio, check_stdio), and
its plant is the sibling test's own.

The red team's major converged on the rewrite:
the test's unengaged phases originally ran while
the served document still matched its pin, so an
always-engaged CLI would have passed them; the
scoping rule had no discriminating test at the
binary level. The rewritten test re-runs both
unengaged commands after the substitution, where
silence is the only correct outcome, and adds the
fetching-run engagement phase and the generate
refusal the spec review found unpinned. The core
test likewise gained the non-sha256 branch and
the non-array shape the spec review listed.

Minors closed: the collector now mirrors the
judge's gates, so engaged runs no longer fetch
for manifests and digests no judgment will use
(the red team, standards, and spec reviews
converged here from three sides); identical
attestation items repeated in one array are
judged once; verify_envelope's layer map and
validate_catalog_with_keys' parameter doc name
the new layer and input; the core test's name
says what it proves. The remaining minors are
the known postures recorded in the as-built
above.

## Cross-origin url binding (design, 2026-11-25)

Main approved this design as written on
2026-11-25, and its three questions are answered
with the record's recommendations: the boundary
is the host, the layer engages on the envelope's
presence and bindings rather than on a verified
signature, and a declaration is read by substring,
named in the open as the weak form it is. The
consequence is accepted knowingly: the normal
deployment shape is cross-origin by this rule and
its descriptors must declare their entries for
the gate to pass them. The last named follow-up
from the trust design: the draft's cross-origin
target binding, sections 4.3.1 and 6.2.

The research, read firsthand from
draft-bu-dawn-ard-discovery-00 today. Section
4.3.1 names the boundary exactly: a valid
trustManifest signature from <publisher> proves
<publisher> authorized the entry, but if url
points to a different origin (the draft's example
is urn:air:attacker.example claiming a url on
victim.example.com), the signature alone does not
prove the target origin consented to host the
resource. When the origin of url differs from
https://<publisher>, consumers and verifying
registries MUST verify that the artifact
descriptor retrieved from url matches the entry's
signed cryptographic digest and explicitly
declares the matching identifier
(urn:air:<publisher>:...) or publisher identity
(trustManifest.identity). Section 6.2 restates it
as the cross-origin target url binding duty.

Half of the duty already stands. In a fetching
run, check compares the served artifact's bytes
against the entry's digest at critical, which is
the digest half, and the envelope's subject
binding seals that digest to the signer. The new
judgment is the declaration half: the descriptor
at the target does not merely hash to the sealed
bytes, it names the entry or its signer back.

The honest difficulty, stated before the design:
no descriptor format almanac reads defines a
field for this declaration. The server card
(SEP-2127 Final) carries name, websiteUrl,
repository, remotes; the A2A card carries name,
url, a provider object; an OpenAPI document
carries info and servers; SKILL.md carries
frontmatter. None carries an ARD identifier. So
the declaration is read the only way it can be
read mechanically: a string in the descriptor
equal to the entry's identifier or to the
envelope's identity. For a JSON descriptor the
strings are its values, searched recursively; for
a text descriptor (SKILL.md) the text itself. A
descriptor that declares neither, retrieved for
an entry that claims a cross-origin url under a
publisher envelope, fails at critical under a new
kind, trust-manifest-cross-origin-unbound: the
signed claim points at an origin that nowhere
acknowledges it, the impersonation shape section
4.3.1 exists to catch, at the severity of the
other binding failures.

Scope. Check only, in fetching runs: validate
retrieves no artifact descriptors, and the
descriptor must be in hand for the judgment to
mean anything; a descriptor the run could not
retrieve draws no finding from this class (the
artifact classes already judge unreachability and
digest failure on their own terms). The CLI hands
the core the descriptor text for each enveloped
cross-origin entry from the same bytes its
artifact handling already reads or fetches, the
pin layer's pattern of plain values in,
judgments out. Entries with data instead of url
are out of scope. Same-origin entries are silent.
Entries with no publisher envelope are silent:
the duty's premise is a signed claim, and an
unsigned cross-origin entry is already an
unverified discovery hint under the standing
posture.

Three questions for Main, with the record's
recommendations. (1) Origin or host? The draft
says origin, and its example is a different host.
This design reads the boundary at the host: url
is cross-origin when its host differs from the
publisher domain (case-insensitive, port and
scheme ignored), because the consent being
tested is the host operator's, and an https
catalog entry pointing at its own host over http
is a transport defect other classes judge, not
an unconsenting third party. The alternative,
the RFC origin triple, is the more literal
reading. (2) Signature premise. Section 4.3.1
premises a valid signature; the gate often runs
unkeyed. This design engages on the envelope's
presence and bindings, verified or not, as the
other envelope layers judge the claim as
published; a keyed-only scoping would leave the
class nearly unreachable in CI. (3) Declaration
by value. A descriptor string containing the
identifier or the identity as a substring counts
as declaring it (equality included); the
alternative, whole-value equality only, is
stricter and would miss a declaration carried in
a sentence or a longer field. This design takes
the substring reading and names it, because a
descriptor that mentions the entry's URN at all
is acknowledging the entry in the only way any
current format can.

The dusk.fm catalog stays parked, and the
deployment shape this class will meet there is
worth naming in the open: a catalog on one host
announcing an MCP endpoint on another is the
normal cross-origin shape, and its server card
will need to declare the entry or its signer for
this gate to pass it, which is the draft's
intention, not an accident. Nothing here needs
David.

### As built (2026-11-25)

The declaration half stands as designed. Check's
trust pass judges it in check_cross_origin over
descriptor texts the CLI collects in the runs
that fetch, handed in as plain (identifier, url,
text) triples on the pin layer's pattern. The
gate (singular envelope, url present, host
differs from the identifier's publisher segment,
the catalog host standing in for a non-URN
identifier) is one core predicate,
cross_origin_host in validate.rs, which judge and
collector share. A JSON descriptor declares
through any of its string values, a text
descriptor through its text, by substring, with
an empty name declaring nothing. Validate holds
no part of it.

Two widenings against the design text, recorded
for Main's ratification. The engagement follows
the standing rule for network judgments rather
than the design's narrower phrase: a check run
engages the class when it fetches for any reason,
a target or --fetch-remote or
--fetch-attestations, not only in the runs the
record called fetching. And the descriptor bytes
come through the same local-first mapping and
guarded fetcher the artifact handling uses, read
for this class rather than reused from the
artifact loop's hands, so a cross-origin baseline
artifact can be fetched twice in one run;
provenance is identical, the cost is one fetch.

Known postures, recorded rather than repaired.
The class reads the singular slot only, the
standing model in which trustManifest is the
publisher's own and the plural array carries
third parties; an envelope living only in the
plural slot is judged there as a third-party
attestation, here not at all. A descriptor read
for a federated entry whose url sits on the
catalog host comes from the checker's own site
directory, the artifact classes' standing
premise. Redirects are judged on the landing
bytes, the fetcher's standing posture.

### Cross-origin checkpoint fix pass (2026-11-25)

All three reviews ran before staging. Spec
fidelity PASS, no critical or major, four minor.
Standards PASS, no critical or major, four minor.
Red team no critical, one major, eight minor, and
a confirmed convergence between collector and
judge on every route it traced but one. Every
major and the repairable minors are closed here.

The major was a parser differential, and it was
real: url_host was hand-rolled and did not treat
a backslash as an authority terminator, so a url
spelled victim.example.com\@example.com named
the publisher to this parser and the victim to a
WHATWG parser, and the class skipped it as
same-origin; the empty-authority spelling
(https:///host) disengaged it the same way. The
artifact classes backstopped baselined entries,
which kept it major rather than critical, but a
class whose defense rests on another parser's
strictness is not defended. url_host now follows
the WHATWG authority rules where they decide the
host (backslash terminates, leading slashes are
skipped), drops the DNS root dot, and the core
test judges both spellings at the host a browser
would name. The same test now pins the boundary's
insensitivities (scheme, case, port, root dot on
the publisher's own host stay silent), which the
spec review found implemented but untested.

Also closed: the duplicated gate composition the
standards review flagged is now the single
cross_origin_host predicate (which also
normalizes a port out of the fallback publisher,
a red team minor); descriptors are keyed by
identifier and url together, so two entries
sharing an identifier no longer judge against one
descriptor (red team); an empty identifier or
identity no longer declares everything by
containing the empty string (red team); the e2e's
silent phases assert the run could run, not only
that the kind is absent (standards); value_declares
carries its doc line (standards). The remaining
minors are the known postures and widenings
recorded in the as-built above.

Ratified by Main on 2026-11-25: both widenings
stand. The engagement rule is the standing
one-rule for network judgments approved with the
sixteenth slice, and the design text's narrower
phrasing was that rule's earlier draft; the
descriptor read keeps provenance identical
through the same mapping and fetcher, its
possible duplicate fetch harmless against a
judgment that must say exactly where its bytes
came from.

### Cross-origin hosted gate fix pass (2026-11-25)

Round 1 (run 45604399580) failed in the test step
only, format, clippy, compile, and the core suites
green, 54 of 55 CLI tests green. The one failure
was the slice's own end-to-end test, two
test-side defects in its plant, both mine: the
subject digest prefixed sha256: onto an entry
digest that already carries its full form (the
sixteenth slice's plant uses the digest verbatim,
and this one should have), drawing a subject
mismatch beside the expected cross-origin
critical; and the declaring descriptor named a
remembered identifier, mcp:demo, where the
fixture's entry is mcp:telorama, so the declaring
phase declared nothing and the class correctly
fired. Both are fixed in the test alone: the
subject digest stands verbatim, and the declaring
descriptor is built from the identifier the plant
actually read. No product code changed.

## Attestation statement subject binding (design, 2026-11-25)

Main approved this design as written on
2026-11-25, and its two questions are answered
with the record's recommendations: the severity
split stands (a statement claiming another
subject is the transplant at critical; a
statement with no readable claim is unverified at
minor), and the entry's url is accepted as a
binding name beside its identifier. The scope
line holds as drawn: the claim is judged, the
attester's own signature is not built. The
remaining binding from the attestation pinning
design: section 3.5.2's first binding, restated
for consumers at section 6.4.

The research, read firsthand from
draft-bu-dawn-ard-discovery-00 today. Section
3.5.2 names two bindings; the sixteenth slice
built the second (the pin). The first: to prevent
attestation transplantation, where an attacker
copies a valid attestation from one agent into
another entry's metadata, each third-party
attestation statement MUST either be a direct
cryptographic signature over the artifact or
include a signed subject claim that identifies
the resource's identifier and/or the artifact's
content digest. Section 6.4 gives the consumer
its side: signed attestations MUST either
directly sign the target artifact or bind to the
resource's identifier and/or the artifact's
algorithm-prefixed digest, and when an entry
delivers its artifact by reference, consumers
MUST compute the digest of the retrieved artifact
and verify that it matches the entry's digest and
the attestation subject before honoring any audit
or provenance claims.

What this build can and cannot judge, stated
before the design. The statement's own signature
is the attester's cryptography (a DSSE envelope,
a sigstore bundle), verified against the
attester's keys, which are not the publisher's
and are nowhere in this system; verifying it is a
second verification stack and is NOT built here,
the same line Amendment D drew at signing. What
the gate can judge, on the fetched statement the
pin layer already retrieves, is the claim itself:
whether the statement's subject names this entry,
by its identifier or by the digest of its
artifact. A transplanted attestation fails
exactly there: it is a perfectly valid statement
about a different artifact, and its subject says
so. The layer judges the claim as published, as
every envelope layer does.

The judgment, inside judge_attestations in the
runs that already engage the pin layer, over the
same observations (the collector widens to fetch
every attestation item carrying a uri, pinned or
not; the fetched text rides the observation into
the core, which stays network-free). The core
reduces a fetched statement to its subject
claims: the in-toto subject form the draft's own
examples name (a subject array whose items carry
a name and a digest object), an ARD-shaped
subject object (identifier and digest as the
envelope's own subject carries them), through
one DSSE unwrap when the document is an envelope
whose payload decodes to the statement. A
statement binds when any claimed digest equals
the entry's digest or any claimed name equals
the entry's identifier or its url. Three
outcomes per item in an engaged run: the
statement claims a subject and none of it binds
this entry, a Critical under a new kind,
trust-manifest-attestation-unbound, the
transplant positively identified; the statement
carries no subject claim this build can read (a
PDF report, a SARIF file, a bundle form it does
not unwrap), a Minor under
trust-manifest-attestation-unverified naming
that limit, never a silent honoring and never a
critical on a format the draft itself names as
an attestation document; the statement binds,
silence. A failed fetch keeps the pin layer's
unverified minor and adds nothing here.

Two questions for Main, with the record's
recommendations. (1) The severity split above:
a statement that positively claims another
subject is the transplant at Critical, a
statement with no readable claim is unverified
at Minor. The alternative, every unbound
statement at Critical, fails the draft's own
named examples (PDF compliance reports carry no
readable subject) on every catalog that uses
them. This design takes the split. (2) The
subject name accepted as an identifier claim: an
in-toto subject name is usually an artifact name
or uri, so the design accepts the entry's url as
well as its identifier as a binding name. The
alternative, identifier only, is stricter and
would leave most real SLSA statements to bind on
their digest claim alone, which they also carry;
this design accepts both and names it.

Scope lines. No attester signature verification,
no sigstore bundle parsing beyond the one DSSE
unwrap, no new engagement rule (the pin layer's
runs, both commands), no change to the pin
judgments themselves. The dusk.fm catalog stays
parked. Nothing here needs David.

### As built (2026-11-25)

A new core module, statement.rs, reduces a fetched
statement to its subject claims (digests normalized
to <alg>:<hex> with a lowercase algorithm through
one shared helper, names as written) from an
in-toto subject array, an ARD-shaped subject
object, or a DSSE envelope typed for in-toto whose
payload decodes to the statement; the core stays
network-free and the statement's own signature is
never read. PinOutcome::Fetched now carries the
served bytes' text beside their sha256, and
judge_attestations judges every item carrying a
uri in an engaged run: the pin comparison where a
well-formed pin stands, then judge_statement,
which binds on a claimed digest equal to the
entry's digest or a claimed name equal to its
identifier or its url. A contradicted claim is
critical under trust-manifest-attestation-unbound;
no readable claim is minor under
trust-manifest-attestation-unverified. The
collector fetches every attestation item carrying
a uri in an envelope, pinned or not. Tests: the
statement module's own reduction tests, a core
test walking every binding route, the transplant,
the unreadable statement, the unengaged run, and
the digest-less entry below, and an end-to-end
test driving the same phases against the binary;
the pin test's served document is now a binding
statement, as the layer requires.

### Statement binding checkpoint fix pass (2026-11-25)

The three reviews ran before staging. Spec
fidelity PASS, no critical or major findings;
red team none either; standards one major, a doc
defect. All of it is closed here. The header of
judge_attestations still closed with the
sixteenth slice's sentence that an item without
a digest is silent; it now records the statement
judgment, and a stale test comment beside it is
corrected the same way. A malformed pin no
longer shields the statement behind it: the
shape major stands and the item is judged on its
claim with no pin, which also makes the
collector's widened fetch honest, since the
fetched document is consumed for every item the
collector fetches. The DSSE unwrap now requires
the in-toto payload type, so a document carrying
payload keys for another format is read at its
own top level. One outcome refinement came out
of the red team's reading, flagged to Main with
the staging report for ratification: a statement
claiming only digests, to an entry that records
no digest, cannot be answered on any channel the
entry holds, and reads unverified at minor rather
than the transplant at critical; a claim
contradicted on a channel the entry can answer
(names against its identifier or url, digests
against a recorded digest) keeps the critical.
Recorded as observations: the reduction reads
name, identifier, string digests, and object
digests in both subject positions, a benign
superset of the record's two named shapes that
only accepts more binding claims; a non-sha256
pin whose fetch failed now reports the fetch
reason rather than the algorithm, same kind and
severity; a statement-only item whose fetch fails
draws the unverified minor, the never-silent
reading; the end-to-end test binds by digest,
with name, url, and DSSE routes covered at core
level. The owner's own pass caught one further
defect before the reviews: the base64
accumulator grew unmasked and would have
overflowed on a real payload in a debug build; it
now keeps only the bits not yet pushed.

Ratified by Main on 2026-11-25: the outcome
refinement stands as built. Critical in this pass
means a positively identified contradiction on a
channel the entry can answer, and a digest-only
claim against an entry that records no digest is
unanswerable rather than contradicted, so
unverified at minor is the honest reading.

### Statement binding hosted gate fix pass (2026-11-25)

PR #20's first hosted run failed in the test step
on the sixteenth slice's end-to-end test, a
test-side premise of exactly the kind the core
test's update had already handled: the pin test's
served document was plain text, which the new
statement layer correctly reports as carrying no
subject claim this build can read, breaking the
test's blanket silence assertions in its clean
phases. The document is now a statement binding
the entry by its digest, rebuilt around the
digest the test reads from the generated catalog,
so the pin test exercises the pin layer against a
statement the statement layer honors. The failure
was read firsthand from the job log; format,
clippy, compile, and the core suites were green,
and the slice's own end-to-end test passed.

## Nineteenth slice: local $ref schemas resolved (design, 2026-11-25)

Main approved this design as written on
2026-11-25: the twelfth slice's own move applied
to the surface it left standing, with the
termination rule, the sibling-overlay reading,
and external references staying literal, and the
one-time re-baseline consequence named in the
open. The OpenAPI reduction's remaining named limit,
recorded by the twelfth slice when it resolved
parameters and left schemas standing: a schema
carried by reference is covered only by the
components hash, which reports that the shared
pool changed without naming which operation it
touches, and attribution without resolution would
be invented. The twelfth slice's own move retires
the limit the same way it retired the parameter
one. The refusal's reasoning was that resolution
means following pointers into other documents,
which is a fetch the core never does; a local
reference resolves against the document already
in hand, and external references stay literal and
unfollowed, exactly as they do for parameters.

The design. Each operation's contract hash is
computed over the operation object with its
description, summary, and parameters removed, as
today; before hashing, local $ref nodes inside
that object are expanded in place, each replaced
by the value its fragment pointer names in the
same document, recursively. A pointer already
being expanded on its own branch stays literal,
so a self-referential schema terminates and its
literal form still hashes. Where an expanded node
carries sibling keys beside the $ref, the pointed
object takes them overlaid, siblings winning, the
OpenAPI 3.1 reading in which siblings augment the
reference; a sibling beside a non-object target
leaves the node unexpanded rather than inventing
a merge. The expansion is a pure function of the
document feeding only the hash; the model stores
nothing new. The components hash stands as it is:
it still covers the pool whole, including schemas
no operation references, whose change would
otherwise be invisible, so a referenced schema's
change now reports both the attributed operation
change at major and the pool change, the same
double report a shared parameter's change already
produces through its own channels.

The recorded consequence, the twelfth slice's
exact precedent: contract hashes change for every
operation carrying a local $ref, so pre-slice
baselines report those operations changed at
major until the publisher regenerates. It is a
one-time re-baseline, stated here rather than
discovered by the first publisher who hits it.

Scope lines. Local fragment pointers only;
external references, file or URL, stay literal.
Resolution feeds the contract hash only, not the
parameter channels (the twelfth slice's ground)
and not the description channel. No new finding
kinds: an attributed schema change arrives as the
standing openapi-operation-changed at major,
which is the point of the slice, since that class
already names the operation the pool hash cannot.
The dusk.fm catalog stays parked. Nothing here
needs David.

### As built (2026-11-25)

The expansion lives in openapi.rs as a small
RefExpansion walk over the contract object after
the three removals, reusing the twelfth slice's
resolve_local_pointer and feeding only the
contract hash; the module header's limit
paragraph now records schemas resolved beside
parameters. Two tests carry it: one walking
attribution, determinism over the self-cycle, the
external literal, the sibling overlay, and a
golden hash of the hand-built expanded contract
with the cycle's recursive edge literal inside
it; and one pinning the literal edges and bounds
below.

### Schema resolution checkpoint fix pass (2026-11-25)

The three reviews ran before staging. Spec
fidelity PASS and standards clean at critical and
major; the red team's one major is the pass's
substance. The expansion as first built had no
budget: a doubling chain of shared references,
each schema referencing the last twice, expands
exponentially from a few kilobytes of input, and
a flat chain of references recurses one frame per
link with nothing bounding the depth. The walk
now carries two bounds, both failing safe the way
the cycle rule does (the node stays literal,
still hashes, and the components hash still
covers its target): a branch follows at most 64
references, and one operation's expansion
produces at most 100,000 nodes inside followed
references, the operation's own inline content
uncounted because the document already bounds it.
The bounds are pinned by tests built to trip
them observably: a hundred-link chain whose leaf
beyond the depth bound cannot move the contract
while a link inside it can, and a wide inline
subtree that spends the node budget before a
later reference is reached, with the same
document under budget as its control. The
reviews' test gaps are closed in the same pass:
the golden hash above, a sibling beside a
non-object target staying unexpanded while the
bare reference expands, and an unresolvable
pointer pinned literal by its own golden hash.
The nested conditionals are restructured as a let
chain, and the doc now says a reference is not
followed rather than stays literal, since the
walk still visits such a node's other children.
Recorded as postures, not built: expansion
follows the design's letter for any local
pointer, so a reference pointing outside
schema-shaped places (the document root, another
operation) pulls that subtree into the contract;
such a reference is not valid OpenAPI, and the
failure direction is false attribution, never
missed drift. Aliased spellings of one target are
distinct strings to the cycle guard; termination
still holds on the finite document, and the depth
bound caps the extra rounds. Shared schemas are
re-expanded per occurrence rather than memoized,
a conscious accept: the budget, not a cache, is
the bound the gate needs.

## Twentieth slice: the served catalog (design, 2026-11-25)

Main approved this design as written on
2026-11-25, and its one question is answered
major, as designed: the critical band is reserved
for integrity losses and third-party
substitution, and a stale deploy is the
publisher's own deployment lag, which is the
major band's exact meaning; the finding exists
and fails by default, which answers the
consumers-read-the-served-copy argument. The gap
this slice closes is the deployment the gate
never looks at. Check compares three sides:
the published catalog in the site, the baseline,
and the live surface behind the entries. The
catalog's own deployed copy is none of the three.
A publisher can regenerate, commit, and pass
every gate while the host keeps serving last
month's ard.json, because nothing in the run ever
reads the catalog as deployed; the tenth slice
fetches it for its media type and deliberately
does not compare its bytes. For a static catalog
the stale deploy is the most ordinary drift there
is, and today it is invisible.

The design. In a fetching run (the standing
engagement: an HTTP target named or resolved, or
--fetch-remote), check also fetches the catalog
at its own well-known paths on the catalog's
domain, ard.json first and ai-catalog.json beside
it, through the guarded fetcher the media type
observation already rides. Each fetched document
is parsed and re-serialized in the catalog's
canonical form (sorted keys, two-space indent,
LF, one trailing newline, the form generation
writes) and compared against the site's published
copy of the same path. A difference is a major
under a new kind, catalog-served-differs, and the
finding names the entry-level differences rather
than the bare fact: identifiers served but not
published, published but not served, and, for
identifiers on both sides, the fields that differ
(digest, version, url), so the reader sees what
the stale deploy is missing. A served copy that
does not parse is the same major naming that it
could not be read. The posture is the media
type's, best-effort in both directions: a path
the run cannot fetch, or one answering
non-success, draws no finding from this class,
because the run's other classes already judge
dead and missing surfaces and this class exists
to judge content, not reachability. A legacy-only
site compares its one path, mirroring the media
type's legacy skip. The comparison is against the
published copy in the site the run was given, in
the same run, so a repo and its deployment are
judged as one moment.

One question for Main, with the record's
recommendation. Severity: this design puts the
class at major, the drift model's line for a
surface that no longer matches what the publisher
announced, failing by default like a removed
operation. The alternative is critical, on the
argument that consumers read the served catalog,
not the repo, so a stale served catalog is the
announced surface being wrong in the only place
it is read. This design takes major: the served
catalog is the publisher's own deployment lag,
not a third party's substitution, and the
critical band has so far been reserved for
integrity losses and unannounced capability.
Either ruling is a one-word change in one place.

Scope lines. Fetching runs only; repo mode gains
no network behavior. No comparison of the served
catalog against the baseline (the published copy
is the deployment's source, and published against
baseline is the standing comparison). The media
type observation is untouched and keeps its own
posture. Nothing here needs David.

### As built (2026-11-25)

The comparison lives in the core as
check_served_catalog beside the media type class:
each served document is parsed to the catalog
model (a byte order mark stripped first, a
serving artifact rather than content) and
compared as a model against the published copy
of the same path, the legacy model standing for
the legacy path when the site carries one. One
major per differing path, detail naming served
only, published only, and changed identifiers
with their differing fields among digest,
version, and url (other fields when an entry
differs only elsewhere; the host block or the
entries' order when no entry carries the
difference). The CLI collects in fetching runs
only, through a new fetcher method that keeps
the media type observation's fallback posture
exactly: http is tried only when the https
request could not be made at all. The domain
guard gains the backslash the WHATWG parser
treats as a slash, in this collector and in the
media type observation's identical guard. DESIGN
section 6 gains the class in its major list. A
core test and an end-to-end test carry it: the
deployment serving the published bytes is
silent, a lagging digest fails the run at major,
and a run that does not fetch compares nothing.

### Served catalog checkpoint fix pass (2026-11-25)

The three reviews ran before staging: spec
fidelity PASS, the red team clean at critical
and major, standards with one critical the pass
exists for. The model equality as first written
compared serde_json's Result values directly,
which does not compile (its Error carries no
PartialEq); the comparison now unwraps both
values and treats an unserializable model as
differing. The same pass closed what the reviews
found around it: the collector's insertion had
stranded the cross-origin collector's doc
comment on the new function (the function moved
below its neighbor, doc restored, and the
collector renamed served_catalog_documents after
the file's idiom); a duplicated sentence tail in
DESIGN's major list is removed; the comparison
is per path against that path's published copy
rather than the one model for both; the http
fallback is the typed posture above rather than
any-error; the detail no longer mislocates an
entry's unnamed-field difference outside the
entries; and the tests now exercise the version
and url arms and the one-finding-per-path shape.

### Served catalog hosted gate fix pass (2026-11-25)

PR #22's first hosted run failed in the test step
on the tenth slice's media type test, the only
failure: a cross-slice premise of the same family
as PR #20's round. That test's server answered
every path with a placeholder body, which the
tenth slice never read; a fetching run now also
reads the served catalog, and the placeholder is
a differing catalog at major on both paths. The
product behaved as designed; the older fixture's
premise is what changed. The test now serves the
published catalog's own bytes after generation,
so it judges the media type alone again, and the
slice's own end-to-end test passed in the same
run.

## Twenty-first slice: boolean spellings at boolean positions (design, 2026-11-25)

Main approved this design as written on
2026-11-25, and its one question is answered as
designed: the single letters stay out, as stray
data likelier than intent. The YAML slice
recorded its limit plainly:
serde_yaml reads YAML 1.2 core scalars, so the
YAML 1.1 boolean spellings (yes, no, on, off)
arrive as strings, and a parameter marked
required: yes records as optional until the
document is corrected to true, which the gate
then reports as a required parameter added. The
harm is not cosmetic. A required parameter is a
major class when it changes; a publisher whose
YAML uses the 1.1 spellings, which much real
YAML in the wild does, gets a baseline that
understates their contract, and a phantom major
later when the document is modernized, for a
change that never happened on the surface.

The design. The reduction reads a boolean in
exactly two places: a parameter's required flag
in the OpenAPI reduction (openapi.rs, through
the resolved parameter object, so $ref parameters
are covered by the same read), and the A2A
card's capability flags in reduce_agent_card
(a2a.rs). Both reads gain one shared helper in
the core: a value reads as a boolean when it is
a JSON boolean, or when it is a string carrying
one of the YAML 1.1 boolean spellings, the
spec's case forms of yes, no, on, off, true, and
false. Any other string, and any other type,
reads as no boolean, exactly as today. The
helper is format-blind, as the YAML slice made
everything downstream of the parse: a JSON
document writing "required": "yes" is invalid
OpenAPI either way, and the lenient read at a
boolean position is the same read. Nothing else
changes: strings elsewhere in the document (a
description reading "yes", a server url) are
never converted, because the helper is consulted
only where a boolean is being read.

One question for Main, with the record's
recommendation. The YAML 1.1 set also includes
the single letters y and n. This design excludes
them: the spellings real documents use are the
word forms, and a one-letter string is likelier
to be data that wandered into a boolean position
than a boolean anyone wrote. Including them is
a two-line change to the helper's table.

Scope lines. The two read points only; the
document-level security read is a presence read,
not a boolean read, and is untouched. The
unquoted-numeric-version half of the YAML
record's limits stands as recorded (a version
field is a string position, and inventing a
number-to-string coercion there would rewrite
data, not read a boolean). Baselines recorded
under the old read report the newly visible
required parameters as required-parameter-added
at major until the publisher regenerates, the
twelfth slice's one-time re-baseline shape,
stated here so the record owns it. Nothing here
needs David.

### As built (2026-11-25)

One helper, read_bool at the core's root, consulted
at the two reads and nowhere else; a crate-wide
grep confirms as_bool survives only inside it.
The placement is a first of its kind in this
crate, whose other shared helpers live in topical
modules; it stands because the two consumers sit
in unrelated modules and no existing module owns
document scalar reading, and the record names it
so the next cross-module helper chooses
deliberately. Call-site tests sample the table at
both positions, including a $ref parameter and
the excluded single letter; a root test reads the
full table. The re-baseline consequence the
design states for required parameters holds
identically on the A2A side: a baseline recorded
under the old read from a YAML card carrying
streaming: yes reports one a2a-capabilities
change at major until regeneration. Recorded as
postures: a spelling rewrite inside components
still moves the pool hash, which hashes the
parsed pool as it stands and can over-report a
pool change on a modernization but can never hide
a required parameter change, the channel this
slice repairs; and an A2A extension item's own
required flag is read in no spelling, a
pre-existing coverage gap beside the flags call
site, named here for a later A2A slice, with the
extensions field's doc corrected in this pass to
stop claiming the gating it does not do.

### Boolean spellings checkpoint fix pass (2026-11-25)

The three reviews ran before staging: spec
fidelity PASS, standards PASS, the red team clean
at critical and major. Their minors converge on
two repairs, both made: the spelling table is
now pinned exhaustively by a test of the helper
itself (all eighteen word forms, the JSON
passthrough, and the exclusions), where the
call-site tests sample it; and the record owns
the A2A-side re-baseline consequence, the
components posture, and the extension gap above,
with the extensions doc corrected to match the
code.

## Twenty-second slice: A2A extension requiredness (design, 2026-11-25)

Main approved this design as written on
2026-11-25, no changes and no question
outstanding. The twenty-first slice's red team
named this gap
beside its own call site, and the record took it
as named future work: an A2A extension item's
required flag is read in no spelling at all. The
reduction takes only the item's uri into the
extensions set, so a publisher who flips an
extension from optional to required, which in the
A2A spec changes what a client must implement to
use the agent, produces an identical view and no
finding. The gate watches the flags and the
extension set beside them and is blind to the
one bit on the item that carries an obligation.

The design. The reduction reads each extension
item's required flag through the twenty-first
slice's read_bool, so the YAML spellings count
here as everywhere a boolean is read, and records
the required extensions' uris in a second sorted
set on the view, required_extensions, beside the
declared set. The baseline gains the same set as
an additive field with a serde default, so
pre-slice baselines parse with it empty. The
drift comparison folds it into the standing
condition: flags, declared extensions, or
required extensions differing from the baseline
report the one existing major,
a2a-capabilities-changed, whose detail already
names capability flags or declared extensions and
gains the words for this set. No new finding
kind. The declared set keeps recording every
extension's uri, required or not, so the two
sets answer different questions: what is
declared, and what among it is obligatory.

The recorded consequence follows the standing
shape: a baseline recorded before this slice
holds an empty required set, so a card whose
extensions are required reports the one major
until the publisher regenerates, a one-time
re-baseline stated here as the twelfth and
twenty-first slices stated theirs.

Scope lines. The A2A reduction and its baseline
and comparison only; the OpenAPI side has no
analogous per-item obligation bit in what it
reads. An extension item's other fields
(description, params) stay unread, as today.
Nothing here needs David, and no question rides
with this one: the representation follows the
view's own idiom of separate named sets, and the
severity is the standing class's.

### As built (2026-11-25)

As designed. The view and the baseline each gain
the required set beside the declared set, the
baseline's with the serde default the additive
precedent sets, and the comparison folds it into
the one major, whose detail now names all three
channels. Inherited postures the record names
rather than repairs: duplicate extension items
take union semantics, so a card contradicting
itself over one uri's requiredness reduces to the
stricter reading and the contradiction is
invisible, where the same function's skills are
refused on a repeated identifier; and an item
carrying required with no usable uri records
nothing in either set, the module's standing skip
for partial entries. The single finding's
granularity is the design's own choice: the three
channels share one major, so a reader sees that
the capability surface moved, and the baseline
keeps the sets apart for whoever needs which.

### Extension requiredness checkpoint fix pass (2026-11-25)

The three reviews ran before staging: spec
fidelity PASS at zero findings, standards PASS,
the red team clean at critical and major. Two
test assertions were strengthened on their
minors: the reduction test asserts the declared
set by exact vector rather than by count, and the
drift flip test asserts the flip produces exactly
one finding, the designed major, where it
previously asserted only that the kind appears.
The red team's remaining minors are the
inherited postures recorded above.

## Twenty-third slice: conflicting duplicate A2A extensions (design, 2026-11-25)

Main approved this design as written on
2026-11-25, its one question answered with the
narrow answer the record recommends: refuse on
conflicting duplicates only. The twenty-second
slice's as-built recorded the
posture this slice takes up: duplicate extension
items in an A2A card take union semantics, so a
card that contradicts itself over one uri's
requiredness, one item marking it required and
another not, reduces to the stricter reading and
the contradiction is invisible. The same
function's skills take the opposite posture: a
repeated skill identifier refuses the reduction
outright, because an identity collision makes the
baseline ambiguous, and the record named the
inconsistency rather than repairing it in a slice
about something else.

The ambiguity is the harm, and it is narrower
than the repetition. Two items naming one uri
whose required flags agree record identically
under any reading, so nothing is ambiguous and
nothing is hidden that the gate could act on. Two
items whose flags disagree leave the baseline
unable to say which obligation was recorded, and
whichever way a later card resolves the pair, the
comparison against that baseline can pass a real
requiredness change or invent one.

The design. The reduction tracks the extensions
it has seen by uri, with the requiredness each
carried. A repeated uri whose requiredness
agrees with the first sighting reduces exactly as
today. A repeated uri whose requiredness
disagrees refuses the reduction with an error
naming the uri, the same surface a duplicate
skill identifier refuses on, so generate declines
the card and check reports it through the
standing a2a-card-unreadable critical rather than
comparing against a guess. The declared and
required sets themselves do not change.

One question rides with this design, and the
record recommends the narrow answer above:
refuse on conflicting duplicates only. The broad
answer, refusing any repeated uri the way skills
refuse any repeated identifier, buys consistency
with the skills posture and catches an authoring
error the gate is not otherwise charged with
seeing, at the price of refusing cards the gate
has read without harm since the A2A slice landed.
The narrow answer repairs exactly the ambiguity
and nothing else. Main's call.

### As built (2026-11-25)

As designed, in the one file. The extensions loop
tracks each uri's first-sighting requiredness and
refuses on a conflicting repeat with the error in
the skills refusal's voice; pushes now happen on
first sighting only, which leaves the two dedup
calls after the loop unable to remove anything.
They stand as harmless symmetry with the
pre-slice code, recorded here so a future reader
does not hunt for a duplicate path they imply.
One consequence the record states in the open: a
self-contradicting card baselined under the old
union semantics refuses at check with the
critical unreadable after this slice, rather than
comparing against the stricter reading. That is
the design's stated intent, the same treatment a
duplicate skill identifier already received.

### Conflicting duplicates checkpoint fix pass (2026-11-25)

The three reviews ran before staging: spec
fidelity PASS, standards PASS, the red team clean
at critical and major. Their minors converge on
test breadth, closed here: the agreement side now
pins agreement across spellings, a JSON true
beside a spelled yes and a spelled off beside no
flag, the posture that distinguishes judging
agreement on the read flag from comparing raw
values, and asserts the declared set beside the
required one. The sightings map is renamed
seen_extensions so the function does not hold two
maps under one name.

## Twenty-fourth slice: MCP prompt contracts (design, 2026-11-25)

Main approved this design as written on
2026-11-25, its one question answered with the
additive shape the record recommends. The MCP
surface model records its three primitive
kinds at three depths. A tool is recorded whole:
name, description, and its input schema, hashed
into the baseline, so a schema change reports at
major per tool and a description change at minor.
A prompt is recorded as a name in a set, and a
resource as a uri in a set. A prompt's arguments
are its input contract, the exact analog of a
tool's input schema: a prompt that gains a
required argument, or loses one, has changed what
a client must supply to use it, and today that
change produces an identical snapshot and no
finding, while the same change to a tool beside
it is a major. A prompt's description changing is
likewise invisible, where a tool's is a minor.

The design gives prompts the tool channels,
mirrored. The probe's prompt walk records each
prompt as a surface in the snapshot: name,
description, and the arguments array as served.
The baseline records per prompt a name, a
description hash, and an arguments hash, the
ToolBaseline shape. The comparison keeps set
membership on the standing major,
prompt-set-changed, and adds the two per-prompt
channels at the tool severities: an arguments
hash change is a major, prompt-arguments-changed,
naming the prompt; a description change is a
minor, prompt-description-changed. Resources stay
set-only in this slice; their metadata is the
same shape of gap and is named here as the
following slice's candidate rather than folded
in, so each surface's change stays attributable.

One question rides with this design: how the
baseline field lands. The prompts field today is
a list of names; recording contracts in place
changes the element type, and a baseline written
before this slice would fail to parse unless the
reader accepts both shapes. The record recommends
the additive shape the fifteenth slice set as
precedent: the name list stays as it is, still
compared for membership, and a parallel
prompt_contracts list, defaulted empty, carries
the per-prompt hashes. A pre-slice baseline then
parses untouched, gates membership as it always
has, and leaves the contract channels ungated
until the publisher regenerates, a one-time
consequence of the standing shape. The in-place
alternative, a tolerant reader accepting a bare
string or an object per element, keeps one field
and pays for it with a custom deserializer the
record format has never needed. Main's call.

### As built (2026-11-25)

Built as designed on the additive shape Main
approved. The probe's prompt walk records each
prompt as a PromptSurface in the snapshot (name,
description, and the arguments value as served),
sorted by name on both transports beside the name
list; the baseline records a PromptBaseline per
prompt (description hash and arguments hash, the
tool channel's own hash functions) in a parallel
prompt_contracts field, serde-defaulted empty;
the comparison keeps membership on the standing
prompt-set-changed major and adds
prompt-arguments-changed at major and
prompt-description-changed at minor, each naming
the prompt. Resources stay set-only, as designed.

Two representation notes the reviews surfaced and
this record owns. First, the design text says the
parallel field is a "list"; as built it is a
name-keyed map, because the same design paragraph
invokes the ToolBaseline shape and tools are
recorded as exactly such a map. No behavioral
difference; the letter of one word against the
shape the design also names. Second, a prompt
whose arguments member is absent records an empty
array: a server that later sends an explicit
empty array shows no drift. The normalization is
a chosen one, mirroring the tool channel's
standing default for an absent inputSchema.

The reviews ran in three seatings; the first two
seatings' report bodies were lost to runtime
interruptions (their verdict lines agreed: spec
pass, one red-team major), and the third seating
wrote its full report to a file the seat read
firsthand. Final verdicts: spec fidelity PASS
(0 critical, 0 major, 2 minor, the two notes
above), standards PASS (0 critical, 1 major,
3 minor), red team FAIL on one major with the
rest of the surface confirmed sound (0 critical,
1 major, 4 minor). The checkpoint fix pass closed
both majors and two standards minors:

- Duplicate prompt names (red-team major). The
  baseline's map collect keeps the last duplicate's
  hashes while the drift loop's find took the
  first live surface, so an unchanged surface
  serving one name twice with different arguments
  fired the major against itself. The live side
  now builds the same name-keyed map the tool
  channel builds, with the same last-wins collect,
  and the drift test pins the unchanged duplicated
  surface silent.
- Ordering stated three ways (standards major).
  The snapshot field's doc said walk order, the
  stdio walk sorted by name, and the HTTP walk did
  not sort at all. The HTTP walk now sorts prompt
  surfaces by name like the stdio walk, and the
  field doc says sorted by name, so a snapshot no
  longer differs by transport in this field.
- The probe imports PromptSurface with its sibling
  surface types instead of qualifying it at its
  use sites (standards minor), and a baseline test
  now pins prompt contract recording directly,
  hashes and the empty-surfaces case (standards
  minor).

Recorded from the red team's minors, standing as
postures rather than repairs: reordering a
prompt's arguments array fires the major, because
the hash reads the array as served, which the
design's own wording orders; an argument's own
description rides the arguments hash to major,
mirroring property descriptions inside a tool's
input schema; a present but non-array arguments
member is hashed as served, faithfully on both
sides, and the reduction does not name the
malformation; and membership stays the standing
sorted-vec comparison, duplicates included, as
resources have always compared. None of these
can hide a real contract change; the first two
can over-report presentation movement as contract
movement, the same trade the tool channels make.
