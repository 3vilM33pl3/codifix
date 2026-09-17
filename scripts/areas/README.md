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

## Complete partition inventory

The current collector is deliberately restricted to the reviewed
`memory-four-area-restore-20260917` sandbox and `memory_restore` database. It does
not accept a live connection URL. After successful restore verification:

```sh
node scripts/areas/inventory.mjs \
  --source-instance OBSERVED_SOURCE_INSTANCE_UUID \
  --snapshot-id NEW_SNAPSHOT_UUID \
  --output /protected/new-baseline/partition-inventory.json
```

The parent directory must be owned by the operator with mode 0700. Both the
inventory and its `.collector.json` evidence sidecar are created exclusively at
0600. One read-only repeatable-read transaction streams all table rows and
reconciles counts. Full-row hashes cover PostgreSQL's complete JSONB value
serialization in UTC; bigint keys are never rounded through JavaScript numbers.
The evidence sidecar binds the file digest and records its precise serialization.
Unrecognized schema is retained for review, not omitted. Tables without primary
keys, foreign tables, broken database foreign keys, oversized envelopes and
incomplete streams fail closed. No raw row bodies or database errors are printed.

The snapshot's source instance is an observed source binding, **not** a claim
that it authored every historical row. Existing authorship remains in the
protected source records. Public and historical project identities stay distinct.

Pass the complete inventory to `memory-area inventory`, then `plan` and `verify`
as described in [the planner contract](../../crates/mem-area/README.md). Keep
reports and unresolved drafts private. On the real source, the inventory is
approximately 1 GB; do not open or parse the whole file in a browser. Collector
streaming is bounded, but the Rust planner still materializes its input and
requires enough memory. Structural validity is not a reviewed area assignment,
and is not permission to import. Opaque links embedded in content
or JSON require separate review; catalog FK coverage alone cannot prove them.

## Exact row payloads and protected preparation

`payload.mjs` uses the same pinned isolated restore and private-file controls.
It exports every original row as bounded NDJSON, ordered by table and exact
UTF-8 primary-key text. Complete PostgreSQL JSON bytes are retained without
JavaScript numeric conversion. The stream includes empty tables and a final
count marker only after PostgreSQL exits successfully.

```sh
node scripts/areas/payload.mjs \
  --source-instance OBSERVED_SOURCE_INSTANCE_UUID \
  --snapshot-id INVENTORY_SNAPSHOT_UUID \
  --inventory-sha256 NORMALIZED_INVENTORY_SHA256 \
  --output /protected/new-baseline/partition-payload.ndjson
```

The digest is a claimed source binding, not a verification result. Use
`memory-area-import verify-payload --inventory /protected/inventory.json
--payload /protected/payload.ndjson --inventory-sha256 INVENTORY_SHA` for an
independent read-only comparison before ownership review. It emits metadata only
and grants no import authority. The protected
[`memory-area-import` workflow](../../crates/mem-area-ops/README.md) independently
verifies all row identities, hashes, ownership decisions and reviewed provenance
before preparing an archive or connecting to an explicit inactive local target.
Do not expose the payload/archive to retrieval, source control or model inputs.
Same-user/root processes remain trusted; directory ancestry is checked and shared
writable non-sticky or symlink ancestors are refused. File and parent-directory
synchronization precede successful publication. No live service is touched.

## Tests

```sh
node --test scripts/areas/*.test.mjs
```

The first real restore evidence is recorded in
[`docs/reviews/four-area-network/README.md`](../../docs/reviews/four-area-network/README.md).
