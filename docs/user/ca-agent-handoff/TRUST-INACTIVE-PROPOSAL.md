# Trust inactive preparation — proposal for the CA agent

> **Postponed for the fast replacement.** Upgrade the existing Memory singleton
> and replace PostgreSQL `15/main` with `17/main` first. Do not create the former
> parallel `17/trust` target. See
> [CA fast replacement](CA-FAST-REPLACEMENT.md).

Updated 24 September 2026. Olivier authorized one bounded autonomous CA run for
backup, the pinned PostgreSQL-17 packages, and an empty inactive `17/trust`
cluster. The exact executable scope and single required report are in
[the PostgreSQL-17 handoff](TRUST-POSTGRESQL17-UPGRADE.md). This document remains
the wider transfer proposal: protected data has not been sent and activation is
not authorized.

## What Monolith has completed

Current update, 25 September: Monolith repeated the full source snapshot,
complete-history review, minimized receiver build and disposable PostgreSQL-17
restore. The current candidate contains **18 histories, 21 versions, 371 rows
and 183 reference-only provenance projections**. One historical Trust assignment
was removed because it concerned an unrelated Homelab application. The encrypted
273,431-byte candidate remains on Monolith; it has not been sent to or restored
on the CA. Its receiver manifest is
`15dc9804d36def59985c33055a81b74018278f336b6b1a00ca5839da84c04046`
and its encrypted tar digest is
`8783b54c0a8ba3840f1331fb78a3fc68ac22450b6b447fbbab458a0a3d87f9a7`.
The first current-data ciphertext used the operator backup recipient, not a
CA-proven data recipient, and remains local recovery material.

Update, 25 September 05:32 UTC: the CA agent subsequently created a dedicated
CA-local encryption identity and reported a passing synthetic round trip.
Monolith independently verified public-key SHA-256
`64d58dc720a940bd1408215507ac1f299346cd80de0f9b018427a3e17088f321`,
primary fingerprint `ACDBD1CB5ECB39FCA84439ABC2E27E7B42623A5E` and encryption
subkey `D39EB8070E71832B63E9E4516CA8C0F9CBCB9BF3`, then prepared the
current Trust ciphertext specifically for that subkey. The portable transfer
archive SHA-256 is
`79c0b5ed84484dcd3284baf8480d44e1dfce2ff7309f4116cc06b206319b6e17`.
At 05:40 UTC it was copied to removable USB filesystem UUID `66F0-75AD`,
verified from that filesystem, unmounted/remounted, verified a second time, and
cleanly unmounted. It has not been read on the CA, uploaded to Notion, decrypted,
restored or activated. On the CA, verify the portable tar digest above and every
entry in the contained `SHA256SUMS` before decrypting. The first CA operation is
transport verification and private decryption only; the transfer manifest does
not authorize restore or activation. The receiver's own `inspect` operation is
deferred because it connects to an already-provisioned port-5433 Trust target,
which has not yet been verified on the CA.

The CA Notion connector currently truncates the main discussion before the new
USB comments. The executable transport instruction is therefore duplicated in
the coordination **page body** as
`monolith-ca-20260925-trust-usb-body-v1`. The CA agent reported a safe stop before
that body update. It must report authenticated decryption and bundle extraction
before any database-target instruction is issued.

That report has now arrived with status `VERIFIED_DECRYPTED_EXTRACTED_ONLY` and
verification-receipt SHA-256
`8e31ae044df06bafc9ac401f291ec8bcd32ac774f576ba2891a03f3bd0a274e3`.
Every content identity matched and the CA reported no host mutation. Its observed
USB filesystem UUID `5A93-3F75` differs from Monolith's `66F0-75AD`; preserve both
as transport provenance rather than rewriting either observation.

The page body's current instruction is now
`monolith-ca-20260925-trust-target-restore-v1`. It creates a new manual,
Unix-socket-only `17/trust` cluster on port 5433 without modifying `17/main`,
creates a fresh passwordless unprivileged target role/database, runs the verified
receiver inspection and transactional restore, validates the receipt, and stops
the new cluster. It does not authorize application credentials, a Memory service,
activation or federation.

Historical 22 September evidence follows for comparison. The full shared source
had an encrypted snapshot and independent row/sequence
restore proof. A Trust-only candidate from the previously approved September19
partition has been restored and checked locally, preserving its logical identity
and import receipts without transferring other areas or executing source DDL.
Its encrypted local bundle is 301,633 bytes. It has **not** been sent to the CA.

Trust candidate: 19 histories, 22 versions, 65 relations, 185 reference-only source
records and 375 total imported rows. Reference-only records are not original
evidence bytes. All auth/runtime/job/federation/supervisor authority remains inert.

Fresh source comparison is **not ready for cutover**: 17 of 19 Trust histories have
dependency changes, although their memory-version/source rows are unchanged.
Across the full source, 262 decisions carry forward, 2,945 require renewed review
and 63 are new. Nothing was automatically approved. Old source writers stay up.

