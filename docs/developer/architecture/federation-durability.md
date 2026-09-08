# Federation durability

Stage C implementation contract, extending [ADR 0009](../adr/0009-private-federation-v1.md).
`mem-federation` remains a pure reducer. `mem-federation-store` owns PostgreSQL
transactions and transport-neutral delivery bookkeeping. It is not an enabled
service route, PDS client, identity verifier or publication-review UI.

## Transactions and order

- Source publication intent takes the caller's existing transaction, checks the
  explicit local policy, and freezes the selected envelope. Commit or rollback
  includes the source mutation. The intent is queued, not remotely accepted.
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

Full receiver resync revalidates history from sequence one, must extend the
already trusted prefix, and swaps inbox/projection/cursor in one transaction.
It cannot roll back a known prefix or silently change the configured authority.
This is replay recovery, not disaster recovery of credentials or signing keys.

## Integration gates still required

The caller must supply independently authenticated identities and server-resolved
local permission/grant inputs. A digest is not a signature. Receiver ingestion
assumes the gateway delivery and its publisher attribution have been verified.
No untrusted HTTP request may directly invoke these trusted in-process APIs.

Source handlers must compose intent with explicit review and subsequent selected
stream updates; this library alone does not turn on automatic synchronization.
Schema-v2 ATProto adaptation, immutable PDS write/read-back verification and the
gateway worker must be wired and tested before C2 is claimed complete. Offline
here means a local PostgreSQL instance works while the gateway/PDS/peer is down,
not that PostgreSQL itself can be absent. R1's existing post-curation queue and
timeline transaction gap are separate unfinished integration work.
