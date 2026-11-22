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
