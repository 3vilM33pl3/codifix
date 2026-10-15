# Company owner backup preparation — 2026-10-15

Status: **fresh encrypted owner backups and isolated restore verification
complete** at `2026-10-15T06:28:33Z`. This is backup preparation, not live
migration or company cutover acceptance.

## Confirmed scope and inventory

Approved-plan checkpoint: `c3b4ac9a-63ce-447c-b384-973270144f21`, capture
`ffc3768d-06e7-4610-9faa-b16be4f61356`. Preparation covers only Homelab and
Workshop owner databases, exact active application mounts and container metadata.
It excludes a whole-gateway/PDS backup or any service/network/authentication
change.

| Owner | Live database container | Database | Schema head / successful rows | Approx. size |
| --- | --- | --- | --- | --- |
| Homelab | `memory-homelab-preview-db-20260919` | `memory_homelab_preview_20260919` | 76 / 76 | 39 MB |
| Workshop | `memory-workshop-preview-db-20260918` | `memory_workshop_preview_20260918` | 76 / 76 | 188 MB |

Inventory observed around `2026-10-15T06:23Z`. Both live database containers use
image `sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180`.
Their `pg_dump` is PostgreSQL 17.11; host `pg_restore` is 17.9.

The exact active Workshop `/config.toml` mount is
`/home/olivier/.local/state/memory-workshop-preview-20260918/questions-renewed-20260919-grant7d/app.toml`.
The root Workshop `app.toml` is not its active container configuration. Homelab
uses `/home/olivier/.local/state/memory-homelab-preview-20260919/app.toml`.
Both applications bind their executable from
`/home/olivier/.local/state/memory-company-20260919/runtime-dependable-20260919/memory`.

## Captured artifacts and verification

Private backup directory (mode 0700):
`/data/tmp/memory-owner-backup-20261015-BohNpprC`.
Ciphertexts and receipts are mode 0600. The existing recipient was read from
`/home/olivier/.local/state/memory-company-20260919/dependable-backup-config.json`.
The initial error-mode pinentry probe failed; one subsequent attempt using normal
existing GPG pinentry succeeded. All seven ciphertexts were then independently
decrypted and hashed successfully. No recipient, key, credential or trust
configuration was changed. There is no remaining decryption-verification gap.

| Ciphertext filename | SHA-256 |
| --- | --- |
| `homelab.pgdump.gpg` | `160fdad344f8cb6556c36b9da94d6627029d6d373f7d2beec3eac89af705c65d` |
| `workshop.pgdump.gpg` | `12e1789021e4d76af5b32aa3e112527e9c4cf23efcb9332177041ba606e079be` |
| `owner-config-runtime.tar.gpg` | `afa0c94249f0304df2b8de8b938dc1e1dab120f4c64767b0cdb2ef60078b5d16` |
| `homelab.globals.sql.gpg` | `09fc5070b2224f8c156a2ec345a2d5b8f8508f3b6edf08c4f7fe71bfe023271e` |
| `workshop.globals.sql.gpg` | `f1019e8347a3a8b8dec27b1b84f729818e3a7a77f0b4395b84c38cfc5c21b94e` |
| `container-metadata.json.gpg` | `ad9560ea22209fb56f5aa8c4a3616a7889d615cb6a3a9eb322aa5ea6d92b798b` |
| `container-metadata-after.json.gpg` | `f697e0a72a56181e46ae7ff347d1422cb0a25167f7e9ac690ab19b9a3eb158a2` |

The configuration archive captures 25 explicit paths: every active application
bind mount, the owner client/deployment/relay configuration and owner secrets.
It includes the active Workshop config, runtime executable/web/interaction files,
state, member configuration, exact per-owner gateway tokens and public CA file;
it does not traverse the whole gateway or PDS. Exact inventory is in protected
`config-inventory.json`; complete live container metadata is encrypted.

Both custom dump buffers passed `pg_restore --list`, then restored with roles,
ownership and privileges into separate fresh PostgreSQL 17.11 containers using
the exact existing image above. Each sandbox used network `none`, a read-only
root, no published ports, no restart policy, fresh data volume and private tmpfs
for PostgreSQL sockets. No application/provider process ran. The existing
repository `archiveInventory`, `inventoryDifferences` and `roleRestoreSql`
helpers verified every table's row multiset and every sequence state.

| Owner | Dump UTC window | Tables | Rows | Sequences | Restored migrations |
| --- | --- | ---: | ---: | ---: | --- |
| Homelab | 06:27:27–06:27:28 | 146 | 41,395 | 4 | 76 successful, 0 failed |
| Workshop | 06:27:34–06:27:39 | 146 | 474,347 | 4 | 76 successful, 0 failed |

