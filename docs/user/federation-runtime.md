# Federation runtime: controlled integration environments

Stages C/D provide opt-in durable publication and trust enforcement, **not a released
customer federation product**. The wire namespace is unpublished. Do not enable
this against customer data before the [E and release gates](../reviews/federation-first/README.md)
pass. Native Spaces alpha is not used.

## Enable deliberately

`memory service run` starts no federation tasks by default. An administrator may
set `MEMORY_LAYER_FEDERATION_CONFIG` to an absolute JSON file path. Each service
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
  "token_file": "/run/secrets/memory-gateway-token",
  "receipt_keys": [{
    "generation": 1,
    "first_sequence": 1,
    "public_key": "<independently enrolled gateway audit did:key>"
  }]
}
```

The project/principal must exist locally. Starting the worker neither grants
remote capability nor enrolls a memory. The first Stage E [application
API](federation-workflow.md) now exposes centrally authenticated local grants,
preview/review, retained context and status over this configured runtime.
Review opts the selected stream into future content/state updates using **only
the reviewed evidence references**, with explicit standing consent and retry-safe
submission. A fresh explicit review is needed to change that evidence. The worker
re-resolves each review owner's local policy before processing their queue;
it never borrows its configured principal's role. Web/CLI/TUI enrollment, review
and proposal clients remain E work. Trusted in-process APIs remain available;
do not expose them without the local authentication/policy boundary.

The gateway configuration is separate. Start only
`memory-federation-gateway /absolute/path/gateway.json`, with a dedicated,
pre-provisioned database and a non-owner restricted SQL login. The binary never
runs migrations, the local app, MCP, watchers or model providers. See the
[private topology and grants](../../deploy/federation/README.md).

```json
{
  "space": "00000000-0000-0000-0000-000000000001",
  "authority": "00000000-0000-0000-0000-000000000002",
  "listen": "127.0.0.1:4055",
  "database_url_file": "/run/secrets/gateway-database-url",
  "pds": "https://pds.federation.example.com",
  "repository": "did:web:space.federation.example.com",
  "signing_key": "<enrolled did:key public signing key>",
  "token_file": "/run/secrets/memory-pds-token",
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
      "token_file": "/run/secrets/memory-authority-token"
    },
    {
      "instance": "00000000-0000-0000-0000-000000000003",
      "token_file": "/run/secrets/memory-gateway-token"
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
The [isolation harness](../../tests/federation-isolation/README.md) exercises those
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
[key rollover and recovery](../developer/architecture/federation-security.md).

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

### F1 deterministic-answer rehearsal only

F1 is not enabled by the production gateway configuration. In a disposable debug
rehearsal, an instance configuration may additionally set `expertise_principal`
to a **separate**, internal, project-scoped principal. It must have current project
read/export permissions, the local space's `AnswerExpertise` grant and an explicit
grant for the reviewed collection. Global roles, global permissions and group
identities are rejected. Merely naming a principal does not grant access.

The optional worker answers queued deterministic questions and validates retained
answer delivery in independent loops. It never invokes models or ordinary query
retrieval, and does not yet provide end-user F1 commands. Both loops poll every
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
This remains component-tested rehearsal software, not homelab activation or
customer-release readiness.

Migration 0052 adds origin-private immutable result storage. The result commits
before gateway completion; a new transaction rechecks service authority, reviewed
sources and exact citations before sending. Recovery may recompute deterministic
excerpts, but cannot replace an already stored result with different bytes or
silently substitute newer source versions. No exactly-once provider claim is
made: this path has no provider invocation. Stored results retain 24-hour payloads
and 30-day metadata; holds do not extend read access. The project-scoped cleanup
helper is implemented, but automatic retention maintenance and full process-crash
rehearsal are still pending. Requester service/UI submission wiring also remains
unfinished; adding this option does not create consent, grants or enrollment.

## Observe and recover

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
shared plaintext. Browser parity, actual deployment security review and complete
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
configuration. E still supplies the user journey and concrete tool-free adapter.

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
[full security contract](../developer/architecture/federation-security.md).
Previously persisted `Published` observations are not retroactively certified;
do not treat pre-D state as cryptographic evidence or promote it to a customer
deployment without a reviewed verification/migration procedure.

For synthetic verification, see [the PDS harness](../../tests/federation-pds/README.md)
and [the transaction contract](../developer/architecture/federation-durability.md).
