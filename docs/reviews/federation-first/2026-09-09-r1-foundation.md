# R1 foundation and first transaction conversions

**R1 is not complete.** The scoped contract currently has 1/16 fully completed
items, with partial implementation across several others. The approved full
plan and federation-first master remain open; neither is archived.

## Implemented

- `d3c6aba`: approved contract/checkpoint and explicit mutation coverage inventory.
- `7fb0880`: independent `mem-store`, pure job contracts, commit-held timeline
  counter/retention watermarks, immutable durable jobs/dependencies, project
  leases, bounded attempts, retry receipts and fenced result transactions.
- `064383c`: token revocation and membership grant/revoke commit with protected
  security audits. Failed audit append rolls back the mutation. Revocation
  retries report the actual stored revocation time.
- `02aa183`: transaction-taking capture API, attributed event in the same
  transaction, post-commit-only HTTP broadcast, concurrent replay returning the
  original session/task/capture IDs. New offline captures carry capture and
  activity intent in one durable item; PostgreSQL replay commits both before
  local acknowledgement. SQL validation/constraint errors no longer masquerade
  as database outages and silently queue offline work.
- `810b656`: transaction-taking curation API and atomic run/automatic-replacement
  events, subject identity preserved, competing curators serialized per project,
  direct-library dry-run handling and no duplicate HTTP event append.

## Evidence

- Required-disposable-PostgreSQL workspace suite: **730 tests / 41 suites**, zero
  failures or ignored tests. Includes existing C/D runtime and E two-instance
  HTTP regressions in their default local test modes. This is not a new run of
  the real-PDS/browser/CLI/PTY harness.
- Final focused store/curation/service suite passed after the unused-import
  cleanup. The final direct-library dry-run adjustment was additionally checked
  with all 20 `mem-curate` tests. No live provider calls were needed.
- Workspace Clippy with `-D warnings`, formatting and boundary checks passed.
  Lean/no-default-feature CLI check passed with the same eight existing
  unrelated dead-code warnings; this is not Lean theorem checking.
- Store tests prove event/mutation rollback, delayed-transaction cursor
  ordering, retention signalling, immutable/deduplicated jobs, competing
  claims/dependencies, stale-worker rejection, output rollback on lease expiry,
  bounded retry/blocking and retained retry history.
- Added capture/curation tests prove rollback and concurrent delivery identity;
  offline test reopens DuckDB and repeats delivery after PostgreSQL commit but
  before acknowledgement, retaining one event and original actor/time.
- Plan-execution dry-run correctly reports **incomplete, 1/16**, and does not
  create a completion record or archive the plan.

Local command logs were written under `/tmp/memory-r1-validation.MmgPjd/`;
temporary logs are not a substitute for the committed regression tests.

## Remaining implementation / next slice

The **old in-memory post-curation queue is still active**. The new store is not
yet a production worker or an exposed job API. Next, split embedding and
consolidation provider preparation from transactional result writes; only then
wire the leased worker and atomic curation job intents. The existing embedding
rebuild currently deletes chunks before provider work, so merely wrapping it in
a lease would not provide safe fencing or preserve data on provider failure.

Complete the remaining mutation/audit inventory, service cursor integration,
authority-aware worker lifecycle, coalescing/dependent stages, operational
metrics and Web/CLI/TUI inspection/retry. No client job UI or routes were added
in this slice. Full end-to-end job recovery, provider-result fencing, real-client
workflow, Web and release validation remain outstanding.

Existing offline items lacking an authenticated intent snapshot cannot be
retroactively given verified attribution. Drain legacy queues before cutover;
do not fabricate historical events or lost work. New migrations require all
writers stopped, a verified backup, migration and coordinated restart. No
customer/dev application deployment or database migration was performed; only
the disposable test database was migrated.

This work does not establish customer recovery, release readiness, evaluation
quality, or eligibility to begin F.
