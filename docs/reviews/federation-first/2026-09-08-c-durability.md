# Stage C durability foundation — 2026-09-08

Status: **implemented persistence increment; C1–C3 integration remains open**.
The [roadmap](README.md) is still 8/39 complete. This is not a working federated
Memory product and does not advance the paid-pilot release gate.

## Implemented

`mem-federation-store` and additive migration 0033 provide trusted in-process
PostgreSQL APIs, separate from the unchanged neutral reducer and Atrium adapter:

- Caller-owned source transactions freeze explicit publication intent after the
  B2 local permission/grant intersection. Memory-version source references must
  resolve to the exact local project, canonical stream and source version. Queue
  rollback and duplicate intent are tested using real `memory_entries` writes.
- One row lock per gateway space serializes acceptance. Replaying committed
  history, checking authenticated publisher/current capabilities, applying the
  reducer, assigning the next order, recording the immutable event, replacing the
  derived projection and inserting outbound work are one transaction. No sequence
  allocation or repository key order is used. Duplicate retry returns the original
  acceptance even after intervening records or revocation; changed bindings fail.
- Publication leases have unique attempt tokens, database-clock expiry, bounded
  retries/backoff, visible blocked/error categories and explicit manual retry.
  Writer-epoch rotation rejects stale database writers and acknowledgements.
  The worker publishes only the next unpublished record and advances the public
  prefix after a successful immutable transport operation.
- Queued, accepted, published and per-receiver replicated status are distinct.
  Source acceptance receipts cannot silently change envelope, locator or sequence.
- Receiver inbox, derived projection and cursor commit together. Gaps, malformed
  deliveries and mid-transaction failures preserve the previous state. Full-log
  resync revalidates from genesis, rejects truncation/forks of the known prefix,
  and can rebuild a damaged projection while preserving a valid log/cursor.
- Pull authorization uses latest **accepted** gateway membership, not its lagging
  PDS prefix. Thus an accepted revocation denies new pulls immediately, even while
  the revocation itself is still unpublished. Previously delivered copies remain.

The [transaction contract](../../developer/architecture/federation-durability.md)
documents assumptions and limits. The implementation reconstructs projections
from all retained events: auditable reference persistence, O(history), not a
production-scale checkpoint/index implementation.

## Evidence and validation

Tests in `crates/mem-federation-store/tests/durability.rs` exercise a real
disposable pgvector/PostgreSQL 16 container, with fresh tmpfs data and a
loopback-only port. Existing dev/customer databases and volumes are not used.

Covered cases include concurrent acceptance/duplicate delivery, SQL failure
between accepted-event and outbound insertion, source rollback, missing local
grant, lost PDS reply with equal-record read-back, head-of-line blocking, lease
expiry, exhausted retries, immutable conflict, writer recovery, invalid receiver
batch, SQL failure at projection update, full resync, provenance/evidence bytes,
proposal approval, relation retraction, tombstone and post-revocation denial.

Validation:

- Nine focused durability tests pass against disposable PostgreSQL, including
  concurrent execution. Quarantined version/evidence gaps and cross-space intents
  consume neither acceptance order nor outbound work.
- The CI database command (`mem-test-support`, `mem-federation-store`, `mem-graph`,
  `mem-curate`, `mem-reinforce`, `mem-search`, `mem-service`) passes with a real
  database and `MEMORY_LAYER_TEST_REQUIRE_DB=1`. The ninth focused regression was
  added afterward and passed in its separate database run. The container and its
  tmpfs test data were removed after verification.
- `cargo test --workspace --all-targets --locked --offline` passes with isolated
  XDG configuration, inherited service/database variables unset and serial tests.
  Database-gated cases in this run are covered by the separate real-DB run above.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`,
  `cargo fmt --check`, `scripts/check-boundaries.sh` (offline) and `git diff --check`
  pass. CI now includes the storage crate in the database change bucket and test
  command; a boundary guard excludes network/ATProto/service dependencies from it.
- Neutral/Atrium tests pass: 40 core unit/property tests, 24 adapter tests and the
  two shared-trace harnesses. Lean `lake build`, `lake test` (fourteen traces) and
  `lake env leanchecker` pass, with unchanged legacy proof-linter warnings. No
  reducer semantics or formal claims were changed by the storage implementation.
- The approved-plan dry run correctly refuses full completion: 8/39 checked,
  31 remaining. C1–C3 stay open. Remote CI and independent nanoda were not run.

The PDS used by the worker tests is an in-memory **fake**, not Atrium or a live
network endpoint. None of these checks establishes live schema-v2 interoperability.

## Remaining C work (not hidden under D/E)

1. Persist/manage local sharing grants, review decisions and selected-stream
   subscriptions; wire source handlers so explicit publication and subsequent
   content/state changes really queue automatically in their mutation transaction.
   Evidence expansion must require renewed review. The new API is not called by
   current service handlers, and no existing project is enrolled by migration.
2. Add production-shaped schema-v2 ATProto mapping/unknown-field compatibility,
   then an authenticated gateway-only PDS adapter with immutable create/equal
   read-back and CID verification. `ImmutablePublisher` is the seam, not evidence
   of that adapter. Test uncertain network writes against the pinned private PDS.
3. Wire source-to-gateway intent retries/receipts and published-prefix receiver
   scheduling, including authenticated page/cursor verification and paginated
   full resync across separate databases. Queue enumeration is scheduling only;
   dependency quarantine must be retried, not interpreted as acceptance.
4. Wire local durable post-curation jobs and close the existing timeline
   mutation/event-log gap (R1). This increment neither replaces that queue nor
   relies on its event sequence for federation.

All APIs here are trusted internals, not remote authorization boundaries. Caller
supplied authenticated identities must come from verified transport; a digest is
not a signature. SQL fencing cannot revoke an in-flight PDS request. Restore of
credentials/identities and stale network writers, privacy isolation, runtime
resource limits, independent administration and Web/CLI/TUI workflows still need
their own evidence. No HTTP routes, deployment, credentials, Notion content or
startup documents were changed. Native Spaces remains disposable-data-only.
