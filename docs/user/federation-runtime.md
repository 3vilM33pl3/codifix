# Federation runtime: opt-in configuration

This guide follows current `main` (`3.0.0`), including controls that an older
installed release may lack. Start with [private federation](https://codifix.dev/docs/federation)
and review participant compatibility before configuring a deployment.

Stages C/D provide opt-in durable publication and trust enforcement, **not a released
customer federation product**. The wire namespace is unpublished. Do not enable
this against customer data before the [E and release gates](https://github.com/3vilM33pl3/codifix/blob/main/docs/reviews/federation-first/README.md)
pass. Native Spaces alpha is not used.

## Enable deliberately

`codifix service run` starts no federation tasks by default. An administrator may
set `CODIFIX_FEDERATION_CONFIG` to an absolute JSON file path. Each service
process runs one configured instance/space. IDs are stable, non-nil UUIDs selected at
enrollment, not regenerated on restart. Use separate PostgreSQL databases for
each instance and the gateway. The general service refuses the gateway role;
use the dedicated executable described below.

An instance configuration has these fields (replace all examples with enrolled
identities and an explicitly configured test deployment):

```json
{
  "role": "instance",
  "space": "00000000-0000-0000-0000-000000000001",
  "authority": "00000000-0000-0000-0000-000000000002",
  "instance": "00000000-0000-0000-0000-000000000003",
  "project": "00000000-0000-0000-0000-000000000004",
  "principal": "00000000-0000-0000-0000-000000000005",
  "gateway": "https://gateway.federation.example.com",
  "token_file": "/run/secrets/codifix-gateway-token",
  "receipt_keys": [{
    "generation": 1,
    "first_sequence": 1,
    "public_key": "<independently enrolled gateway audit did:key>"
  }]
}
```

The project/principal must exist locally. Starting the worker neither grants
remote capability nor enrolls a memory. The first Stage E [application
API](https://codifix.dev/docs/federation/workflow) now exposes centrally authenticated local grants,
preview/review, retained context and status over this configured runtime.
Review opts the selected stream into future content/state updates using **only
the reviewed evidence references**, with explicit standing consent and retry-safe
submission. A fresh explicit review is needed to change that evidence. The worker
re-resolves each review owner's local policy before processing their queue;
it never borrows its configured principal's role. Web/CLI/TUI expose enrollment,
review and proposal controls. Trusted in-process APIs remain available;
do not expose them without the local authentication/policy boundary.

### Multiple projects in one instance (current main)

The existing top-level `project`, `principal` and `expertise_*` fields remain the
primary binding. An optional `projects` array adds explicit local project bindings;
it does not create additional instances or synchronization cursors:

```json
{
  "projects": [{
    "project": "00000000-0000-0000-0000-000000000006",
    "principal": "00000000-0000-0000-0000-000000000007",
    "expertise_principal": "00000000-0000-0000-0000-000000000008",
    "expertise_browser": true,
    "expertise_output_destinations": ["local_human"]
  }]
}
```

Merge this fragment into the instance configuration. Each additional binding has
its own `expertise_callers`, `expertise_output_destinations`,
`expertise_model_destinations` and optional `expertise_model`; omitted fields use
their safe defaults, never inherit another project's policy. At most 256 projects
including the primary are accepted. Project IDs and configured answering service
identities must be unique. Configure current exact-project permissions and local
space grants separately; listing a project here grants no authority.

All bindings share one ordered instance/space receive stream. Publication checks
each originating project's current principal/review owner independently. A single
mailbox worker resolves addressed collections locally to exactly one configured
project and its dedicated answerer. An ambiguous collection ID is denied rather
than answered with another project's knowledge. HTTP paths still select the exact
local project. Separate instances/databases remain the isolation boundary between
knowledge areas.

Cloud MCP callers need an operator-bound identity and output-destination policy
in that project's `expertise_callers`, plus separately reviewed collection consent
for that destination. The `local_human` default does not authorize cloud disclosure.

The gateway configuration is separate. Start only
`codifix-federation-gateway /absolute/path/gateway.json`, with a dedicated,
pre-provisioned database and a non-owner restricted SQL login. The binary never
runs migrations, the local app, MCP, watchers or model providers. See the
[private topology and grants](https://github.com/3vilM33pl3/codifix/blob/main/deploy/federation/README.md).

```json
{
  "space": "00000000-0000-0000-0000-000000000001",
  "authority": "00000000-0000-0000-0000-000000000002",
  "listen": "127.0.0.1:4055",
  "database_url_file": "/run/secrets/gateway-database-url",
  "pds": "https://pds.federation.example.com",
  "repository": "did:web:space.federation.example.com",
  "signing_key": "<enrolled did:key public signing key>",
  "token_file": "/run/secrets/codifix-pds-token",
  "audit_key_file": "/run/secrets/gateway-audit-key",
  "receipt_keys": [{
    "generation": 1,
    "first_sequence": 1,
    "public_key": "<independently enrolled gateway audit did:key>"
  }],
  "policy": { "external_models": [] },
  "credentials": [
    {
      "instance": "00000000-0000-0000-0000-000000000002",
      "token_file": "/run/secrets/codifix-authority-token"
    },
    {
      "instance": "00000000-0000-0000-0000-000000000003",
      "token_file": "/run/secrets/codifix-gateway-token"
    }
  ]
}
```

Gateway bearer credentials identify enrolled **instances**, not human authors.
An authority credential can submit explicit membership records at `/v1/accept`;
merely listing a member token does not grant Pull or Publish. `/v1/pull` requires
current Pull membership and returns a fresh nonce-bound signed page with the
contiguous published prefix, independent publication receipts and current policy.
The client supplies `after`, `limit` (1–32) and a random UUID `nonce`.
PDS credentials must never be distributed to participant instances. The listener
is HTTP and requires a reviewed private-network/TLS termination arrangement;
the outbound clients require HTTPS. Prevent all raw PDS/CAR/firehose bypasses.
The [isolation harness](https://github.com/3vilM33pl3/codifix/blob/main/tests/federation-isolation/README.md) exercises those
controls; setting this file does not itself install a network or TLS edge.

`signing_key` is now mandatory: replace the placeholder with the repository's
canonical compressed `did:key` public signing key, enrolled through a trusted
administration channel together with the repository DID and space. Missing or
malformed keys prevent gateway startup. The public key is not a PDS access token
or private signing key. Do not copy a key from an untrusted record or silently
trust the first key returned by a server. There is no discovery or automatic
key replacement. PDS key changes require an explicitly verified upstream migration.
Gateway receipt keys are independent: append a generation with the exact next
unpublished sequence, distribute trust to receivers and restart with matching
audit secret and replacement tokens. Existing readers/writers are fenced. See
[key rollover and recovery](https://github.com/3vilM33pl3/codifix/blob/main/docs/developer/architecture/federation-security.md).

Token/database URL files must be private (0600 on Unix), 32–4096 bytes; audit keys
are exactly 64 hexadecimal characters representing a 32-byte scalar. Never reuse
a PDS signing key as an audit key. Configuration rejects
unknown fields, invalid IDs and unsafe URL shapes. Endpoints are pinned origins,
not discovered from imported records; redirects and environment proxies are
disabled. Optional `gateway_ca_file` (instance) and `pds_ca_file` (gateway) select
a trusted PEM root instead of built-in roots, retaining hostname/certificate
verification. No insecure TLS option exists.
`disposable_test_mode: true` permits fixture HTTP only in debug builds;
release service builds reject it. No automatic session refresh is implemented:
expired PDS credentials block work until an operator replaces the private token
file, restarts the role and explicitly retries blocked work. Do not skip a blocked
publication to make a later cursor appear healthy.

### F1 private expertise opt-in

F1 stays disabled unless the gateway explicitly configures a compatible
`expertise_fleet`. An instance may separately set `expertise_principal`
to a **separate**, internal, project-scoped principal. It must have current project
read/export permissions, the local space's `AnswerExpertise` grant and an explicit
grant for the reviewed collection. Global roles, global permissions and group
identities are rejected. Merely naming a principal does not grant access.

Provision that identity through the explicitly confirmed
[`codifix auth expertise-answerer` installation command](https://github.com/3vilM33pl3/codifix/blob/main/docs/user/cli/auth.md#internal-expertise-answerer-installation-operator),
not fixture SQL, a service token or the local-owner administrator. The command
creates only local read/export permissions and protected audit. Remote
membership, local space and collection grants remain separate; no worker or
model is enabled automatically.

The optional worker answers queued questions and validates retained answer
delivery in independent loops. Ordinary query retrieval is never used; model
synthesis is separately disabled unless explicitly configured below. Both loops poll every
two seconds and stop with the federation worker lifecycle. Answer work has a
12-second cycle limit within the gateway's 30-second lease, with renewal at ten
seconds if still active. Failed/expired attempts remain recoverable through the
gateway, subject to the original question deadline. Each read needs
a fresh gateway challenge, valid for at most 15 seconds. The origin locks current
permissions and source eligibility while checking the exact stored excerpt and
authorizing its release. Offline origins therefore cannot authorize new delivery;
status reads and exact submission retries never return cached answer bodies.

Approvals contain identifiers/digests, not repeated private payloads. They are
single-use and bound to a live addressed request; gateway restart, expiry and
replay cannot create reusable access. The gateway commit authorizes release, not
proof of physical delivery. An already-authorized in-flight transfer cannot be
recalled, and the two databases do not form one atomic transaction. Existing
24-hour payload expiry still applies; holds do not extend ordinary read access.
This does not activate an existing installation or establish customer readiness.
Use TLS and leave `disposable_test_mode` false for any later approved internal
installation. Cleartext fixture mode remains prohibited in release builds.

Migration 0052 adds origin-private immutable result storage. The result commits
before gateway completion; a new transaction rechecks service authority, reviewed
sources and exact citations before sending. Recovery may recompute deterministic
excerpts, but cannot replace an already stored result with different bytes or
silently substitute newer source versions. No exactly-once provider claim is
made: uncertain provider attempts cannot be automatically reinvoked. Stored
results retain 24-hour payloads and 30-day metadata; holds do not extend read
access. Owning runtimes perform retention maintenance every minute. Adding this
option does not create consent, grants or enrollment.

### Browser discovery and questions

`expertise_browser: true` separately enables project/space capability listing,
question preparation/submission and result reads. A current browser session,
project read/export permission and explicit local `AskExpertise` grant are
required; mutations additionally require CSRF and Origin headers. Read-only mode
currently denies these remote operations. CLI/TUI and MCP require explicit
operator bindings described below; ordinary bearer tokens remain denied.

Prepare accepts `request_id`, `capability` (`publisher` and `id` UUIDs), the
question (at most 4 KiB UTF-8), and an optional explicitly approved synthesis
`model_destination`. Synchronize first so the capability is in verified
retained history. It returns the exact `QuestionReview`, including authenticated
identity, instance, revision, digest, destination and 120-second expiry. Inspect
all fields before confirming. Preparation itself neither persists consent nor
sends the question. Submit accepts `{ "review": <exact returned review>,
"confirmed_digest": <review.digest> }`; consent commits before network I/O and
current authority is checked again. Network failure may leave consent committed:
retry that same review/ID before expiry, never silently replace it with a new
question. Stale reviews require deliberate preparation and review again.

Submission returns gateway status metadata only, not an answer or proof of
completed execution. `GET capabilities` lists advertisements permitted for the
operator-bound caller's output and answer modes with a limit of 1–32.
`GET capabilities/{publisher}/{record}` returns one complete, currently permitted
advertisement using the same checks. Follow `next_after` using both
`after_publisher` and `after_record`; a filtered empty page can still have a cursor.

`GET questions/{id}` returns status and, when authorized, the retained answer.
Only the original consenting principal can read it, including when another
principal is an administrator. The 120-second execution deadline does not remove
a completed result: retained reads remain possible within the 24-hour payload
window. Holds preserve storage, not access or permission to restart an expired
question. Each answer read requires fresh origin approval and current local and
gateway authorization; an unavailable origin cannot release a cached answer.
Reads can wait for that approval, and all responses are no-store.

Web/CLI/TUI/MCP support explicit review and private question/result operations.
See the experimental
[OpenAPI contract](https://github.com/3vilM33pl3/codifix/blob/main/docs/api/openapi.yaml) for paths and request shapes.

### Collection review and withdrawal

The same explicit browser opt-in exposes `POST collections/draft`,
`POST collections/activate` and `POST collections/withdraw`. These require
current project read, curate and export permissions, browser CSRF protection and
an explicit local space grant. Activation additionally requires local and observed
gateway `Publish` and `AnswerExpertise` capabilities; withdrawal requires `Publish`.
They do not enroll instances or create the answering service identity's separate
collection grant. Optional synthesis requires explicit reviewed model destinations
and the separate operator configuration described below.

The same operations accept an explicitly bound local-human service token.
`expertise_callers` in the instance's operator-owned federation JSON is an array
of `{ "principal": "UUID", "token": "TOKEN-ID-UUID", "output_destination":
"local_human" }`. IDs are identifiers, **not secret token values**. Bind the exact
token and principal; replacement tokens require new explicit bindings. At most
64 distinct token bindings are accepted. Current token expiry/revocation and
project permissions are rechecked during operations.

For assistant access, use a separate token and an opaque destination ID such as
`company_assistant`. The gateway's `policy.external_models`, the origin's
`expertise_output_destinations`, and the reviewed collection must all permit that
ID. Instance output destinations default to `["local_human"]`; no URL, credential
or executable endpoint belongs in an advertisement. Changing operator policy
requires restarting/reloading the explicitly configured runtime; origin workers
recheck it before answering and before each release.

MCP offers `codifix_expertise_discover`, `codifix_expertise_prepare`,
`codifix_expertise_ask` and `codifix_expertise_result`. The bridge marks every call
as onward model disclosure, and the service denies local-human credentials on
this path. Submission additionally requires `[mcp] read_only = false` and exact
review confirmation. The operator, not the assistant, must verify that the token
is installed only in the intended destination. The marker can restrict access
but cannot authorize a destination. Ordinary `codifix_query` is unchanged.

The dedicated gateway JSON accepts an optional `expertise_fleet` array of
`["INSTANCE-UUID", 3]` pairs for every enrolled credential, including authority.
It validates fleet/history compatibility before enabling expertise; absence
keeps expertise disabled. It does not grant membership or collection access.

### Watched-agent addressed questions

Addressed ATProto questions are a separate opt-in from the human F1 mailbox.
The gateway requires both the complete `expertise_fleet` above and a complete
`addressed_expertise_fleet` containing `["INSTANCE-UUID", 1]` for **every**
configured credential, including the authority. The `1` is the addressed-record
schema, not the publication schema. Enable this only after all participants and
the private PDS proxy have been upgraded; an omitted or partial fleet fails
closed. The proxy must allow the gateway's fenced `deleteRecord` request so
expired addressed records can be removed after verified signed-key absence.
Participants still receive no PDS credentials or raw repository access.

For each watched Codex or Claude installation, create a separate project-scoped
service token and bind its exact principal and token **IDs** in the owning
instance's `expertise_callers`. Set a fixed non-local
`output_destination` (for example `codex_openai` or `claude_anthropic`) and an
`agent_ask` object containing a positive `revision`, `daily_limit` of 1–20 and
an `allowed_capabilities` list of exact publisher/collection record keys. A
binding is not collection consent: the reviewed collection must separately
allow that requester instance and output destination, and current memberships
must include the expertise capabilities. Use distinct tokens/destinations for
Codex and Claude; never bind an assistant to `local_human`.

For a split schema-owner/runtime-role installation, the runtime role needs
`SELECT, INSERT, UPDATE` on `federation_agent_expertise_intents` and
`SELECT, INSERT, UPDATE` on `federation_agent_expertise_daily`. The submit path
uses `SELECT ... FOR UPDATE` on the intent row, which PostgreSQL denies without
`UPDATE` privilege even though the handler does not change that row. Grant
only these new tables; no grant on gateway addressed records is implied.

The bundled `codifix-federation-expertise` skill teaches watched agents to
discover, submit one bounded question and read its cited result. Installing the
skill does not enable the service, expand collection consent or permit a paid
model call. Agent answers are deterministic only. The gateway stores private
addressed bodies for at most 24 hours of participant access; a hold may retain
bytes for audit, and backups or previously delivered copies cannot be recalled.
For an approved TLS installation, expose only the exact `/v1/expertise` path
through the fixed-upstream gateway edge in addition to the existing routes;
never expose the PDS, raw repository or database interfaces. Customer release
gates remain separate from this default-off internal-use configuration.
The owning gateway runtime cleans expired mailbox payloads/receipts every minute;
instance runtimes clean project-private question/result payloads on the same
interval. Holds are respected; reads enforce TTL independently of cleanup.

Draft accepts a `CollectionBlueprint`: collection UUID, reviewed name, description
and topics, explicit requesting instance IDs, `modes: ["deterministic"]` by default,
empty `model_destinations` by default, explicitly reviewed `output_destinations` (default
`["local_human"]`, limited by operator policy) and selected canonical
IDs with explicitly selected evidence. The returned draft displays the exact
current memory versions, evidence, authorship, disclosure policy and digest.
Inspect every field: preparing it creates neither standing consent nor publication.

Activate accepts `{ "draft": <exact returned draft>, "confirmed_digest":
<draft.review_digest> }`. Changed source versions or altered review bytes fail.
Consent, its audit and the advertisement outbox intent commit together. A `202`
with `submission: "queued"` is local acceptance only, not gateway acceptance or
PDS publication. Retry the exact request after an ambiguous response; do not
silently generate a new review. Only advertisement metadata enters the outbox;
the collection snapshot stays in the origin database.

Withdraw accepts `request_id`, `collection`, `expected_revision` and
`confirmed: true`. It disables local standing consent while atomically queuing
the withdrawal advertisement. Exact retries retain the same intent. It cannot
recall already delivered or already authorized in-flight bytes. All responses
are no-store. Browser service-grant inspection and reviewed changes are available
after collection inspection. The disposable real-service browser rehearsal covers
these controls; it does not activate an existing installation.

`GET collections/{collection}/grants?principal=<UUID>` reads a stored local grant
(`enabled: null` means no matching grant). `POST` to the same path accepts
`principal`, `enabled` and `confirmed: true`. Current browser read/AuthManage
permissions and a local space grant are required; mutations require CSRF.
Enabling requires an active internal target with no global/group authority and
current project read/export access. Revocation remains possible for a disabled
target. These operations do not configure workers or confer space capabilities.
After an uncertain change, inspect and review current policy again; grants are not
replayable publication intents. All responses are no-store.

`GET collections?limit=32` inspects local collection metadata, including withdrawn
heads. Follow `next_after` with `after=<collection UUID>`; limits are 1–32 and
pages use UUID ordering, not publication ordering. Pages are not a frozen snapshot.
`GET collections/{collection}` returns the exact stored head snapshot,
advertisement, reviewer and request ID. Both reads use the same conservative
browser, current read/curate/export and local-grant policy; read-only mode remains
denied. They make no gateway request.

These are local review tools, not retrieval or answer authorization. `active`
means stored standing consent only: sources may since have changed, permissions
may have been revoked, and publication may still be queued. Detail preserves the
reviewed source bytes even after a newer source version exists or the collection
was withdrawn. Do not use it to answer remote questions. Prepare and activate a
new explicit review to change the collection.

## Observe and recover

### Optional F1 synthesis

Deterministic excerpts remain the default. Synthesis is a separate tool-free
path; it never uses ordinary query context, source paths or the full-content LLM
audit. Four permissions must intersect: gateway `external_models`, the
collection's reviewed `model_destinations`, the requester's
`expertise_model_destinations`, and the responder's `expertise_model`.
Onward output consent remains separate, including for deterministic MCP answers.

The instance JSON's `expertise_model` is absent by default. An example fragment
for a fake-provider rehearsal is:

```json
{"expertise_model":{"provider":"ollama","model":"f1-fake","daily_calls":20,"daily_tokens":800000,"max_output_tokens":800}}
```

This is not a command to start a model or activate a deployment. The automated
rehearsal uses only a temporary loopback fake provider. The exact provider/model
must match the service's existing LLM settings; configure a nonempty stable
`cluster.service_id` for the attempt journal even when clustering is disabled.
Missing credentials or mismatched model settings fail without substitution.
Advertisements contain provider IDs, never endpoint URLs.

The gateway reserves one call and a conservative 40,000-token allowance before
invocation. Limits are at most 20 calls and 800,000 reserved tokens per responder
per UTC day; output is capped at 800 tokens. Successful calls do not refund the
conservative reservation; unknown usage is never credited back. The serialized
request body plus output-token allowance must fit that reservation. An
eight-second provider timeout stays within the origin execution lease; the
question deadline remains 120 seconds. Redirects are forbidden and provider
responses are bounded at 64 KiB. Environment proxies are disabled for this
restricted path. No tools or fallback providers are offered.
Transport ambiguity becomes `uncertain`, not an automatic retry. Another question
can incur repeat billing.

Structured responses reference unique one-based indexes into selected reviewed
citations. Codifix reconstructs attribution from those exact citations; invented
indexes and malformed answers produce `failed`. This checks provenance, not
factual correctness or every real-model prompt-injection behavior. Operational
journals retain identifiers, fingerprints, reservations and fixed outcome
reasons—not prompts, source contents or provider error bodies.

The storage APIs expose separate `DeliveryStatus` observations:

| Observation | Meaning |
| --- | --- |
| Queued | Local intent committed; gateway has not accepted it |
| Accepted | Gateway committed authorization, reducer result, order and PDS work |
| Published | Verified exact URI/CID/signed inclusion and independent receipt committed with the published prefix |
| Replicated | One named receiver verified receipts and committed them with inbox, projection and cursor |

`intent_status`, `GatewayStore::status` and `ReplicaStore::status` report their
own observation; none implies every participant has received the record. Frozen
`federation_source_changes` with `prepared = false` are also queued work, awaiting
conversion to record envelopes. Operational SQL inspection must be scoped to the
administrator's configured space/instance/project and avoid dumping payloads:

- Source: `federation_source_outbox` records `attempts`, `available_at`, `blocked`,
  `last_error` and acceptance receipt. `retry_source_work` rechecks local authority.
- Gateway: `federation_publish_outbox` records lease, attempt and blocked state;
  `GatewayStore::retry_blocked` restarts the same immutable head after remediation.
- Receiver: `federation_replicas` records `sync_failures`, `retry_at`, `sync_error`
  and its applied cursor. Worker warnings are sanitized; they do not include tokens.

Local PostgreSQL work stays usable while the gateway/PDS/peer is unavailable.
Receiver receipts and inbox/projection/cursor commits are atomic. A gap or corrupt batch
does not advance the cursor. An authorized recovery path can call
`InstanceSync::resynchronize` to fetch a complete paginated history and atomically
rebuild the projection, but it cannot truncate or fork the known prefix. This
low-level API requires the caller to recheck local read permission, just like
`catch_up` and snapshot reads; the normal `cycle` does this itself.

Reference replay is O(history), with a 16 MiB full-resync limit and 32 records per
page. Backoff and source/PDS retry budgets are durable; exhausted or invalid work
needs intervention. This is not a scalable checkpoint implementation or a complete
disaster-recovery system. Revocation denies later gateway access/publication but
does not erase offline copies. Administrators and admitted recipients can read
shared plaintext. Actual deployment security review and complete
production backup recovery remain gates; the reducer's Lean proofs do not prove
these runtime or operational guarantees.

## Use and retain shared data deliberately

Imports stay separate from automatic local search, curation and model pipelines.
The guarded `InstanceSync::retained_context` API requires current local read
permission and a space grant; it excludes current tombstones/archived streams and
keeps reviewed evidence and origin/author/version attribution as untrusted data.
`answer_shared` additionally intersects local export permission, configured local
provider IDs, every selected record's frozen publication policy, and fresh signed
gateway policy/Pull access. All provider sets default to deny; offline/revoked
calls fail before invoking a provider. No commercial provider is enabled by this
configuration. Shared-context forwarding is distinct from the optional F1
tool-free synthesis workflow above.

An authority token may POST `{"action":"hold","reason":"<owner decision>"}`,
`release_hold`, or `request_erasure` to `/v1/retention`. These append operational
decisions. `physical_erasure_authorized` is always false: releasing a hold does
not approve deleting protocol history, pending work, backups or other people's
copies. Immutable guards protect retained records, receipts, reviews and intents.
Actual legal retention periods and whole-space decommission require a separately
approved policy/procedure. Revocation cannot recall already delivered bytes;
already-authorized in-flight operations may finish.

## Repository trust check and its limits

Before PDS work is acknowledged, the gateway obtains a full CAR from the fixed
PDS `getRepo` endpoint and checks SHA-256 block CIDs, the canonical v3 commit,
enrolled DID, low-S secp256k1/P-256 signature, complete canonical MST and the exact
publication key/bytes. No URLs are derived from DIDs or record contents. Matching
JSON alone, invalid signatures and oversized/incomplete proofs cannot advance
the published state. Bad proofs persist as invalid-receipt failures and require
operator remediation; there is no unsigned fallback or automatic overwrite.

This bounded implementation accepts one-root full CARs up to 16 MiB, 32,768
physical blocks, 1 MiB per framed block, 4 KiB commit/header, 8,192 records/nodes
and 64 tree edges. All blocks must have canonical DAG-CBOR/SHA-256 CIDs. Duplicate
blocks and arbitrary block order are supported; incremental CARs, blobs and
native Spaces are not this verification contract. Verification/downloads are
serialized per publisher and CPU work runs outside the async I/O executor.
Full snapshot verification costs O(repository) per publication and is not yet
the scalable production synchronization path. Hitting a limit blocks work; do
not raise limits or bypass verification without assessment.

The proof establishes inclusion in a signed snapshot, not factual correctness,
current membership, history freshness or a transferable human-review receipt.
The five-minute future-revision tolerance is a clock sanity check, not replay
defense; application order still comes from the transactional gateway sequence.
Membership/revocation checks remain in the gateway domain. Receivers now verify
independent audit receipts, their chain, key windows and fresh signed pages.
Retained receipts are re-verified on offline snapshot reads. See the
[full security contract](https://github.com/3vilM33pl3/codifix/blob/main/docs/developer/architecture/federation-security.md).
Previously persisted `Published` observations are not retroactively certified;
do not treat pre-D state as cryptographic evidence or promote it to a customer
deployment without a reviewed verification/migration procedure.

For synthetic verification, see [the PDS harness](https://github.com/3vilM33pl3/codifix/blob/main/tests/federation-pds/README.md)
and [the transaction contract](https://github.com/3vilM33pl3/codifix/blob/main/docs/developer/architecture/federation-durability.md).
