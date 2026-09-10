# R1 continuation: transactional provider-result boundaries

Historical slice report. For current status, see [R1 completion](2026-09-10-r1-completion.md).

Date: 2026-09-09. Branch: `codex/federation-v1`.

R1 remains in progress. This is evidence for two bounded implementation slices,
not completion of durable background execution or a deployment approval.

## Implemented

- `00c6390`: embedding provider preparation precedes all destructive chunk
  writes. Prepared results validate vector counts/dimensions/finite values and
  source identity/content. Application uses only the caller's transaction;
  replay uses stable chunk IDs. Existing pool-based rebuild callers now replace
  chunks atomically and retain old results if the provider fails.
- `40af23c`: proposal creation commits the proposal, original-input receipt,
  linked run trace/count and timeline event together. Operation-key replay is
  serialized and rejects changed inputs/actor identity without overwriting
  human edits or adding another proposal. Run/target project checks precede
  insertion. HTTP creation broadcasts only the committed event and records
  authenticated attribution; direct callers use an explicit internal identity.
- Migration `0041_proposal_creation_receipts.sql` is additive. Original creation
  fingerprints are separate from mutable proposal review state. No older
  migration was edited and no historic receipt/event was fabricated.
- Consolidation provider preparation now returns a proposal request separately
  from persistence. Provider-attempt auditing remains separate; there is no
  claim that external provider effects share the result transaction.

## Verified

All database tests used a task-owned, loopback-only disposable pgvector database,
with `MEMORY_LAYER_TEST_REQUIRE_DB=1`. No customer data or paid provider calls.

- `cargo test -p mem-search --all-targets --locked -- --test-threads=1`: 46 tests.
  New cases cover provider errors/bad vectors, wrong project, prepared-output
  round-trip, rollback/replay, changed/deleted source, and real embedding writes
  rolled back by an expired `mem-store` result fence.
- `cargo test -p mem-service proposal_creation --lib --locked -- --test-threads=1`:
  three new tests. Concurrent operation replay creates one proposal/event/trace;
  replay preserves a later reviewed state; changed payload/actor and a foreign
  run fail; a late event failure rolls back all creation writes; lease expiry
  rolls back actual proposal/receipt/trace/event results.
- `cargo test --workspace --all-targets --locked -- --test-threads=1`: passed
  with the database required, including existing federation, offline replay,
  proposal application, procedural utility and OpenAPI inventory tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo build -p mem-cli --no-default-features --locked`: passed, with eight
  dead-code warnings in unchanged CLI code.
- `cargo fmt --all --check`, `git diff --check`, and
  `scripts/check-boundaries.sh`: passed.

Federation results refer to the suites' default local test modes. Live private
PDS, browser/CLI/PTY journeys, Web tests/build and customer recovery exercises
were not rerun in this slice. Stale-fence tests simulate expiry in PostgreSQL;
they do not replace future service process-kill/restart tests.

## Remaining integration boundary

The old in-memory post-curation worker is still active. Curation does not yet
enqueue durable jobs. Existing automatic proposal callers still allocate fresh
operation IDs; the stable-ID API is ready but their durable retry path is not
connected. Source-version snapshots/revalidation for consolidation, reusable
successful provider outputs, semantic dedup result transactions, durable child
jobs, policy/authority checks, worker recovery/shutdown and client job operations
remain open. Proposal edit/approve/reject and other mutation families also remain
in the [coverage inventory](r1-coverage.md).

The approved-plan dry-run verifier still reports **1/16 complete** and refuses
completion. No checkbox was promoted for partial implementation. The next slice
should persist version-pinned stage inputs and connect execution only after all
derived writes can use the lease fence and current authorization checks.

No running Memory service or application database was migrated/restarted. The
task-owned PostgreSQL container and its disposable volume were removed after
verification. Nothing from a production or development application store was
deleted. The coordinated writer-stop rollout requirement remains unchanged.