Olivier selected PostgreSQL 17 for the CA. The Trust target is PostgreSQL 17.11,
pgvector 0.8.6, `pgcrypto` 1.3 and `pg_trgm` 1.6. The CA's observed PostgreSQL
15.18 singleton remains untouched. Prepare `17/trust` side by side on port 5433;
moving the singleton is a later, separate migration. See
[the PostgreSQL-17 handoff](TRUST-POSTGRESQL17-UPGRADE.md).

## CA agent: return the remaining PostgreSQL-17 prerequisites

The CA already reported PostgreSQL/client 15.18, no available pgvector package,
no existing data-decryption recipient, an active singleton, one offline DuckDB,
unused named-instance paths, port 8091, database/role `memory_trust` and backup
path `/var/backups/memory-layer/trust`. Keep that packet as pre-change evidence.

That read-only packet is complete. The exact official PGDG repository, key,
packages and hashes are now bound in
[the package binding](PGDG-BOOKWORM-ARM64-BINDING.md).

## Bounded inactive host preparation

The PostgreSQL-only portion no longer needs intermediate human approvals. The
CA agent executes it once, stops with the empty cluster inactive, and reports
`COMPLETED_INACTIVE` or `STOPPED_SAFE`. Data encryption/transfer, restore and
activation remain outside that operation.

CA owns exact local paths/DB/role/network/security and interruption handling.
Monolith owns the minimized transfer, content bindings and source/delta review.
The CA agent does not ask Olivier for intermediate approval during the bounded
PostgreSQL-only run. It reports once on completion or a fail-closed stop.

| Resource | Proposed action and boundary |
| --- | --- |
| Signed release 2.1.0-rc.1 | Reuse authenticated native ARM64 artifacts already staged; preserve the USB. No new release signature or Pi artifact. |
| Root-owned tools | Exact authenticated `console.py`, `instance.py`, `release.py` at `/usr/libexec/memory-layer/releases/`; confirm hashes from original handoff. |
| Instance files | New `/opt/memory-layer/instances/trust/release/`, `/etc/memory-layer-instances/trust/`, `/var/lib/memory-layer-installations/trust/`; no overwrite/upgrade of existing state. |
| PostgreSQL 17 | Side-by-side `17/trust` cluster on port 5433, Unix socket only during preparation. Preserve the PostgreSQL15 singleton. Exact package source, versions, hashes, auto-cluster suppression and rollback are bound in the PostgreSQL-17 handoff. |
| Database and role | CA-local `memory_trust`, owned by login role `memory_trust` with no elevated attributes. Credentials generated and retained locally; fresh and isolated, never copied auth or service authority. |
| Data delivery | Separately authenticated, approved-recipient encrypted Trust-only artifacts over an explicit transport. No full archive or unrelated area. Receiver tooling/version pair must be tested first. |
| Recipient | Later work outside the current operation: create a dedicated CA-local encryption identity. Do not reuse release signing. Export and independently verify only its public fingerprint/key; prove a synthetic round trip first. |
| Restore | Use the hash-bound PostgreSQL17 receiver against the exact fresh target. Trusted extensions + role-owned schema + allowlisted data + trusted post-data commit in one transaction; then independent receipt/hash/sequence/ownership validation. Preserve target UUID/import UUID. Never `--clean`, disable triggers or edit immutable receipts. |
| Configuration | Exact digest reviewed locally; read-only application mode and all background/model/watch/federation/offline replay/MCP paths off. Secrets stay in protected files. |
| Systemd | Install inactive under the existing console contract; no start/enable or activation marker. Native sandbox/CA-secret-path/egress denial tests are later explicit checks, not assumed from package compatibility. |
| Recovery | Preserve inputs, encrypted backup and install journal. Use inspected journal recovery for interrupted file installation. Failed fresh restore must roll back; ambiguous success requires inspection, never automatic drop/recreate. |

Keep Step CA, OpenBao, DNS, OS and the existing singleton unchanged. Installing
inactive is not permission to launch the service: startup can bootstrap/migrate
a database even when application requests are read-only.

The current receiver manifest is
`10dd74498e9fcb11abcfd81f22ce6c086731258722def4001895211d9a63a057`.
It passed inspect, atomic restore, 375-row verification and replay refusal against
PostgreSQL17.11/pgvector0.8.6. It is unencrypted, not transferred and still bound
to the historical partition; do not treat it as the final migration payload.

## Activation remains out of scope

Before stopping or moving any original writer: complete CA-only/offline
reconciliation, renewed source/dependency review, exact receiver validation,
native runtime and sandbox checks, and a final coordinated writer-fenced capture.
Both the original staging copy and the relocated logical Trust instance must not
be active simultaneously. After any accepted target writes, recovery is not a
blind rollback to the old source.

Pi, federation enrollment, model destinations/budgets, R3 and customer readiness
remain separate. The [coordination page](https://app.notion.com/p/3e38990715f48117b9e8d8f3a95637d1)
is for sanitized agent messages, not authorization or an automatic worker.