Both restores also match schema DDL, owners, object grants and global
roles/grants, with zero invalid indexes or unvalidated constraints. Extensions
are `pg_trgm=1.6`, `pgcrypto=1.3`, `plpgsql=1.0`, `vector=0.8.6`.
Normalized schema SHA-256s are Homelab
`5a4581f301db78bbbdd026a115db9497986523509cf30802081dd4d117ea3b18`
and Workshop
`d9cdefe51cda413901ee11f5e6baa73792bdfb7252d4ef37c9cc70d79859a7a3`.
Private `manifest.json`, per-owner receipts and restore inventories bind the
exact ciphertext/plaintext hashes, container/volume identities and row hashes.
Manifest SHA-256:
`885e8fb765e7c86fb0dc5ece044565b0fe00e7795083b46006773121c2ce29e0`.

No plaintext dump, roles SQL or configuration file was written to disk; their
transient process buffers ended with the operator processes. Restored database
volumes remain sensitive plaintext and are explicitly retained for the pending
schema-84 migration rehearsal, with their containers stopped and network `none`.
Containers are `memory-four-area-restore-owner-homelab-20261015-bohnpprc` and
`memory-four-area-restore-owner-workshop-20261015-bohnpprc`; each data volume uses
the same name plus `-data`. Exact IDs are in the protected per-owner receipts.
All live owner
container IDs, start times and complete active mount mappings were unchanged.
Docker varied mount array order during inspection; sorted complete mappings
matched. No live service was stopped/restarted, and no source record was written.

## Sufficiency and remaining gaps

This provides a decryptable, restore-verified pre-migration snapshot sufficient
as the backup input for **owner-only migration rehearsal**. It does not by itself
approve or prove a future live upgrade. Test the exact intended migrations on a
recovered clone before changing live owners, and arrange a fresh final backup
appropriate to the authorized cutover and its rollback needs.

Each live `pg_dump` provides a coherent database snapshot separately. PostgreSQL
sequence values are nontransactional; roles and filesystem files are separate
reads. These owner backups do not establish gateway/PDS/global consistency, a
stopped-writer cutover point or recovery of subsequent live writes. No new
application startup, runtime role-grant acceptance or off-host backup copy was tested.
Whole-company rollback, off-host redundancy and gateway/PDS recovery remain
outside this scoped preparation. All originals are preserved.

## Candidate schema 76 → 84 rehearsal

Both retained clones subsequently passed the exact candidate's pending
migrations, without connecting to either live database. Candidate executable
`/home/olivier/Projects/memory/target/release/memory` was hash-checked as
`b37d8a057f99640727755e9971e951c3f1424ba29a2e1e080150de872347d248`.
The current source migrations 0001–0084 were copied into the protected backup
directory and individually hash-bound before execution. The candidate build
predates only documentation changes, as confirmed by the coordinating task.

The existing `sqlx-cli 0.8.6` from
`/data/tmp/memory-agent-rc4.GM1GOp/tools/bin/sqlx` ran inside each clone against
its exact database over `/var/run/postgresql`, using `--no-dotenv` and the frozen
migration directory. Tool SHA-256:
`acff2959b3913ea607caf22f83c8f135168cf5f5fc693ade1121bdc0a6104bc3`.
It emitted a nonfatal weak `GLIBC_2.39` warning inside Debian; the actual SQLx
migration and verification runs completed successfully. No migration history was
edited manually. There was no Memory application startup, application config,
provider credential mount, scheduler, federation worker or watcher; persisted
clone jobs could not execute. Sandbox networking remained `none`, with no
published ports. Only the disposable clones were started/stopped.

| Check | Homelab | Workshop |
| --- | ---: | ---: |
| Schema head / successful migration rows | 84 / 84 | 84 / 84 |
| Failed migration rows | 0 | 0 |
| Original nonhistory tables with unchanged complete row hashes | 145 | 145 |
| Original nonhistory rows preserved | 41,319 | 474,271 |
| Original sequence states unchanged | 4 | 4 |
| Expected new tables, all empty | 13 | 13 |

Every original data table is covered, including knowledge, provenance,
federation, durable jobs and usage/budget records. The original 76 SQLx history
rows also match their backup row hashes, while all 84 applied SHA-384 checksums
match the frozen migration files. New tables match the explicit expected set for
0077–0084; no unexpected table or new data row appeared. Both clones have zero
invalid indexes or unvalidated constraints. Repeating `sqlx migrate run` after
completion is a no-op. Both containers are stopped again; their retained
sensitive plaintext volumes now hold schema 84. Original encrypted schema-76
backups remain unchanged.

Protected evidence: `homelab-schema84-receipt.json`,
`workshop-schema84-receipt.json`, both `*-schema84-inventory.json`,
`migration-source-manifest.json` and `schema84-rehearsal.json` under the backup
directory. Migration source manifest SHA-256:
`988e7a2dd773ac4ee404fb990729b25162c6d911ddc8df980a52cf088613913f`.
Combined rehearsal receipt SHA-256:
`710da295d6fca737ad2940068302b39933dfab5159b13dbaf9aa7d619d125bf4`.

This proves the SQLx schema upgrade and preservation of those two snapshot
datasets. It does not prove application runtime privileges, new company login,
background behavior, federation compatibility or a safe live writer-fencing
cutover. Those remain separate acceptance steps.
