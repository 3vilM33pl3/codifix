# Fresh-target area import

`import_partition` is a trusted PostgreSQL library operation, not a deployment
command. It accepts only an opaque `MaterializedPartition` from `mem-area` after
complete inventory/manifest/payload validation. It never discovers a source or
target database, migrates schema, reads credentials, writes an archive, starts
services, enrolls an instance or grants permissions.

The operator must independently approve the manifest, retain and durably protect
the complete source archive, identify the correct fresh target, and stop all
writers. The supplied target UUID is an explicit receipt binding, not proof of
host identity. A cloned database retains its receipts; this is not distributed
identity or stale-writer fencing.

## Transaction and verification

The only supported target head is migration **0056**, with the exact public table
catalog, no historical schemas, and matching source columns for every imported
row. The first import requires empty knowledge, authentication, jobs, federation
and supervisor state; only the known empty-installation seed rows are allowed.
It checks freshness before taking broad locks and again under locks.

One transaction imports the selected project/session/task/capture records,
complete memory histories (including tombstones), canonical state, sources,
source checks, tags, relations and commit provenance. It never copies old tokens,
membership grants, jobs, federation identity, supervisor authority or derived
indexes. Those original rows remain accounted for in the protected archive by
the upstream planner/materializer.

Original logical row bytes use PostgreSQL `to_jsonb(row)::text` under the pinned
UTC/format settings. Insertion uses PostgreSQL composite decoding without a
floating-point JSON round trip. Existing triggers remain enabled; explicitly
restoring canonical state after all versions prevents insertion order from
silently replacing reviewed state. Every imported row is compared with its
expected JSON value and SHA-256 after triggers execute. Row counts must match.

Reviewed mixed-source projections are the sole content transformation: their
safe note/reference and null context fields come from `mem-area`, not importer
heuristics. Immutable provenance receipts retain the original source/version/
archived-task hashes and exact review-decision binding. PostgreSQL-rendered target
hashes are recorded separately from the materializer's wire-byte hashes.

Migration 0056 adds singleton import identity, per-row and projection receipts.
Rows, receipts, canonical state, the source-check identity sequence restart and
the local loop kill switch commit together. A failure before commit rolls the
operation back; a lost commit acknowledgement remains ambiguous until exact
receipt reconciliation. Errors are fixed categories and never contain PostgreSQL failing-row
details or private input. No external effect is described as rolled back.

An exact retry requires a still-inactive target (including the enabled local
kill switch and empty authority/job tables), then verifies identity, all imported rows, counts and receipts before
returning `already_applied=true`; it does not reinsert data or repair changed
rows. A different target/operation/manifest or changed target fails. If the
commit acknowledgement was lost, reconnect to the independently verified same
target and retry the exact operation. Preserve failures for reconciliation; do
not delete the target or invent a new operation to bypass them. Replaying after
activation is not supported; use a separately authorized operational audit.
Receipts attest to import contents, not deployment approval.

## Limits and remaining integration

This is offline bulk import, not merge, sync, backup restore or a service route.
No automatic retries, destructive overwrite, credential migration, legacy job
replay or reconstructed timeline events are provided. The importer does not
establish application read-only mode or prevent a privileged administrator from
changing schema; the stopped-writer/operator boundary remains required.

Real-data classification approval, filesystem/archive durability, actual-host
writer fencing, a protected operator CLI, release packaging, downstream index
rebuild, target isolation, enrollment and live cutover are separate gates.
Synthetic PostgreSQL evidence is not authorization to migrate the running
Monolith or CA instances.

The current snapshot collector leaves `memory_relations` without project
provenance. The planner therefore blocks sending those records to an area; it
does not silently assign an owner. A reviewed endpoint-ownership contract and
collector evidence are still needed. The importer tests deliberately do not
claim populated relationship migration coverage.

## Focused verification

Nine tests use generated, migrated `mem-test-support::IsolatedDatabase` sources
and targets with synthetic text only. They exercise exact historical versions
with reversed UUID order, tombstones, canonical state, source-check history,
tags, commits, exact retries, identity conflicts, changed rows, nonempty targets,
schema mismatch, private provenance projections and immutable receipts.
Injected insertion, post-trigger verification and deferred-commit failures
demonstrate rollback of data, receipts, identity sequence and the kill switch.
Replaying after activation or after provenance-receipt corruption is rejected.

Use only an independently selected disposable PostgreSQL test database:

```bash
MEMORY_LAYER_TEST_REQUIRE_DB=1 cargo test -p mem-area-store --locked
```

The existing test harness requires `MEMORY_LAYER_TEST_DATABASE_URL`; it creates
and drops generated fixture databases and must never target a live or restored
real-data database. No paid provider, source snapshot or running application is
needed by these tests.
