# Federation application API — first Stage E increment

This experimental local API connects the application to the C/D runtime. It is
**not a completed two-organization product or an enrollment UI**. Enable only in
controlled integration environments using the [runtime configuration](federation-runtime.md).
Each process currently binds one local project to one explicitly enrolled
instance/space. Remote identities, credentials, TLS roots and receipt keys remain
administrator configuration; requests cannot replace them.

Use the local service's existing authentication: a scoped service token, or a
browser session with Origin and CSRF for POST. Even single-user retained reads
require an authenticated principal, not the anonymous compatibility reader.
All these responses, including failures, use `Cache-Control: no-store`.

## API surface

The common prefix is `/v1/projects/{slug}/federation/{space}`. The [OpenAPI
specification](../api/openapi.yaml) documents bodies and errors; Rust contracts
are exported by `mem_service::federation::workflow`.

| Method and suffix | Operation | Local requirements |
| --- | --- | --- |
| GET (prefix only) | Retained membership and synchronization observations | `memory_read`, explicit Pull |
| POST `/grants` | Set/disable a local principal's grant | `auth_manage`, `memory_read`, writable service |
| GET `/memories/{canonical}/preview` | Exact current source content selection | `bundle_export`, `memory_read`, explicit Pull |
| POST `/reviews` | Persist review and standing source-update consent | `memory_curate`, `bundle_export`, `memory_read`, explicit Pull/Publish, writable service |
| POST `/context` | Read selected retained shared streams and citations | `memory_read`, explicit Pull |

Every permission is resolved for the path's local project. A role on another
project does not count. The server checks that the configured space belongs to
that project. Local grants do not create gateway membership; even a local admin
does not become the remote authority. Publication also needs observed gateway
Publish membership, and the gateway rechecks current membership when accepting
the queued intent. A successful local review is never proof of remote acceptance.

Local grants take `principal`, `capabilities` (unique `pull`, `publish`,
`manage_membership` values), and `enabled`. The active target principal must
already exist locally. Changes and the actor's local auth audit event commit
together. Granting a capability does not add missing local application permissions.
The existing operator preset can review; the writer preset alone cannot curate.

## Review a publication

1. Inspect `/memories/{canonical}/preview`. It returns `space`, `stream`,
   `selection` and `requires_future_update_consent: true`. This response contains
   the current memory content/version, **not** raw captures or expanded sources.
2. Select each evidence excerpt explicitly. An evidence snapshot contains only
   `space`, `stream`, `public_source_label`, `selected_text`, and `observed_at`.
   Do not copy private paths, sessions, credentials or unselected material into
   the label/text. These fields are the bytes you are approving for recipients.
3. Compute SHA-256 over the snapshot's RFC 8785 canonical JSON (Rust:
   `CanonicalJsonCodec::digest`). Confidence in the source preview is integer
   parts-per-million; preserve the entire `selection` unchanged.
4. POST `/reviews` with this body shape:

```json
{
  "request_id": "<new non-nil UUID; retain it for retries>",
  "canonical": "<canonical UUID from the preview stream>",
  "expected": "<the complete selection object, not a string>",
  "evidence": [
    {
      "snapshot": "<the complete reviewed snapshot object, not a string>",
      "snapshot_digest": { "algorithm": "sha256", "hex": "<64 hexadecimal digits>" }
    }
  ],
  "confirm_future_updates": true
}
```

The placeholders above explain the shape; substitute actual JSON objects before
sending. The server stamps reviewer identity and review time. It rejects client
reviewer/publisher/access fields, changed evidence hashes, wrong space/stream,
and source previews that no longer match the current local version. Source
comparison, immutable review, subscription, deferred source snapshot and
idempotency result commit in one transaction (migration 0037).

