# Four-area staging: future cutover and recovery gates

This runbook prepares a later, separately approved cutover. **Do not execute a
writer stop, installation or activation as part of the current staging task.**
Original source writers and the Workshop/Homelab previews remain operational.
Trust and General remain inactive. R3 remains paused; no paid model run, public
DNS change or CA change is authorized here.

The authoritative staging source is the original PostgreSQL database shared by
the development and production configurations. The two previews are separate
deployments, not authoritative replacements for that source and not inputs to
an implicit merge. Preserve their independent identities, publication state,
usage journals and private answers through their own backup/recovery process.
Starting services against an import target can create authentication/runtime
state and invalidate fresh-import assumptions. Use separate disposable clones
for application validation; never treat those clones as another spending ledger.

## 1. Preserve the current evidence

Keep the encrypted source snapshot, independently recovered copy, sequence and
row verification, restore descriptor, complete inventory, exact payload, review
pack/index, ownership ledger, manifest, payload/import receipts and per-area
verification reports together. Record full hashes and the reviewed executable
revision. Preserve prior stages; never replace the only verified baseline.

The preview archive and its recovery evidence are separate. Follow the
[preview backup](../../../deploy/federation/preview-backup.md) and
[isolated recovery](../../../deploy/federation/PREVIEW-RECOVERY.md) procedures;
checksums alone are not recovery. If refresh maintenance is active, the backup
configuration must explicitly include `memory-pds-refresh-only.service` and its
approved state files so rotation cannot race the consistent backup. Interrupted
shutdown recovery restores only the exact original preview writers and makes
no claim that a backup was verified. It never automatically restarts the paid
worker or refresh maintainer.

Staging import preserves reviewed IDs, complete histories and tombstones.
Same-area relations stay local; cross-area relations remain protected archive.
Source projections are reviewed opaque references, not recovered evidence bytes.
Raw captures, sessions, tasks, proposals and derived-data gaps remain an explicit
backlog unless separately reviewed. Archival preservation is not searchable
migration. No copied authentication, jobs, enrollment or supervisor authority
may become active simply because its historical row exists in the archive.

## 2. Re-snapshot and revalidate while writers continue

The existing stage ages as the source receives writes. Before a later rehearsal
or cutover, take a **fresh** encrypted snapshot into a new protected directory
using [the snapshot/restore workflow](../../../scripts/areas/README.md). Capture
all required roles, configuration, identities and external evidence separately;
the online database dump does not claim a whole-host atomic backup. Verify an
isolated, network-none restore, including every sequence's `last_value` and
`is_called`, before generating its digest-bound restore descriptor.

From that fresh restore collect the complete inventory/payload and unresolved
manifest, then generate the complete redacted review pack using the
[review-ledger workflow](../../../scripts/areas/review-ledger-README.md). Do not
run those collectors against a live database or repoint an old descriptor.

After replacing the example protected paths with the exact reviewed old/new
artifact directories, the supported offline delta command is:

```sh
node scripts/areas/review-ledger.mjs delta \
  --index /protected/old/full-review.ndjson.index.json \
  --pack /protected/old/full-review.ndjson \
  --ledger /protected/old/ledger.json \
  --next-index /protected/new/full-review.ndjson.index.json \
  --next-pack /protected/new/full-review.ndjson \
  --output /protected/new/carried-ledger.json
```

This creates a new ledger and `.delta.json`, never modifying the previous
approval. Reuse requires matching **complete-history and dependency hashes**,
not just equal latest text, unchanged project slug or equal row counts. Source,
canonical-state, relation/endpoint or schema changes can invalidate an unchanged
text's decision. Review every new/changed history again, preserve the actual
agent reviewer, and resolve removed items explicitly. Do not turn an agent's
review into Olivier's personal approval or model/federation disclosure consent.

```sh
node scripts/areas/review-ledger.mjs validate-ledger \
  --index /protected/new/full-review.ndjson.index.json \
  --pack /protected/new/full-review.ndjson \
  --ledger /protected/new/carried-ledger.json
```

