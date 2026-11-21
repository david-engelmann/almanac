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
protocol revision the server negotiates is learned at handshake
time and recorded, for the reason the next section gives.

## Protocol posture

The probe speaks the stateful Streamable HTTP shape defined by the
2025-06-18 revision and carried into 2025-11-25, because that is
what deployed servers run. The rules that matter to a probe, read
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
the probe proceeds without one. A server speaking the newer
stateless shape, carrying identity per request rather than in a
handshake, is recorded as such in the snapshot. Full support for
that revision is a later addition, named here so its absence is a
decision and not a surprise. The negotiated protocolVersion from
the initialize result goes into the baseline either way, and a
change in it rides the server version drift class in DESIGN.md
section 6, because a server that renegotiates its protocol has
changed its surface.

## Probe sequence

One probe run against one MCP entry proceeds in this order.

1. Resolve the endpoint URL from the catalog entry. No discovery,
   no guessing at well known MCP paths. The catalog is the claim
   and the entry names the surface.
2. POST initialize with the probe's client identity and the newest
   protocol version the probe supports. Record the HTTP status,
   the response media type, the session id header if present and
   the negotiated protocolVersion and serverInfo from the result.
3. POST notifications/initialized, expecting 202 with no body.
4. POST tools/list. Follow pagination cursors until the set is
   complete. A partial tool set is a wrong tool set, and a checker
   that compares against half a surface manufactures drift.
5. POST resources/list and prompts/list the same way, recording
   empty sets as empty rather than absent.
6. If the server issued a session id, send the DELETE courtesy.
7. Assemble the surface snapshot and hand it to the core, which
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
means. An announced server that cannot complete initialize is the
critical handshake failure DESIGN.md section 6 already defines.

## The authenticated path

dusk.fm's server answers an unauthenticated handshake with an
authorization challenge. That is the normal state of the first
customer, so the probe treats 401 as a branch, not a dead end.

The ladder, in order.

1. Open. Initialize completes. The probe runs the full sequence
   and the run is a live pass.
2. Protected, with metadata. Initialize returns 401 with a
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
3. Protected, bare. Initialize returns 401 with no metadata
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

What the probe never does in this milestone. It holds no
credential, runs no authorization flow and presents no token. The
credential aware probe, using a publisher supplied token from the
CI secret store, is the first post launch enhancement DESIGN.md
section 11 already flags. Until then, an authenticated surface
gives the gate its card, its catalog, its baseline and its auth
metadata, and the report says exactly that.

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
