# Trust-only relocation preparation (local rehearsal)

These tools prepare evidence, **not a CA installer or permission to deliver data**.
They never select a live restore destination. The four-area Rust importer remains
unchanged: it still requires the complete protected source on Monolith.

## 1. Derive private Trust expectations

Use the existing exact four-target staging contract and independently verify the
Trust target against the full prepared archive, keeping that archive local:

```sh
node scripts/areas/trust-expectation.mjs \
  --contract /protected/staging-verification-contract.json \
  --contract-sha256 EXACT_CONTRACT_SHA256 \
  --output /protected/new-trust-expectation.json
```

The output contains only Trust row/receipt hashes, IDs and archive-derived counts.
It is nevertheless **private**: composite tag keys contain tag text. Do not post
it to Notion. Exact bytes must be independently authenticated before use; decoding
JSON or checking its self-reported digests does not authenticate it. Counts and
tombstones are bound to original archive observations, not re-derived from hashes.
The required schema hash is an expectation, not source-function authentication.

## 2. Rehearse with trusted schema, not source executable definitions

```sh
node scripts/areas/trust-transfer-rehearsal.mjs \
  --contract /protected/staging-verification-contract.json \
  --contract-sha256 EXACT_CONTRACT_SHA256 \
  --expectation /protected/new-trust-expectation.json \
  --expectation-sha256 EXACT_EXPECTATION_SHA256 \
  --schema-tool /absolute/reviewed/memory-area-import \
  --source-archive /absolute/frozen-release/memory-source-b35faf2983d20c69bc086fb9bcb7e95c51bd8f47.tar \
  --output /protected/NEW-rehearsal-directory
```

This deliberately pins the reviewed schema60 helper, frozen source archive and
already installed PostgreSQL17 image; it does not discover/download substitutes.
The helper is the separately hashed local staging tool, **not a binary included
in the signed ARM64 release**. The frozen source is separately hash-pinned.
The compiled schema identity is recomputed from all 60 original migration
checksums and executable SQL, including the established migration4 compatibility
rule. Every restored migration checksum is then compared with those originals.

The script:

1. Revalidates exact source container, Unix target descriptor and inactive rows.
2. Exports a data-only archive and requires exactly 114 allowlisted table-data
   entries plus four sequence states. Extra schema, functions, large objects,
   foreign tables, duplicate entries or unrecognized objects fail closed.
3. Creates a new network-none, read-only-root PostgreSQL sandbox with ephemeral
   data. Its only bind is a new private socket directory, never a live volume.
4. Initializes a separate reference database with the pinned migration helper.
5. Restores **reference pre-data + Trust data + reference post-data**, in one
   transaction guarded against nonempty targets. It never disables triggers,
   executes source DDL, edits receipts, uses `--clean`, or starts Memory.
6. Independently verifies the clone and original Trust target; preserves the
   logical target-instance/import UUIDs, row hashes, references and sequence state.
7. Removes only its newly created ephemeral container, leaving private evidence
   and candidate files. Failure artifacts remain; no failed run is deliverable.

Successful status is `local_trust_restore_verified_not_deliverable`. The data
dump, expectations and local evidence stay protected. The full source archive is
not part of a future Trust transfer. The four-area source is not locked by this
rehearsal; continued inactive-target fencing and destination verification remain
mandatory. The original staging target and a relocated copy must never both be
activated as the same logical instance.

## 3. Build the PostgreSQL-17 receiver bundle

The receiver bundle is a separate minimized format. It contains the reviewed
Trust data and expectations plus schema rendered from the independently pinned
migrations. It does not contain the all-area archive.

```sh
node scripts/areas/trust-receiver-bundle.mjs \
  --data /protected/rehearsal/trust-data.pgdump \
  --data-sha256 EXACT_DATA_SHA256 \
  --selection /protected/rehearsal/data-selection.list \
  --selection-sha256 EXACT_SELECTION_SHA256 \
  --expectation /protected/trust-expectation-v1.json \
  --expectation-sha256 EXACT_EXPECTATION_SHA256 \
  --schema-tool /absolute/reviewed/memory-area-import \
  --source-archive /absolute/frozen-release/memory-source-b35faf2983d20c69bc086fb9bcb7e95c51bd8f47.tar \
  --output /protected/NEW-receiver-directory
```

