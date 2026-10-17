# Fresh-target area import

`import_partition` is a trusted PostgreSQL library operation, not a deployment
command. It accepts only an opaque `MaterializedPartition` from `codifix-area` after
complete inventory/manifest/payload validation. It never discovers a source or
target database, migrates schema, reads credentials, writes an archive, starts
services, enrolls an instance or grants permissions.

The operator must independently approve the manifest, retain and durably protect
the complete source archive, identify the correct fresh target, and stop all
writers. The supplied target UUID is an explicit receipt binding, not proof of
host identity. A cloned database retains its receipts; this is not distributed
identity or stale-writer fencing.

## Transaction and verification

Supported fresh target heads are migrations **0056 through 0061**, with the exact public table
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

All importer queries explicitly use nonpersistent (unnamed) statements. This
includes insertion, full-row verification, canonical hashing, catalog reads and
both receipt paths. It must remain safe when the caller sets the SQLx statement
cache capacity to zero, as the operator does. No intermediate commits, disabled
triggers or omitted verification are used to limit database memory.

Reviewed mixed-source projections are the sole content transformation: their
safe note/reference and null context fields come from `codifix-area`, not importer
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
The receipt binds the exact target head. An additive upgrade retains earlier
receipts unchanged but does not authorize replaying the import at the new head.
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

The snapshot collector leaves `memory_relations` without a project owner.
Manifest v2 can explicitly review an edge whose two canonical endpoints belong
to the same area, preserving both original project bindings. Missing, archived
or cross-area endpoints remain blocked; no fictional source owner is assigned.

## Focused verification

Eleven tests use generated, migrated `codifix-test-support::IsolatedDatabase` sources
and targets with synthetic text only. They exercise exact historical versions
with reversed UUID order, tombstones, canonical state, source-check history,
tags, commits, populated reviewed relations across preserved project boundaries
within one area, exact retries, identity conflicts, changed rows, nonempty targets,
schema mismatch, private provenance projections and immutable receipts.
Injected insertion, post-trigger verification and deferred-commit failures
demonstrate rollback of data, receipts, identity sequence and the kill switch.
Replaying after activation or after provenance-receipt corruption is rejected.

Use only an independently selected disposable PostgreSQL test database:

```bash
CODIFIX_TEST_REQUIRE_DB=1 cargo test -p codifix-area-store --locked
```

The existing test harness requires `CODIFIX_TEST_DATABASE_URL`; it creates
and drops generated fixture databases and must never target a live or restored
real-data database. No paid provider, source snapshot or running application is
needed by these tests.

### Large-partition PostgreSQL memory regression

On 2026-09-20 a disposable, network-disabled, 2 GiB-limited PostgreSQL **17.11**
container using image
`sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180`
reproduced the large-import failure with synthetic relations only. This was not
a JSON decoding leak, a relation FK event queue, or tmpfs database storage.

The operator's zero-capacity statement cache combined with SQLx **0.8.6**'s
default `persistent(true)` created a new named server statement for each query,
without retaining a handle to close or reuse it. PostgreSQL keeps named
statements for the connection lifetime. The relevant implementation is SQLx's
[`prepare` and `get_or_prepare`](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-postgres/src/connection/executor.rs);
the server lifetime is described in the
[PostgreSQL extended-query protocol](https://www.postgresql.org/docs/17/protocol-flow.html).

Measured backend memory contexts over 20,000 identical-shaped parameterized
inserts (different synthetic row values):

| SQLx configuration | Initial bytes | Final bytes |
| --- | ---: | ---: |
| Cache 100, persistent | 1,368,744 | 1,419,944 |
| Cache 0, persistent (old operator behavior) | 1,368,744 | 497,289,896 |
| Cache 0, nonpersistent | 1,368,744 | 1,632,936 |

The failing variant accumulated approximately 24.8 KiB per row, predominantly
`CachedPlanQuery` and `CachedPlanSource`. The original probe's own periodic
monitoring queries also used persistent statements, explaining the small growth
in the nonpersistent variant; the retained diagnostic now marks those reads
nonpersistent too. These are backend allocator measurements, not total host RSS
or a promise about every dataset's memory use.

`tests/memory_growth.rs` includes a required-database regression with 2,048
relations and a reviewed source projection. It uses an actual zero-cache pool,
imports and exactly replays the complete opaque partition, checks all normal
receipts and requires **zero named prepared statements** after each operation.
A synthetic insert trigger and final allocator checks enforce a 16 MiB backend
context ceiling. The trigger only exists in the generated test database.

For larger verification, run the explicit 100,000-relation variant on a
resource-limited disposable server (not a production or restored database):

```bash
CODIFIX_TEST_REQUIRE_DB=1 cargo test -p codifix-area-store --test memory_growth \
  --locked full_import_large_relation_memory_probe -- --ignored --nocapture
```

The separately ignored `composite_statement_memory_probe` reproduces the old
SQLx behavior with a bounded 20,000-row comparison and aborts if measured
contexts exceed 512 MiB. It is diagnostic evidence, not an alternative import
path. Existing fault-injection tests remain the authority for all-or-nothing
row/receipt/sequence rollback. Real snapshot import and recovery evidence must
be recorded separately after operator approval and target verification.

The corrected 100,000-relation test completed import and exact replay in the
same 2 GiB-limited server: backend contexts measured **6,589,048 bytes** after
import and **6,593,144 bytes** after replay, with zero named statements after
both. It also included an explicitly reviewed source projection, exercising
canonical hashing and immutable provenance receipts. Total test time including
synthetic capture/materialization was 141.38 seconds. These results do not
claim that the real Workshop snapshot was imported by this test.

## Existing installation reconciliation

`reconciliation::preview` and `reconciliation::reconcile` are separate from the
fresh-target importer. They require migration 0061 and an existing immutable
area/instance binding (an import receipt or supervisor instance binding).
Preview binds the validated partition, inventory, manifest and snapshot plus
all destination knowledge rows, including destination-only rows. It contains
identifiers and hashes only. Source rows must have exactly matching columns.

Identical rows are reused. New histories append only beyond the destination's
existing version numbers; an occupied ID, older slot, different project or
different mutable state is a conflict. Conflicts are returned in the preview
and prevent execution. An operator may explicitly set `keep_destination=true`
for existing non-version rows, recording the retained hash in the immutable
preview receipt. This permits retaining local project metadata and mutable
state while appending compatible versions. Immutable version conflicts must be
resolved in the ownership review; this API has no force-overwrite switch.
Version insert triggers can reassert canonical state; reconciliation restores
only the exact reviewed canonical state (including explicitly retained local
state) after those inserts.

Execution requires a stopped-writer attestation and takes exclusive knowledge
table locks in a fixed order, rechecks the preview, and commits rows and an
immutable receipt together. Existing authentication, federation, job, policy,
budget and destination-only knowledge rows are retained. Sequence advancement
is transactional and never reduces an existing allocation watermark. Exact
retry verifies the full resulting knowledge snapshot and all source rows;
changed input, operation identity or post-commit destination data fails closed.
The receipt binds the complete decision manifest, including projection review;
the protected prepared archive remains required evidence.

Database locks fence the transaction only. The deployment operator must stop
and fence all writers until routing switches. Reconciliation does not migrate
schema, create identities, approve content, activate a service or publish it.
