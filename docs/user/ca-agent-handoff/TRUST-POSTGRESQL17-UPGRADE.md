# CA PostgreSQL 17 and Trust receiver handoff

24 September 2026. PostgreSQL 17 is the selected CA target. This document is a
reviewed change proposal, **not authority to execute live host changes**. The
existing PostgreSQL 15 cluster, `memory-layer.service`, Step CA and OpenBao stay
untouched until Olivier approves the final command-and-recovery packet.

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

## Read-only prerequisite packet

The CA agent should reply on the coordination page with one sanitized packet:

1. `pg_lsclusters` state with cluster name, major, port and running/stopped only.
2. OS release and configured APT source families, excluding credentials.
3. `apt-cache policy` candidates for `postgresql-17`, `postgresql-client-17`,
   `postgresql-server-dev-17` and the PostgreSQL-17 pgvector package.
4. Confirmation that cluster `17/trust`, port 5433 and the default proposed data
   path are unused.
5. Free space in the PostgreSQL data filesystem and backup filesystem, plus the
   size of every existing PostgreSQL cluster.
6. Whether package installation would auto-create `17/main`, and the exact
   local mechanism used to prevent an unwanted cluster from starting.

Do not add repositories, install packages, create a cluster or gain new catalog
access to answer this packet. `unavailable` is a valid result. Monolith must bind
the final package versions and hashes after this reply; a package name alone is
not enough.

## Grouped live-change proposal

After the packet is complete, the CA agent prepares one approval request with
the resolved commands and package hashes. It must include all of the following:

1. Take a fresh encrypted backup of the PostgreSQL 15 singleton plus globals,
   configuration metadata and the Memory offline store. Prove recovery in an
   isolated location. Do not stop Step CA or OpenBao.
2. Add only the reviewed package source if PostgreSQL 17 is not already
   available. Pin the exact PostgreSQL/client/dev/pgvector versions. Prevent
   package hooks from creating or starting an unreviewed `17/main` cluster.
3. Create only `17/trust`, with port 5433, Unix socket access and manual startup
   policy during preparation. Keep PostgreSQL 15 running on its existing port.
4. Create `memory_trust` as a login role with no superuser, createdb,
   createrole, replication or bypass-RLS capability. Create `memory_trust` owned
   by that role. Generate credentials locally into protected files; never place
   them in Notion, shell history or process arguments.
5. Validate exact server/client/extension versions and an otherwise empty
   database. Stop if any identity differs from the receiver manifest.
6. Transfer only the encrypted, hash-bound Trust receiver bundle. Decrypt it
   into a new private CA-local directory, run receiver `inspect`, then run its
   single-transaction restore. Retain the immutable receipt.
7. Leave `memory-layer@trust.service` absent or inactive. Do not point any
   service at the restored database and do not enable federation, watchers,
   models, MCP, offline replay or Pi.

If package installation, cluster creation or restore fails, preserve the exact
evidence and stop. A fresh, never-accepted Trust cluster can be removed only by
the rollback steps named in the approved packet. Never remove or downgrade the
PostgreSQL 15 singleton as rollback.

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

## Separate gates after inactive restore

The historical receiver bundle is not eligible for activation. Before a live
Trust service starts, complete renewed review for the 17 dependency-changed Trust
histories, reconcile CA-local/offline data, create a final writer-fenced capture,
and prove that only one instance with the Trust identity can write. Activation,
singleton PostgreSQL-15 migration and federation enrollment each need their own
explicit approval.
