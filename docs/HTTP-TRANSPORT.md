# Streamable HTTP transport and the authenticated path

Milestone 1 design note for almanac. This note is a child of
docs/DESIGN.md and fixes how the CLI crate probes a remote MCP
server over Streamable HTTP, and how the check behaves when that
server is protected by OAuth. DESIGN.md sections 3 and 6 state the
requirements. This note states the mechanism. Where the two
disagree, DESIGN.md wins and this note gets corrected.

## The reference surface

The design target is dusk.fm's production shape, on record from
David's September 2026 provider survey. One remote MCP server over
Streamable HTTP with OAuth 2.0, built once and consumed by ChatGPT,
Claude, the Gemini tooling and Copilot Studio, with one OAuth
client registered per platform and least privilege scopes. Dawn,
the booking and discovery layer over dusk.fm, rides on that server.

Two things about that surface are deliberately not assumed here.
The endpoint path and the tool list are learned from the catalog
entry and the probe, never hardcoded. A checker that assumes the
first customer's paths has stopped being a checker. And the
protocol revision the server negotiates is learned at the entry
point and recorded, for the reason the next section gives.

## Protocol posture

Re-baselined 2026-11-23 to the current core revision, 2026-07-28,
which is frozen until the next revision targeted 2027-03-30. The
probe speaks that revision first. It is stateless, with no
initialize handshake, no sessions and no Mcp-Session-Id, and
server/discover is the mandatory entry point every server
implements. Every request
carries a _meta block naming io.modelcontextprotocol/protocolVersion
with the revision, the client identity and the client
capabilities, because identity travels per request now, not in a
handshake. The discover result is read for the same fields the
initialize result carried, serverInfo and protocolVersion, and a
result that omits the version is recorded under the revision the
probe offered.

Deployed servers still run the earlier revisions, so the probe
keeps the stateful ladder as its fallback. A server that does not
know server/discover, answering it method not found or with
nothing usable, is walked through the initialize handshake of the
2025-06-18 revision, carried into 2025-11-25, exactly as this note
originally specified. The rules that matter to that ladder, read
from the specification text.

- One MCP endpoint per server, supporting POST and GET.
- The client sends JSON-RPC messages by POST and offers both
  application/json and text/event-stream in the Accept header,
  because the server chooses per request which one answers.
- A server may assign a session id in the Mcp-Session-Id response
  header on the initialize result. When it does, the client sends
  that header on every later request.
- After initialization the client sends the MCP-Protocol-Version
  header with the negotiated version on every request. A request
  without it is read by the server as version 2025-03-26, and an
  unsupported version earns a 400.
- The GET stream exists for server initiated messages. A probe
  sends no GET. It has nothing to receive, and opening a stream it
  will abandon mid check makes the probe a rude client.
- Session termination by DELETE is a courtesy the probe performs
  when it holds a session id, on a best effort basis. A failed
  DELETE changes nothing in the result.

Tolerance in the other direction. A server that answers initialize
without a session id is running stateless or near stateless, and
the probe proceeds without one. The snapshot records which shape
the surface took, stateless under the current revision or the
handshake ladder behind it, and the protocolVersion from the
discover or initialize result goes into the baseline either way. A
change in it rides the server version drift class in DESIGN.md
section 6, because a server that renegotiates its protocol has
changed its surface. A server migrating between the two shapes
between generation and check shows up there too, which is the
gate working as intended, not noise.

## Probe sequence

One probe run against one MCP entry proceeds in this order.

1. Resolve the endpoint URL from the catalog entry. No guessing
   at well known MCP paths. The catalog is the claim and the entry
   names the surface. (server/discover below is the protocol's
   own entry method on that endpoint, not a path guess.)
2. POST server/discover with the _meta block. Record the HTTP
   status and the protocolVersion and serverInfo from the result.
   On a result, walk the lists statelessly, each request carrying
   the _meta block, no session, no version header, no initialized
   notification, no DELETE.
3. When discover is answered method not found, in the JSON-RPC
   error or as an HTTP 404 or 405, the server predates the
   revision, so fall back to the earlier ladder. Any other
   discover failure, a 500, a different error, a result that is
   not an object or names no server, a transport failure, is a
   probe failure at the server/discover step and the ladder is
   not attempted, because a broken current revision surface must
   not hide behind a healthy legacy one. POST initialize with the probe's client
   identity and the newest of the earlier protocol versions the
   probe supports. Record the HTTP status, the response media
   type, the session id header if present and the negotiated
   protocolVersion and serverInfo from the result. Then POST
   notifications/initialized, expecting 202 with no body. As built
   in milestone one, a 401 answer to the notification routes the
   run to the metadata ladder below, because the surface is
   protected after all. Every other answer is ignored, because a
   refused notification on a working surface is the server's
   quirk, not drift.
4. POST tools/list. Follow pagination cursors until the set is
   complete. A partial tool set is a wrong tool set, and a checker
   that compares against half a surface manufactures drift.
5. POST resources/list, prompts/list and resources/templates/list
   the same way, recording empty sets as empty rather than absent.
6. POST resources/read once per announced concrete resource,
   recording a contents hash per uri the read answers for. A
   failed read records nothing for its uri and fails nothing.
7. If the server issued a session id, send the DELETE courtesy.
8. Assemble the surface snapshot and hand it to the core, which
   owns every comparison and every hash.

Response handling. A POST answer arrives as one JSON object or as
an SSE stream carrying notifications and then the response. The
probe parses both, matches responses to requests by JSON-RPC id,
and takes the response object as the result. Progress
notifications inside a stream are read and discarded. Requests are
sent one at a time, never batched, so a failure names its step.
Every request carries a timeout, thirty seconds by default, and a
response size cap, sixteen megabytes by default, above which the
probe stops reading and reports the surface as unprobeable at that
step rather than swallowing an unbounded stream.

