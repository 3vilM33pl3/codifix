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

## Remaining receiver gates

- Test the exact CA PostgreSQL/extension/client versions. PostgreSQL17 success
  does not establish a PostgreSQL15 or another-major import path.
- Approve an exact existing encryption recipient/decryption route and transport.
  The release-signing key has no encryption subkey and is not a data recipient.
- Authenticate the minimized artifact set and receiver tools independently.
  The rehearsal script is not an authorized remote installer.
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
```

The opt-in synthetic test uses a new isolated container, no provider or private
data. It covers real TOC parsing, exact row/sequence preservation, canonical
trigger ordering, source-only malicious DDL exclusion, rollback before commit,
nonempty-target rejection and an excluded other-database canary. It removes its
own container and fixtures. Under a sandbox that remaps `/` to an untrusted UID,
the private-path tests correctly fail: run them with real filesystem ownership;
do not loosen the ownership checks.