The script requires the exact pinned PostgreSQL17.11/pgvector0.8.6 image, restores
the candidate into an isolated reference target and emits `bundle/manifest.json`
only after independent verification. Its success status is
`verified_inactive_unencrypted_not_transferred`. The bundle is private and
unencrypted; do not put it on USB, Notion or the CA until it is sealed to the
separately approved CA recipient.

The 24 September historical rehearsal produced manifest SHA-256
`10dd74498e9fcb11abcfd81f22ce6c086731258722def4001895211d9a63a057`
with 375 verified rows and 185 projected sources. Those exact bytes prove the
receiver path but are not a current-data approval.

## 4. Inspect and restore on the CA

The authenticated target descriptor is a private mode-0600 JSON file:

```json
{
  "format": "memory-trust-receiver-target-v1",
  "connection": {"kind": "unix", "directory": "/var/run/postgresql", "port": 5433},
  "database": "memory_trust",
  "admin_username": "postgres",
  "owner_role": "memory_trust"
}
```

First run read-only inspection from a private directory, substituting only
independently verified digests:

```sh
/usr/bin/python3 -I -S /usr/libexec/memory-layer/trust_receiver.py inspect \
  --bundle /private/received/bundle \
  --manifest-sha256 EXACT_MANIFEST_SHA256 \
  --target /private/receiver-target.json \
  --target-sha256 EXACT_TARGET_SHA256
```

Only after the grouped restore approval, run:

```sh
/usr/bin/python3 -I -S /usr/libexec/memory-layer/trust_receiver.py restore \
  --bundle /private/received/bundle \
  --manifest-sha256 EXACT_MANIFEST_SHA256 \
  --target /private/receiver-target.json \
  --target-sha256 EXACT_TARGET_SHA256 \
  --receipt /private/evidence/trust-restore-receipt.json \
  --execute --writers-stopped --fresh-database
```

`writers-stopped` attests that no application is connected to the new Trust
database; it does not require stopping the unrelated PostgreSQL15 singleton.
The receiver refuses a nonempty target, a second restore, altered artifacts,
version/extension drift and additional database clients. It never provisions
packages, clusters, roles, databases, credentials or services.

## Remaining receiver gates

- Resolve and approve exact CA PostgreSQL17/client/dev/pgvector packages and a
  side-by-side `17/trust` cluster without disturbing PostgreSQL15.
- Approve and create a dedicated CA-local encryption recipient/decryption route
  and transport. Release signing is not a data recipient.
- Authenticate the minimized artifact set and receiver tools independently.
  The receiver neither installs PostgreSQL nor authorizes a restore.
- Provision the dedicated CA-local DB/role/paths under one approved inactive
  host-change plan, then run receiver-side integrity, sandbox and restore checks.
- Complete fresh-history/dependency review and reconcile CA-only offline data.
  Reference-only provenance does not restore the original evidence bytes.
- Obtain separate final writer-fencing/cutover approval. New local observations
  while old writers run do not establish a current final dataset.

## Tests

```sh
node --test scripts/areas/*.test.mjs
MEMORY_AREA_TEST_DOCKER=1 node scripts/areas/trust-transfer-restore.test.mjs
MEMORY_AREA_TEST_PG15=1 node --test scripts/areas/trust-receiver-pg15.test.mjs
MEMORY_AREA_TEST_RECEIVER=1 \
  MEMORY_TRUST_RECEIVER_BUNDLE=/protected/receiver/bundle \
  MEMORY_TRUST_RECEIVER_MANIFEST_SHA256=EXACT_MANIFEST_SHA256 \
  node --test scripts/areas/trust-receiver.test.mjs
```

The opt-in synthetic test uses a new isolated container, no provider or private
data. It covers real TOC parsing, exact row/sequence preservation, canonical
trigger ordering, source-only malicious DDL exclusion, rollback before commit,
nonempty-target rejection and an excluded other-database canary. It removes its
own container and fixtures. Under a sandbox that remaps `/` to an untrusted UID,
the private-path tests correctly fail: run them with real filesystem ownership;
do not loosen the ownership checks.
