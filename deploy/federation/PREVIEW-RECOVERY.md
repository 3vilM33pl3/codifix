# Isolated two-area preview recovery

This operator verifies an explicitly selected encrypted preview backup from the
NAS. It does **not** restart archived applications, the PDS server, a supervisor,
any model caller or a production writer. It does not prove complete customer
disaster recovery or perform a cutover.

## Commands and boundaries

Use the same private configuration and immutable receipt as
`preview-backup.mjs`, an explicit approved SHA-256, and a fresh directory directly
under `/data/tmp` named `memory-preview-recovery-<unique-suffix>`:

```sh
node deploy/federation/preview-restore.mjs extract BACKUP_CONFIG RECEIPT NEW_RECOVERY_DIRECTORY APPROVED_ARCHIVE_SHA256
node deploy/federation/preview-restore.mjs verify BACKUP_CONFIG RECEIPT NEW_RECOVERY_DIRECTORY APPROVED_ARCHIVE_SHA256
node deploy/federation/preview-restore.mjs proof BACKUP_CONFIG RECEIPT NEW_RECOVERY_DIRECTORY APPROVED_ARCHIVE_SHA256
```

All arguments are explicit. No live container discovery, credential provisioning,
image download, namespace replacement or writer shutdown occurs. The approved NAS
copy is hashed before decryption. Extraction accepts only complete configured
paths plus the three dumps, role dumps and backup intent; it rejects symbolic or
hard links, traversal, duplicates, unknown paths and oversized archives. The
extracted sensitive files and their hash inventory remain owner-only.

Each database gets a newly named volume and container using the locally available
PostgreSQL17/pgvector image
`sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180`.
Containers have `--network none`, read-only root filesystems, no published ports,
no restart policy and only their new data volume plus temporary memory mounts.
The initial database superuser is `postgres`; local trust authentication is
restricted to this no-network clone. Archive role passwords/grants are restored.
Only the known `CREATE ROLE postgres;` collision is omitted, never its `ALTER`,
password or grants. Other SQL errors fail closed.

Readiness requires the final PID1 postmaster as well as `pg_isready`, because the
image's temporary initialization server can otherwise create a readiness race.
After an interruption, `resume` may reuse only the exact recorded container ID,
image and volume. It does not clear partial databases or ignore SQL errors:

```sh
node deploy/federation/preview-restore.mjs resume BACKUP_CONFIG RECEIPT RECOVERY_DIRECTORY APPROVED_ARCHIVE_SHA256
```

An incomplete database must pass the same full verification or the recovery
fails; prepare a new isolated attempt instead of deleting evidence. A completed
`database-recovery.json` is immutable, so repeating a fully completed verification
does not overwrite its receipt. Previous per-database receipts must still match.

## What is verified

- Every schema's COPY row multiset, including duplicate rows, via the shared
  `archiveInventory` implementation.
- Sequence values and `is_called`, schema DDL, object ownership, grants, RLS,
  global roles and membership; no invalid indexes/unvalidated constraints.
- Member instance/space/authority bindings, receipt-key schedules, replica
  cursors, consistent accepted/published head, empty pending publication leases
  and preserved retained-answer hashes across origin and gateway stores.
- Every archived PDS SQLite database's integrity; the actor private key derives
  the enrolled public key rather than the discarded bootstrap key.
- A CAR reconstructed directly from archived actor SQLite blocks verifies under
  the pinned PDS SDK and enrolled key, and rejects the bootstrap key. Its exact
  publication keys and decoded record contents match the restored accepted log.
- The gateway private audit scalar derives its trusted public key. Every retained
  audit receipt verifies its domain-separated signature and previous-receipt hash.

The offline cryptographic helper runs in the pinned PDS image with no network,
read-only archive mounts, no capabilities and a bounded private tmpfs. It does
not start the PDS server. SQLite WAL readers need sidecars; the helper reads a
byte copy into its disposable tmpfs instead of altering the backup. No raw rows,
private keys, credentials, prompts or answer bodies are printed.

## Recorded verification — 2026-09-19

NAS backup SHA-256:
`c0ad3dbe68367de400e9881af2745fd5df462cb1ef28a60f4a5f4e65d73b772d`.
Recovery directory: `/data/tmp/memory-preview-recovery-20260919t2225`.
The original backup receipt remains unchanged with `restored: false`; separate
`database-recovery.json` and `operational-recovery.json` record this recovery.

| Restored database | Tables | Rows | Sequences |
| --- | ---: | ---: | ---: |
| Workshop | 114 | 5,160 | 4 |
| Homelab | 114 | 763 | 4 |
| Gateway | 114 | 162 | 4 |

All row/sequence/schema/role comparisons passed. Both participant cursors and the
accepted/published head are10, with no unpublished records, publication leases or
unsettled model attempts. Four retained origin answers match gateway answer
hashes (one Workshop, three Homelab). Four PDS SQLite databases passed integrity
checks. The reconstructed CAR has ten records, fourteen blocks and8,953 bytes;
all ten receipt signatures and the receipt chain verify.

CAR SHA-256:
`b23b173c3d4e47180eacf11678e18fdeb6602f43763ccc7a94979bb779d6ef2b`.
Repository commit:
`bafyreib4gmcji2coqxwnynfde65fwpkfhz3dqj6fw5f2p3cbfurymkxpc4`.

The three restore containers/volumes are retained with no network and no live
writers. Their exact identities are in the protected recovery directory. Treat
their data and the decrypted archive as sensitive; **never connect them to live
networks or start the archived configurations**. Retention/secure disposal is a
separate operator decision, not automatic cleanup.

This evidence covers the coordinated backup's point in time only. It does not
include later writes, prove online credential recovery, execute browser flows or
demonstrate a safe production writer-fencing cutover. Those remain separate gates.

## Tests

```sh
node --test deploy/federation/preview-backup.test.mjs deploy/federation/preview-restore.test.mjs
```

Pure tests cover path/link/scope rejection and exact handling of the known initial
role. The recorded isolated restore supplies the database and cryptographic
integration evidence. No paid providers or real customer deployments are used.
