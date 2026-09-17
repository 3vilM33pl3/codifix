# Protected four-area preparation

These operator tools do not cut over a deployment. Use Node 22 or newer,
PostgreSQL client tools compatible with the source server, GPG and an already
available pgvector Docker image. No provider calls are made.

## Encrypted baseline

Choose a **new**, protected absolute output directory on a sufficiently large
disk. Verify that the existing operator encryption key can be decrypted before
depending on it. The source configuration must have one explicit PostgreSQL
`[database].url`; its password never appears in command arguments or evidence.

```sh
node scripts/areas/snapshot.mjs \
  --database-config /protected/instance-config.toml \
  --out /protected/new-baseline \
  --recipient FULL_EXISTING_GPG_FINGERPRINT \
  --file /protected/instance-config.toml
```

The consistent custom-format database dump is streamed directly into encryption.
Selected regular files are archived and encrypted separately. Directories and
symlinks are refused. A complete `snapshot.json` contains ciphertext hashes, not
credentials. No complete manifest means failure: preserve the partial artifact
for diagnosis, do not label it a valid backup. Copy the encrypted artifacts and
manifest to independent storage and compare hashes. Protect migration backups
from ordinary rolling-backup cleanup.

This is an **online baseline**, not an atomic whole-host backup. Roles/grants,
system-only configuration, offline stores, external evidence and identities need
their own documented capture. Active offline databases require a supported
consistent export or a writer stop, not an arbitrary file copy. The final cutover
requires stopping/fencing every old writer and taking a new consistent capture.

## Isolated restore and row verification

Provision a new disposable PostgreSQL/pgvector container with all of:

- A name starting `memory-four-area-restore-`, a fresh `memory_restore` database,
  and explicitly allocated data storage, preferably off the root filesystem.
- `--network none`, no published ports, a read-only container root and writable
  PostgreSQL data/socket/temp mounts.
- A pinned already-reviewed image digest and no application processes, watchers,
  workers or credentials. Never attach a live database volume.

Then run:

```sh
node scripts/areas/restore-audit.mjs \
  /protected/new-baseline memory-four-area-restore-EXACT_SANDBOX
```

Only an empty sandbox is restored. Use `--verify-existing` solely to audit a
previously restored sandbox; it does not restore again. New captures pin UTC.
For a legacy archive without recorded timestamp rendering, supply the verified
source zone explicitly, for example `--archive-timezone Europe/London`.

The audit compares **all schemas**, including historical evaluation schemas,
using original COPY column lists, exact row bytes and sorted row-hash multisets.
No timestamp, credential, content or unknown field is ignored. Different row
order is harmless; changed values, lost duplicate rows and missing tables fail.
Mismatch diagnostics contain only table metadata/counts/hashes. A successful
exclusive `restore-evidence.json` records the image/container and comparisons.
It is not proof of application startup, role provisioning, sequence recovery,
external-file recovery or operational cutover. Sequence state is retained in the
database archive but is not independently compared by this row auditor yet.

Protect the restored database and inventories as sensitive data even though the
container has no network. They contain the complete original dataset and must
never become an application instance or a source for model prompts.

## Tests

```sh
node --test scripts/areas/snapshot.test.mjs scripts/areas/restore-audit.test.mjs
```

The first real restore evidence is recorded in
[`docs/reviews/four-area-network/README.md`](../../docs/reviews/four-area-network/README.md).
