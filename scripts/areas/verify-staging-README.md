# Read-only verification of inactive staging imports

Run this **after** all four approved imports, with writers still excluded. It
does not run an importer, migrate, start a service or create credentials. It
supports explicit schema-0060 and schema-0061 reference-only projection profiles.
Other schemas, imported raw context and reviewed excerpts require a separate
review. Existing active instances use the reconciliation operator and are never
accepted as inactive staging databases.

Create a mode-0600 contract in a mode-0700 directory. Bind exact file SHA-256
values, the import operation IDs and Docker **image IDs**, not image tags or
repository digests. Every area has its own independently initialized target.
The four entries below are illustrative; never copy an example identity into
real import authority.

```json
{
  "schema_version": 1,
  "kind": "four_inactive_staging",
  "prepared": { "path": "/protected/prepared/prepared.json", "sha256": "EXACT_FILE_SHA256" },
  "manifest": { "path": "/protected/reviewed-manifest.json", "sha256": "EXACT_FILE_SHA256" },
  "archive": { "path": "/protected/prepared/archive.ndjson", "sha256": "EXACT_FILE_SHA256" },
  "targets": [
    {
      "area": "workshop",
      "target": { "path": "/protected/workshop/target.json", "sha256": "EXACT_FILE_SHA256" },
      "container_id": "EXACT_64_HEX_CONTAINER_ID",
      "container_name": "memory-four-area-staging-REVIEWED_SUFFIX-workshop",
      "image_id": "sha256:EXACT_64_HEX_IMAGE_ID",
      "operation_id": "EXACT_IMPORT_OPERATION_UUID"
    }
  ]
}
```

Supply equivalent entries for `homelab`, `trust` and `general`—exactly four
targets are mandatory. Target files retain the existing private importer format:
Unix socket connection, PostgreSQL user `postgres`, null password and database
`memory_staging_AREA`. The socket directory must be the exact bind mounted at
`/var/run/postgresql` in its network-none, read-only-root, nonprivileged container.
Public ports, host namespaces, extra capabilities, duplicate identities and a
source-instance UUID reused as a target are rejected before any database read.

```sh
node --max-old-space-size=4096 scripts/areas/verify-staging.mjs \
  --contract /protected/staging-verification-contract.json \
  --contract-sha256 REVIEWED_CONTRACT_FILE_SHA256 \
  --output /protected/new-staging-verification.json
```

The manifest is bounded at 256 MiB. The full protected archive is streamed, not
loaded into memory; it may take time to hash a large snapshot. PostgreSQL output
contains only row digests, receipt metadata and counts, never knowledge text or
provider secrets. SQL and database errors are suppressed. An existing output
cannot be overwritten. A partial or absent report is not success evidence.

Verification independently derives selected rows from the digest-bound
materialized archive and exact manifest. It checks every original row hash,
complete canonical/state/project membership, source projection, same-area
relation endpoint, import receipt and per-source decision binding. The fixed
114-table schema-0060 or 115-table schema-0061 allowlist fails on
extra/missing/foreign/partitioned tables.
Every runtime, authentication, federation, supervisor policy and derived-data
table must be empty. Only schema migration history, import receipts and the
exact inert seed counts are permitted; the loop kill switch must be on, timeline
position zero and supervisor revisions null. Concurrent database clients,
invalid indexes, invalid constraints and broken canonical relations fail closed.
All four schema-0060 sequences must have their exact names, owners and default
bigint configuration, with `last_value=1` and `is_called=false`. This profile
imports no access events, source checks, timeline events or supervisor journal
rows. The importer's transactional restart of the empty source-check sequence
must therefore also yield 1, uncalled. Extra, missing, advanced or reconfigured
sequences fail verification without calling `nextval` or changing counters.
This is independent from the original snapshot's preserved sequence values.

## Explicit rc.2 schema selection

Version-1 fleet contracts and version-2 subset contracts stay pinned to
schema-0060. They reject a schema-0061 database rather than automatically adopting
the observed head. A new version-3 contract retains the same `kind`, artifact
bindings and target entries, and adds exactly:

```json
{
  "schema_version": 3,
  "schema_plan": {
    "path": "/protected/rc2-schema-plan.json",
    "sha256": "EXACT_SCHEMA_PLAN_FILE_SHA256"
  }
}
```

The additional file is the exact JSON output of the reviewed rc.2
`memory-area-import schema-plan`. Its head, count and executable-migration digest
must match the reviewed profile: head `61`, count `61`, digest
`e502de46fba6435667e2562b2e1758d262c33dc86952e3ec1c0b6616cf539576`.
The plan must report no credential creation or service startup. This binds
the historic migration checksums and exact compatibility SQL; it does not claim
independent inspection of every deployed PostgreSQL function or trigger.

Schema-0061 adds only `area_reconciliation_receipts` to this verifier's table
catalog. That table must remain empty for a fresh inactive staging target;
import receipts must identify target migration `61`. Its sequences remain the
four exact schema-0060 sequences. Schema-0060 sources can be imported into a
schema-0061 target under this explicit contract.

Trust expectations derived from a version-3 schema-0061 contract use
`memory-trust-expectation-v2`. Legacy v1 expectations retain their original
schema-0060 digest and format. The historical CA receiver v4 and its frozen
schema/tool pins remain schema-0060 only. Use the reviewed existing-target
reconciliation input and journaled upgrade procedure for live Trust; do not
replace its database with a fresh receiver dump.

The result is point-in-time logical-row evidence, not application readiness,
cryptographic authentication of reviewers, customer disaster recovery or
permission to cut over. Continue to fence writers after verification. The
full encrypted backup and independent restore sequence evidence remain required.
Exact import replay and changed-input denial must still be tested through the
import operator; this read-only helper does not substitute for those tests.

Application checks must use separately approved disposable validation clones,
never these still-inactive import targets. A clone needs its own database,
credentials and instance identity, with no provider credentials, watchers,
enrichment, federation workers or outbound network. Starting a service creates
runtime administration records and intentionally makes a target unsuitable for
inactive import replay. Do not copy that activation back into staged partitions.

Tests:

```sh
node --test scripts/areas/verify-staging.test.mjs
MEMORY_AREA_TEST_DOCKER=1 node --test scripts/areas/verify-staging.test.mjs
```

The opt-in Docker test uses an already installed `pgvector/pgvector:pg17` image
and `target/debug/memory-area-import` (override with
`MEMORY_AREA_IMPORT_BINARY`). It creates one network-none synthetic PostgreSQL
container, initializes schema without starting Memory, verifies complete row and
projection hashes, checks unchanged database state, and rejects a subsequently
disabled kill switch. It removes only its own disposable container/directory.