`ready: false`/exit 2 is a stop, not permission to fill decisions heuristically.
Only after review is complete create and verify a new manifest, materialize its
exact payloads and prepare fresh inactive targets using the existing
[import operator](../../../crates/mem-area-ops/README.md). Prove exact replay
and changed-input rejection again. A successful delta does not stop writers or
capture writes that occurred after its snapshot.

## 3. Resolve the CA-specific gap at the local console

Use [the existing CA handoff](../../../deploy/areas/CA-CONSOLE-HANDOFF.md) and its
read-only preflight. Transfer only the reviewed script/docs through an existing
approved local channel, independently verify their hashes, and return redacted
metadata. Do not enable SSH or examine CA keys/OpenBao storage. Staging Trust on
Monolith does **not** migrate the CA or prove that CA-only/offline knowledge was
included in the shared snapshot.

The console operator must establish the actual Memory/database binding, all
other writers and any CA-only database, offline outbox/activity or evidence
files requiring preservation. Unknown or inaccessible data remains unresolved,
not empty. A later consistent export/backup of that material requires its own
exact scope; do not copy an actively written DuckDB file or merge conflicts
automatically. Reconcile stable operation IDs and attribution before deciding
whether pending offline work is replayed or retained for operator repair.

The pinned upstream Pi v0.5.1 ARM64 artifact requires **glibc 2.43**. Lower libc,
wrong architecture or unknown runtime compatibility blocks that artifact. Do
not upgrade the CA's libc, substitute an unreviewed binary/container or relax
isolation. The patched AMD64 preview proves neither ARM64 patch parity nor CA
sandboxing. A separately reviewed compatible artifact and CA-specific negative
filesystem/network tests are required; Memory installation and Pi activation
are separate gates. No privileged selected-instance installer/rollback helper
is claimed by this handoff.

## 4. Approve the final cutover explicitly

Before a maintenance window, record exact instance/database/container/service
identities; every writer including dev/prod, direct tools, watchers and offline
replay; selected release/hash/schema; target credentials and authorization;
publication reconciliation; health checks; recovery owner; and stop conditions.
Do not assume that stopping one Web service fences direct library/CLI writers.

The approved future sequence is:

1. Stop/fence only the identified source writers and resolve in-flight/offline
   work; preserve all old data and configurations. Keep CA signing/OpenBao and
   unrelated services untouched. Confirm no stale writer can return.
2. Take the final encrypted capture; independently restore it; repeat full
   delta/review/reconciliation above while writers remain fenced. A late change
   requires a new snapshot/review, not an exception to the hashes.
3. Import and independently verify four fresh inactive targets and the protected
   backlog. Provision distinct authority deliberately; never restore historical
   job/auth/federation/supervisor rows as execution permission.
4. Activate only approved instances one at a time with current schema-compatible
   artifacts, fresh scoped credentials and explicit area policies. On the CA,
   use the reviewed local-console action only. No DNS or issuer change is
   implicit. Start with models/watchers/federation disabled until each separate
   authority and disclosure gate passes.
5. Verify local search/history/provenance and no cross-area leakage, then explicit
   two-way cited exchange, revocation, disconnect/recovery, publication order and
   visible stale/blocked health. Keep queued/accepted/published/replicated distinct.
6. Reconcile the final source position and target receipts before allowing new
   writes. Retain the old source read-only for the approved retention period;
   do not run concurrent old/new authoritative writers or count a copied usage
   journal as a fresh budget. Record the exact activation and unresolved backlog.

## 5. Recovery is not a database downgrade

Before accepting new target writes, failure may be recovered by stopping only
the new selected instance and returning to the preserved old writer **only**
after schema compatibility, unchanged old state and exclusive authority are
verified. Never start an old binary against a database that received newer
migrations. There is no automatic down-migration or package-removal rollback.

After target writes or externally accepted publications, stop and preserve the
new databases, jobs, inbox/outbox, cursors and receipts too. Do not discard them
or revert authority to a stale snapshot: reconcile accepted work and fence old
writers under a reviewed forward-recovery plan. External publication is not
rolled back by restoring PostgreSQL; prior recipients retain delivered bytes.
Pi/question workers and refresh maintenance resume only after their identity,
current grants, budget accounting and configuration pins are checked explicitly.

Completion of today's staging work proves neither this future cutover nor CA
installation, customer disaster recovery, autonomous supervision or R3 quality.
