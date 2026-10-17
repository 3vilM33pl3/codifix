# Codifix 3.0 federation cutover operators

These release-specific operators require Python 3, psycopg2 and an explicitly
selected `CODIFIX_CUTOVER_DATABASE_URL`. They never discover a database, enrol a
peer, load provider credentials or contact a gateway/PDS. Inspection is read-only.
Keep generated manifests local with restrictive permissions: they contain public
keys, identifiers, schema checksums and row counts/digests, never source payloads.

First upgrade the isolated rehearsal and target to the exact candidate schema85.
Create the schema reference from a pristine candidate database; this is independent
of the populated target being reviewed. Applied SQLx checksums must match the
candidate source. The schema fingerprint excludes catalog names and dropped physical column slots,
while retaining exact types, defaults, constraints, functions and triggers. New
migration85 explicitly qualifies two legacy UUID defaults; existing IDs stay intact.
Schema drift, extra tables/dependencies, changed rows or unknown
partial retirement markers cause rejection.

```sh
# Environment points at a pristine, disposable candidate database here.
python3 scripts/codifix-cutover/federation-retire.py schema-reference > schema-reference.json
# Select the populated, isolated target and its inventoried local instance.
python3 scripts/codifix-cutover/federation-retire.py inspect \
  --schema-reference schema-reference.json --local-instance LOCAL_INSTANCE_UUID > retirement.json
```

Review the exact `payload` and `sha256`. Before applying, stop and **drain** every
old process, watcher, scheduler, provider call and federation worker. Capture the
matched final recovery bundle. Only candidate binaries may restart afterwards;
old binaries do not know this fence. The flag records that operational prerequisite,
not automatic process cancellation or a distributed lock.

```sh
python3 scripts/codifix-cutover/federation-retire.py apply \
  --manifest retirement.json --review-sha256 EXACT_SHA256 --writers-stopped
```

The transaction locks the reviewed tables, rechecks schema/database/rows and
inserts immutable space markers. It disables old source subscriptions and laptop contribution bindings,
blocks unfinished source/publication work, invalidates leases and rotates the
reviewed gateway epoch. Statuses, origins, signed history, projections, content,
attachments, embeddings, canonical/auth/area IDs and old outbox bindings remain.
An exact immediate retry checks existing receipts and retired effects. Changed
manifests or later unexpected state changes fail; do not regenerate a manifest
from already retired state to bypass that check.

The trusted read-only `verify-retired-spaces` Rust example verifies each pinned
legacy receipt signature, purpose, sequence, chain, key schedule, final history
fingerprint and reducer projection. Service `/context` independently requires
current exact-project read permission and an enabled local Pull grant, even with
no active federation registry. Active ingestion never accepts the legacy signing
domain/purpose. Laptop subscription-addressed search/history retain old views;
new subscriptions never overwrite an existing projection.

For a laptop contribution rebind, prepare the freshly enrolled binding separately.
Keep the old subscription in the explicit laptop read configuration if its retained
view is needed. Inspect and review the one-time old/new mapping:

```sh
python3 scripts/codifix-cutover/laptop-rebind.py inspect \
  --schema-reference schema-reference.json --project PROJECT_UUID \
  --new-binding new-binding.json > rebind.json
python3 scripts/codifix-cutover/laptop-rebind.py apply \
  --schema-reference schema-reference.json --manifest rebind.json \
  --review-sha256 EXACT_SHA256 --writers-stopped
```

The rebind requires the exact disabled binding and retired-space receipt. Company,
area, logical owner-instance, project and device remain identical; the new space
must be active. The permanent receipt supports exact retries and rejects another
mapping. Canonical origin mappings and old queued/accepted/proposed payloads are
never moved or cleared, so corrections cannot silently become fresh publications.
