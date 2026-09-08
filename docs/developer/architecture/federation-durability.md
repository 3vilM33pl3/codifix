# Federation durability

Stage C implementation contract, extending [ADR 0009](../adr/0009-private-federation-v1.md).
`mem-federation` remains a pure reducer. `mem-federation-store` owns PostgreSQL
transactions and transport-neutral delivery bookkeeping. `mem-federation-runtime`
adds gateway HTTP, immutable PDS publication and instance scheduling;
`mem-service/src/federation.rs` starts these only with explicit administrator
configuration. No project is automatically enrolled. Review/enrollment are
trusted in-process APIs at C; Web/CLI/TUI parity is E.

## Transactions and order

- Source publication intent takes the caller's existing transaction, checks the
  explicit local policy, and freezes the selected envelope. Commit or rollback
  includes the source mutation. The intent is queued, not remotely accepted.
- Reviewed opt-in persists the exact selection, evidence and standing stream
  subscription. Migrations 0034/0035 add deferred hooks to existing memory,
  canonical-state and tag mutations. They freeze the final transaction snapshot
  and coalesce by transaction identity under a per-stream row lock. The transaction
  ID is only a deduplication key, never an ordering source. Unselected streams
  create no work. Future source changes retain the previously reviewed evidence
  references; new raw evidence is never automatically copied.
- Frozen changes are expanded into envelopes, shared content versions and state
  revisions in one subsequent transaction. This preparation is retry-idempotent.
  The immutable snapshot survives disconnection and local source-history pruning.
  Unknown original authorship is labeled `unknown:local-author`, not invented.
- Gateway acceptance locks one space row, replays the committed log, rechecks
  authenticated publisher/capability, and applies the reducer. Acceptance order,
  immutable event, projection and outbound work commit together. No PostgreSQL
  sequence, UUID sorting or repository key defines application order.
- Receiver batches lock their replica row and commit inbox, derived projection
  and cursor together. A bad record or gap rolls back the whole batch. Only the
  gateway's contiguous published prefix is eligible for pull, with membership
  checked against its latest **accepted** state (including pending revocations).

The first store replays retained history to construct the reference projection.
This is deliberately simple and auditable, but O(history), not a production-scale
checkpoint/index design. Retention must not delete queued or accepted history.

## Delivery and recovery

Queued, accepted, published and replicated are distinct milestones, not one
boolean. Replication is per receiver, never a claim about all members. Gateway
workers lease the next unpublished record with a unique attempt token, bounded
attempt count and database-clock expiry. Retry preserves the record and locator.
An exhausted head blocks the prefix visibly; explicit retry does not skip it.

A transport must create an immutable record or verify that an existing record
has exactly the same bytes. A lost reply is retried at the same locator. A lease
token fences late acknowledgements; space writer epochs fence stale database
writers. Neither can undo an in-flight network request: external safety depends
on immutable idempotency and gateway-only PDS credentials, not on a SQL lock.
An expired/superseded attempt returns `LostLease` and permits a later retry;
only a stale writer epoch returns `Fenced` and stops that gateway worker.

Full receiver resync revalidates history from sequence one, must extend the
already trusted prefix, and swaps inbox/projection/cursor in one transaction.
It cannot roll back a known prefix or silently change the configured authority.
This is replay recovery, not disaster recovery of credentials or signing keys.

## Runtime and remaining release gates

The caller must supply independently authenticated identities and server-resolved
local permission/grant inputs. A digest is not a signature. Receiver ingestion
assumes the gateway delivery and its publisher attribution have been verified.
No untrusted HTTP request may directly invoke these trusted in-process APIs.

The service now resolves current principal/project permissions for each worker
cycle; explicit grants are checked for pull and again for already-prepared
outgoing work. The gateway binds a private bearer token to an enrolled instance,
not the payload's author. It rechecks current membership under the acceptance lock.
PDS credentials are only present on the gateway path. Pinned DID repository keys
are UUIDv5 over space/publisher/record UUID; neither keys nor CID sort order is
application order. `createRecord` never overwrites: equal readback with matching
AT-URI, known delivery, complete unknown fields and computed DAG-CBOR CID is
required even after a successful reply. A neutral SHA-256 envelope digest is
distinct from that transport CID. Digests are not signature verification.

Instance ticks pull a bounded published-prefix page, prepare frozen work, submit
due intents and commit receipts, then catch up again. Retry counters, deadlines,
blocked work and sanitized errors survive restart. Rejections require explicit
retry; quarantine awaits dependencies without consuming the attempt budget.
Receiver failures back off up to 300 seconds. Network requests have 20-second
deadlines, 5-second connects, no redirects/proxies, and bounded streamed bodies.
Full resync fetches 32-record pages anchored to the initial published head and
atomically revalidates/swaps at most 16 MiB. Exceeding this limit fails closed;
larger installations need a reviewed staging/checkpoint implementation.

See the [operator configuration](../../user/federation-runtime.md) and
[C completion evidence](../../reviews/federation-first/2026-09-08-c-completion.md).
Offline here means a local PostgreSQL instance works while the gateway/PDS/peer is down,
not that PostgreSQL itself can be absent. R1's existing post-curation queue and
timeline transaction gap are separate unfinished integration work.

Real-PDS tests do not establish the full production trust boundary, native Spaces,
runtime signature validation, automatic token refresh, offsite recovery or E's
retrieval/citation/proposal/user interfaces. Administrators and authorized recipients
can read shared plaintext, and revocation cannot recall previously delivered bytes.