**Standing consent matters:** this opts the selected origin stream into future
content/state changes, using only the reviewed evidence references. It is not a
one-shot publication endpoint. A new review is required to change evidence.
Disabling the local Publish grant pauses transmission; it does not delete the
subscription or queued history. Re-enabling it can resume that backlog. A
per-stream stop/edit-consent interface remains later E work.

The response is HTTP 202 with `request_id`, `review_id`, `submission: "queued"`.
Keep the request ID and exact body after an uncertain reply. Identical concurrent
retries return the same review ID without another evidence record/source capture.
An ID reused with different content returns 409. Current permissions still apply
to retries. A fresh request after a source change needs a fresh preview and review.

Request limits: 128 KiB, at most 16 evidence snapshots, and at most 48 KiB
canonical JSON for the source selection or each evidence item (leaving room for
the adapter's envelope limit). Raw JSON field injection fails validation.

The background worker resolves the configured principal for ordinary pull work.
It also visits one additional review owner per tick in UUID keyset order,
re-resolving that owner's current local permissions and grant. It never publishes
their work using the configured principal's role. Durable outbox retries remain
bounded; rejected/blocked work still needs explicit operator remediation.

## Read and cite retained shared knowledge

POST `/context` with `streams`, an array of `{ "origin": "<instance UUID>",
"canonical": "<canonical UUID>" }`. Select 1–16 known streams. There is no
cross-space/global search or arbitrary URL resolution. Archived, tombstoned and
unknown streams fail closed. Duplicate selections collapse to one citation.

The response includes:

- `space`, `authority`, `as_of_sequence`: the verified retained prefix;
- `records` and `receipts`: selected records, reviewed evidence dependencies,
  signed provenance and each record's frozen external-model policy;
- `citations`: stream, publication record key (including publisher), shared
  version, original source version, author/time, locator, digest and evidence keys;
- `untrusted: true`, `retained_copy: true`, and
  `external_forwarding_requires_fresh_policy: true`.

For approved proposals, the citation retains the proposal author's attribution
and identifies the origin owner's resolution as the publication record. Signatures
prove origin/integrity, not factual correctness. Imported instructions remain
quoted data. This API invokes no model and inserts nothing into local search,
curation or embedding pipelines. Receipt policy alone is not permission to send
content to a provider; the separate guarded runtime path requires fresh gateway
policy and local authorization before a configured provider can be invoked.

At most 64 record dependencies and 128 KiB of context are returned. Other streams
are not followed through arbitrary causal references. Recipients with current
local read permission and Pull grant can inspect previously received copies
offline; remote revocation cannot recall those bytes. Disabling the local grant
denies subsequent local API reads. Infrastructure administrators can read stored
plaintext. No claim of secure erasure or customer legal-policy approval is made.

## Interpret synchronization status

`retained_through` is this receiver's cryptographically verified cursor.
`observed_capabilities` is membership at that prefix, **not** current remote
authorization. `local` reports the caller's queued intents, accepted-but-not-
replicated intents, replicated intents, blocked intents (a subset of queued),
and frozen source changes awaiting preparation. Replica retry time/error are
shared worker observations, not a connectivity probe or complete per-user health
assessment. Queue counters can advance while this snapshot is being read.

There is deliberately no invented current gateway publication head: accepted,
published and replicated remain distinct observations. Status does not assert
that other instances have received anything. Use the existing gateway operator
status for its publication queue. A successful GET while offline is not “synced”.

401 means authentication failure; 403 local permission/grant/CSRF denial; 404
disabled/mismatched configuration or absent source; 409 stale review or changed
binding; 413/422 body limit/shape errors; 503 unavailable or unverifiable state.
No database errors, endpoints, tokens or trust configuration are returned.

## Still required for E

Remote enrollment/member administration, grant listing and consent-management
clients, origin-owned correction proposals/approval, Web/CLI/TUI parity, and the
complete independently administered two-instance user journey remain open. This
API foundation is not permission to release customer federation or use Spaces
alpha. See the [roadmap](../reviews/federation-first/README.md).
