# CA PostgreSQL 17 and Trust receiver handoff

> **Superseded on 24 September 2026.** Do not execute the side-by-side
> `17/trust` instructions below. Olivier selected replacement of PostgreSQL 15
> and upgrade of the existing Memory singleton. Use
> [CA fast replacement](CA-FAST-REPLACEMENT.md).

24 September 2026. PostgreSQL 17 is the selected CA target. Olivier authorized
one bounded autonomous preparation run on 24 September 2026 so he does not have
to approve intermediate checkpoints. This authority is limited to backup,
package installation, and creation/validation of an empty inactive `17/trust`
cluster. It does not authorize Trust data transfer, application activation,
singleton migration, federation, or changes to Step CA or OpenBao.

## Selected topology

Install PostgreSQL 17 alongside PostgreSQL 15. Prepare a dedicated `17/trust`
cluster first so Trust can be restored and inspected without moving the current
singleton.

| Item | Exact target |
| --- | --- |
| PostgreSQL | 17.11 |
| Client | 17.11 |
| Extensions | `vector` 0.8.6, `pgcrypto` 1.3, `pg_trgm` 1.6 |
| Cluster | `17/trust` |
| Database | `memory_trust` |
| Owner/login role | `memory_trust`, no elevated attributes |
| PostgreSQL port | 5433, Unix socket only during preparation |
| Application unit | `memory-layer@trust.service`, inactive |
| Application port | 127.0.0.1:8091 |
| Backup root | `/var/backups/memory-layer/trust` |

Port 5433 avoids PostgreSQL 15's normal 5432 listener. The Trust database is not
a replacement for the singleton database. Upgrading or moving that singleton is
a later migration after its binding, writers, offline DuckDB and recovery path
are fully reconciled.

## Completed read-only prerequisite packet

The CA agent reported Debian 12 ARM64, PostgreSQL `15/main` online on port 5432,
no PGDG repository, no PostgreSQL-17 candidate in its existing sources, no
`17/trust`, and unused port 5433. It also confirmed that the local
`postgresql-common` defaults would create and start `17/main` unless explicitly
suppressed. The current data and backup roots share a filesystem with
907,079,290,880 bytes free. Exact privileged PostgreSQL-15 cluster sizing remains
required before the live change.

Monolith bound the official Debian-12/ARM64 PGDG repository and exact package
hashes in [the package binding](PGDG-BOOKWORM-ARM64-BINDING.md). No other Linux
distribution is required or permitted by this handoff.

## Instructions to the CA agent now

Read this complete packet and the package binding, then execute the bounded
preparation from start to finish without asking Olivier to approve intermediate
steps. Perform the safety checks locally. Reply once on the coordination page
with `COMPLETED_INACTIVE` or `STOPPED_SAFE`, using the exact report template
below. A progress reply is needed only if a blocker cannot be resolved without
leaving this scope.

Execute only the following bounded work:

1. A privileged, read-only final inventory and sizing of `15/main`.
2. A fresh encrypted backup of the complete PostgreSQL-15 cluster, globals and
   relevant configuration metadata, plus isolated recovery evidence.
3. Addition of only the bound official `bookworm-pgdg` ARM64 repository and key.
4. Temporary suppression of package-service starts and automatic main-cluster
   creation. `/usr/sbin/policy-rc.d` is presently absent; any temporary file at
   that path must be uniquely recorded and removed only if this operation made
   it. Preserve and restore the original `createcluster.conf` exactly.
5. An APT simulation followed by the exact reviewed packages only. Expected
   shared upgrades are `postgresql-common`, `postgresql-client-common` and
   `libpq5`; this risk must be called out explicitly. Do not install
   `postgresql-server-dev-17`.
6. Verification that `15/main` stayed online without restart and that no
   `17/main` cluster exists.
7. Creation of only `17/trust`, port 5433, Unix-socket-only access and manual
   start policy. Create the least-privilege `memory_trust` role/database locally.
8. Stop with the new cluster empty and the Trust application absent/inactive.
   Data encryption, transfer, restore and application activation are not part
   of this operation.

The CA agent must retain the exact local command sequence, backup location,
encrypted-backup method, isolated recovery evidence and rollback commands in
its private workspace. Do not put credentials, keys, database URLs, configuration
dumps or unrestricted command output in Notion.

### Mandatory stop conditions

Stop before or during the change if any of these occurs:

- the repository key, signed index, package version, architecture or hash differs;
- APT proposes an unreviewed package, removal, downgrade, Debian upgrade or
  change to the `postgresql-15` server package;
- an automatic `17/main` appears or any package hook starts an unexpected service;
- PostgreSQL 15, Memory, Step CA or OpenBao stops, restarts or changes identity;
- the backup cannot be restored and checked independently;
- port 5433, `17/trust`, `memory_trust` or its target paths are already in use;
- the new cluster listens on TCP, starts automatically or contains unexpected data;
- any required value is unavailable or ambiguous.

Do not improvise around a stop condition. Preserve evidence, leave existing
services running where possible and report `STOPPED_SAFE`.

## Autonomous bounded execution

Execute the following in order, retaining private evidence for every step:

1. Take a fresh encrypted backup of the PostgreSQL 15 singleton plus globals,
   configuration metadata and the Memory offline store. Prove recovery in an
   isolated location. Do not stop Step CA or OpenBao.
2. Add only the reviewed package source if PostgreSQL 17 is not already
   available. Pin the exact PostgreSQL/client/pgvector versions. Prevent
   package hooks from creating or starting an unreviewed `17/main` cluster.
3. Create only `17/trust`, with port 5433, Unix socket access and manual startup
   policy during preparation. Keep PostgreSQL 15 running on its existing port.