Failure at any step is data, not an exception. The snapshot
records which step failed and how, an HTTP status, a JSON-RPC
error, a timeout, a malformed body. The core decides what that
means. An announced server that cannot complete its entry point,
discover under the current revision or the initialize handshake
behind it, is the critical handshake failure DESIGN.md section 6
already defines.

Resolution, as built in milestone one (2026-11-21). Step 1 has a
tension inside the design record, because the catalog entry for an
MCP server names its server card rather than the endpoint itself.
The resolution is a chain of the catalog's own references. The
card the generator writes carries the endpoint it probed, and a
check with no target named reads the card the entry names and
takes the endpoint from it. A card without an endpoint, or one
the checker cannot read on the site, resolves nothing, and the
run falls back to repo mode rather than guessing. A target the
operator names always wins over the resolved one.

## The authenticated path

dusk.fm's server answers an unauthenticated handshake with an
authorization challenge. That is the normal state of the first
customer, so the probe treats 401 as a branch, not a dead end.

The ladder, in order.

1. Open. The entry request completes, discover under the current
   revision or initialize behind it. The probe runs the full
   sequence and the run is a live pass.
2. Protected, with metadata. The entry request returns 401 with a
   WWW-Authenticate header naming a resource_metadata URL, or the
   metadata is found at the well known path
   /.well-known/oauth-protected-resource under the server's
   origin. The probe fetches the protected resource metadata, RFC
   9728, reads the authorization_servers list, and fetches each
   authorization server's metadata, RFC 8414 at
   /.well-known/oauth-authorization-server or the OpenID Connect
   discovery document where the issuer names one. The run then
   degrades to declared evidence, the server card against the
   catalog against the baseline, and the report labels it declared
   evidence only, in those words. It never prints live pass for a
   surface it could not touch.
3. Protected, bare. The entry request returns 401 with no metadata
   reachable. The degradation is the same, and the snapshot records
   the auth surface as protected without published metadata, which
   is itself a fact the drift model can watch. A server that gains
   or loses its metadata has changed its auth surface.

What gets hashed. The protected resource metadata and each
authorization server metadata document are artifacts with bytes.
Their canonical hashes go into the baseline at generation time
beside the card hash, and the check compares them like any other
declared artifact. The fields the comparison reads individually,
because a hash alone cannot say what moved, are the issuer, the
authorization and token endpoints, the jwks URI, the scopes the
resource names, and the resource value itself.

Drift on the auth surface. An issuer change, an endpoint change or
a scope set change is the authentication change class, major, from
DESIGN.md section 6, now measured against documents instead of
inferred from a failed handshake. A surface that was open and now
demands OAuth, or the reverse, is the same class. Metadata that
vanishes while the 401 persists is major. A metadata document
that merely reorders its keys is not drift, because the comparison
runs over canonical bytes, the same discipline as the catalog.

Redirects and origins. OAuth metadata legitimately lives on a
different origin from the resource, so metadata fetches may cross
origins and the snapshot records where each document actually came
from. The MCP endpoint itself is different. The probe posts to the
URL the catalog names. A redirect answer from that URL is recorded
in the snapshot with its target, and the probe does not silently
follow a cross origin redirect of the endpoint, because an
announced endpoint that forwards agents elsewhere is a surface
change the publisher should see named, not a detail to absorb.
Metadata fetches are the opposite case. They follow up to three
redirects, because a metadata document behind a redirect would
otherwise read as absent, and absence is a fact the drift model
acts on.

What the probe never does. It runs no authorization flow:
no client registration, no code exchange, no refresh. Since
the credential aware probe landed, it presents a publisher
supplied token when one is provided through the environment
(the CI secret store's shape), and only then: the walk runs
unauthenticated first, a challenge retries it once with the
token, and the header rides only on requests to the MCP
endpoint itself, never on the metadata fetches, which may
live on another origin. Without a token, an authenticated
surface gives the gate its card, its catalog, its baseline
and its auth metadata, and the report says exactly that.

## Where the code lives

The transport lives in the CLI crate, which owns network code by
the split DESIGN.md section 8 fixes. It produces one value, a
surface snapshot holding the negotiated version, the serverInfo,
the tool, resource and prompt sets with their schema and
description hashes, the auth classification, the metadata hashes
and the step failure record where one exists. The core accepts a
snapshot the same way it accepts the stdio probe's snapshot, and
every comparison, severity and exit decision stays in the core,
testable without a network. The HTTP client is a small blocking
client, ureq with rustls, named here as the choice. No async
runtime enters the build for the sake of a handful of sequential
requests.

## Test plan for the milestone

- A local HTTP fixture serving telorama's seven tool surface over
  Streamable HTTP, built as a test double, so the transport is
  exercised end to end without dusk.fm. The planted breaks from the
  Phase 1 test rerun over HTTP against it, rename, added tool,
  schema change, and the clean run must exit 0 against it first.
- An auth fixture answering initialize with 401 and serving
  protected resource and authorization server metadata. Breaks
  planted there are a removed scope, a changed issuer and metadata
  that disappears, each expected at major, plus the clean degraded
  run, which must report declared evidence only and exit on the
  card comparison alone.
- The source level rename test owed from the Phase 1 gate lands
  with this milestone's work, a rebuilt server with a renamed tool,
  over stdio first and then over the HTTP fixture.
- The first live probe of dusk.fm itself happens only when David
  points the tool at it. Its negotiated version, its metadata
  placement and its card are learned then and recorded, not
  assumed in this note.
