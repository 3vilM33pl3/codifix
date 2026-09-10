# R1 engineering completion evidence — 2026-09-10

Scope: durable local background work and transactional mutation events. No
customer deployment, production migration, startup-document change, retrieval
algorithm change or F work is included. The
[coverage inventory](r1-coverage.md) maps application writes to transactions,
dedicated audits or operational journals, including administration and direct
repository callers. The [rollout procedure](../../developer/architecture/r1-rollout.md)
requires a coordinated writer stop, backup, additive migration and restart.

## Implemented

- `mem-store` owns transaction-aware timeline, jobs, attempt/lease, foreground
  execution and external-effect storage. Shared serializable contracts remain
  in `mem-record`; provider execution stays in `mem-service`. Boundary checks
  prohibit network/configuration/ATProto dependencies in the store.
- Knowledge and administration writes commit with their event/audit and required
  work intents. Commit-held sequence allocation, bounded cursor reads and
  retention watermarks prevent cursors passing an earlier uncommitted event.
  Broadcasts are post-commit conveniences, not the recovery mechanism.
- Recoverable embedding, semantic dedup, independent validation checks,
  coalesced consolidation and durable synthesis children replace the process
  queue. PostgreSQL enforces global project admission and per-project stages.
  Results, caches, proposals and attempts are fenced against expired owners.
- Current authority and configuration apply before execution and publication.
  Retry/block/exhaustion, generations, uncertain attempts, provider caches and
  original attribution remain visible without exposing operational payloads.
- Web/CLI/TUI share project-scoped inspection and explicit idempotent retry.
  Independent offline intent replay and external/local journal reconciliation
  preserve uncertainty without pretending a remote effect was rolled back.
- New migrations are **0039–0047 only**; a diff against the pre-R1 baseline
  confirms no applied migration was edited. Repair creates new selected-version
  work and a current event, never fabricated historic jobs or curation runs.

## Verification

Commands run locally with disposable PostgreSQL, temporary stores and fake
providers. `CARGO_INCREMENTAL=0` kept the large workspace's build cache bounded;
the database variables below were set only to the owned disposable instance.

```sh
RUST_TEST_THREADS=8 MEMORY_LAYER_TEST_REQUIRE_DB=1 \
  cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
scripts/check-boundaries.sh
cargo build -p mem-cli --no-default-features --locked
cargo build -p mem-cli --bin memory --locked
node scripts/r1-process-smoke.mjs
cd web
npm test
npm run build
```

All passed. The workspace includes OpenAPI/router inventory, permission/CSRF,
offline, mutation, store/worker and existing federation C/D/E regressions.
The final service library has 115 passing tests, repository integration 22,
and OpenAPI 1. Web: 22 files / 76 tests. The lean CLI build retains eight existing
feature-specific dead-code warnings; the Web build reports its existing large
graph chunk warning. Neither warning was hidden or treated as a failed build.
No remote CI execution or new real-PDS run is claimed. Lean CLI means the
no-default-feature binary, not a new formal theorem-prover run.

An unrestricted test-runner invocation exhausted the test PostgreSQL connection
limit while creating concurrent isolated databases/owner sessions. The passing
run and CI use eight harness threads; explicit competing-transaction/process
tests still run concurrently. This is test capacity control, not disabled tests.

See [operator/process evidence](2026-09-10-r1-operator-workflow.md) for actual
Web, CLI and 80×24 PTY interaction and the three OS-process crash boundaries.
Independent services preserve live leases, recover killed provider owners and
reuse committed vectors after a crash before acknowledgement. An accepted
provider request without a cached result can still be billed twice.

The privileged repair example was also run twice against the disposable service
with the same operation UUID. It returned the same four jobs and one repair
event. Embedding/dedup succeeded once; disabled validation/consolidation skipped.
The repair transaction regression proves late-event rollback and conflicting
actor rejection without changing the original memory.

## Limits retained

The Memory plan-execution verifier returned `verified_complete: true`, 16/16
items and no remaining items for the explicit R1 thread and local contract.
Verification was read-only (`--dry-run`); it did not archive the wider roadmap
or invoke dev-service curation. That service has paid embedding creation enabled,
so automatic completion curation was deliberately not invoked. Repository
evidence and the completion checkpoint remain the source of truth.

R1 completes engineering, not customer DR, rollout approval, held-out answer
quality or F eligibility. Native Spaces, signatures/identity recovery,
permissioned federation deployment and actual customer restoration keep their
separate gates. Windows directory-metadata power-loss durability is unverified.
Trigger receipt is not completed dispatch; interrupted external work requires
inspection, not automatic command/provider replay. Pre-cutover history and
unverified legacy attribution are not reconstructed.