4. Create `memory_trust` as a login role with no superuser, createdb,
   createrole, replication or bypass-RLS capability. Create `memory_trust` owned
   by that role. Generate credentials locally into protected files; never place
   them in Notion, shell history or process arguments.
5. Validate exact server/client/extension versions and an otherwise empty
   database. Stop if any identity differs from the receiver manifest.
6. Confirm `memory-layer@trust.service` is absent or inactive. Do not point any
   service at the empty database and do not enable federation, watchers, models,
   MCP, offline replay or Pi.

If package installation, cluster creation or restore fails, preserve the exact
evidence and stop. A fresh, never-accepted Trust cluster can be removed only by
the rollback steps retained for this operation. Never remove or downgrade the
PostgreSQL 15 singleton as rollback.

Stop with an empty, inactive database. Transfer and restore remain future work
because no current reviewed Trust payload exists yet. Agent-to-agent
coordination may continue without Olivier relaying messages, but neither agent
may infer permission to transfer private data or activate the application.

## Required CA report

Reply once to the existing Notion coordination discussion. Keep the same
`operation_id` across an internal retry. Post hashes and sanitized facts only;
retain full output in root-owned local evidence.

### Single completion or blocker report

```text
message_id: ca-monolith-<UTC-date>-pg17-result
recorded_at: <UTC timestamp>
actor: ca-agent
target: monolith-agent
kind: RESULT | BLOCKED
reply_to: <Monolith autonomous-execution instruction message_id>
phase: PostgreSQL-17 inactive Trust preparation
operation_id: <new UUID retained across retries>
status: COMPLETED_INACTIVE | STOPPED_SAFE | ROLLBACK_ATTEMPTED
authority_reference: Olivier message of 24 September 2026 removing intermediate gates
baseline:
  os: Debian 12 bookworm arm64
  clusters_before: 15/main port 5432 online; no 17 cluster
  pg15_data_bytes: <exact privileged size>
  free_bytes_before_after: <exact pair>
  existing_service_start_identifiers: <sanitized before/after evidence>
backup:
  encrypted_artifact_sha256: <hash>
  globals_sha256: <hash>
  isolated_restore_receipt_sha256: <hash>
repository:
  key_fingerprint: <fingerprint>
  inrelease_sha256: <hash>
apt_simulation:
  transaction_sha256: <hash of private sanitized simulation report>
  proposed_vs_installed_match: yes/no
packages:
  installed: <package=version list>
  package_receipt_sha256: <hash>
existing_pg15:
  state: online
  port: 5432
  restarted: no
  before_after_identity_match: yes/no
new_pg17:
  clusters: 17/trust only; no 17/main
  state: stopped | manually started for local validation then stopped
  port: 5433
  listen_addresses: empty
  start_conf: manual
  server_version: 17.11
  vector_version: 0.8.6
  database: memory_trust
  owner_role: memory_trust; no elevated attributes
  unexpected_rows: 0
unaffected_services:
  memory-layer: <state and restart=no>
  step-ca: <state and restart=no>
  openbao: <state and restart=no>
trust_application: absent or inactive; never started
rollback_or_recovery_actions: none | <sanitized exact summary>
deviations: none | <sanitized list>
private_evidence_root: <CA-local path; no contents>
next_action: stop; Monolith may continue preparation from this report without Olivier relaying it
```

A success report is incomplete unless it proves the encrypted backup was
restorable, PostgreSQL 15 did not restart, no `17/main` exists, `17/trust` is
manual/socket-only and the Trust application never started.

## Tested receiver evidence

Monolith produced a PostgreSQL-17 receiver bundle from the historical,
previously reviewed Trust partition. The receiver:

- accepts only PostgreSQL/client 17.11 and `vector` 0.8.6, `pgcrypto` 1.3,
  `pg_trgm` 1.6;
- accepts only database/role `memory_trust` on port 5433;
- rejects nonempty targets, changed manifests, altered artifacts and replay;
- installs only three allowlisted extensions as administrator, creates all
  Memory objects as `memory_trust`, and executes no source-provided DDL;
- restores schema and data in one transaction, then verifies 114 tables, four
  sequence states, all 60 migration checksums, ownership and immutable import
  receipts;
- restored and verified 375 imported rows, 185 reference-only sources, 19
  histories and 22 versions in the isolated rehearsal.

The current private bundle manifest SHA-256 is
`10dd74498e9fcb11abcfd81f22ce6c086731258722def4001895211d9a63a057`.
It is unencrypted and has not been transferred. Its contents remain bound to the
September 19 partition, so this proves receiver mechanics—not current dataset
approval.

## Encryption recipient bootstrap

The CA currently has no observed data-encryption recipient. Do not reuse the
release-signing key. The grouped proposal must create a dedicated CA-local
encryption identity with these properties:

- private key generated and retained on the CA in a root-owned private keyring;
- encryption use only, with explicit expiry and recovery procedure;
- public key exported through the approved operator channel;
- Olivier verifies the full fingerprint through an independent display before
  Monolith encrypts any data;
- a disposable synthetic round trip succeeds before real Trust data is sealed;
- the exact recipient fingerprint is recorded in the encrypted bundle manifest.

Choosing GPG versus age is part of the reviewed package proposal. Do not add a
new package or create the real key merely because this section exists.

## Later work outside this operation

The historical receiver bundle is not eligible for activation. Before a live
Trust service starts, complete renewed review for the 17 dependency-changed Trust
histories, reconcile CA-local/offline data, create a final writer-fenced capture,
and prove that only one instance with the Trust identity can write. Activation,
singleton PostgreSQL-15 migration and federation enrollment are separate scoped
operations; the agents should coordinate them without asking Olivier to relay
routine evidence.
