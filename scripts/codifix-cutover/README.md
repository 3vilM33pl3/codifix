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

If normal company enrollment replaces the device as well as its retired space,
use the separate fresh-device mode after human approval and authenticated
subscription retrieval. It preserves the same company/area/owner/project and
requires every old contribution to be accepted, receipted and drained. Retain
the normal CLI subscription preview and independently reviewed company approval
as bounded absolute evidence files; their hashes are sealed into the manifest.

```sh
python3 scripts/codifix-cutover/laptop-rebind.py inspect --fresh-device \
  --schema-reference schema-reference.json --project PROJECT_UUID \
  --new-binding new-binding.json \
  --fresh-bootstrap /absolute/private/subscription-preview.json \
  --fresh-approval /absolute/private/company-approval.jsonl > fresh-rebind.json
python3 scripts/codifix-cutover/laptop-rebind.py apply --fresh-device \
  --schema-reference schema-reference.json --manifest fresh-rebind.json \
  --review-sha256 EXACT_SHA256 --writers-stopped
```

This operator records the transition in the existing immutable receipt table;
it does not change the same-device SQL function or applied migrations. Old
subscriptions, origin mappings and accepted receipts remain untouched. Enable
normal operational area launchers before restarting after this transition:
the retained cold-runtime helper deliberately rejects enabled contribution
bindings. Preserve its sealed inputs as recovery evidence.


## Mac preserved-runtime handover (3.0 only)

`macos-runtime-rebind.py` is a separate `inspect` / `bind` / `run` operator for
an already populated, renamed Mac root. It never applies an old onboarding
journal, changes its stages, initializes a cluster, creates a role/database,
refreshes credentials or obtains a subscription. The original `onboard.py`
remains create-only. See [the reviewed boundary and input contract](../../docs/reviews/codifix-rename/evidence/macos-runtime-rebind.md)
and [the reinstall gates](../../packaging/CODIFIX-3-REINSTALL.md#mac-preserved-runtime-binding).

Before inspection, retain the reviewed source bytes in a durable private bundle
outside both the development checkout and runtime roots. Preserve this layout:

```text
retained-3.0.0-rc.1/
  scripts/codifix-cutover/macos-runtime-rebind.py
  packaging/macos/onboard.py
  packaging/macos/onboarding_files.py
  packaging/macos/onboarding_native.py
```

Copy these four exact reviewed files before `inspect`; use current-user private
files (0400) and directories (0700), with trusted ancestors. Keep the candidate,
explicit Python 3.11+ executable and PostgreSQL/pgvector distribution at durable
protected paths too. Resolve the Python executable to its real path, because
symlinked operator/data/prerequisite paths are refused. Do not run from a mutable
developer checkout or move this bundle when renaming `/Projects/memory` last.
The manifest pins all four sources, Python, candidate, PostgreSQL tools and
pgvector files. Any later replacement or relocation fails validation.

Create the exact private input and reconciliation files from independently
reviewed inventory, using the contract in the evidence document. Archive the
original journal, receipt and every original plist byte-for-byte outside the
new root. Stop/drain every application, watcher and sync worker and unload all
original jobs. Keep their exact labels/ports and prior states in the inventory;
a loaded job, including an inactive loaded job, is refused. Complete the exact
filesystem/database/role/configuration/pgpass rename and preservation checks
first. Do not fabricate an archive or change an incomplete journal to `done`.

`inspect` and `bind` require the renamed cluster running **only for maintenance
queries**, with all old jobs and application writers stopped. That bounded
maintenance startup is a separately reviewed native cutover step, using the
pinned existing PostgreSQL against the populated cluster, loopback/SCRAM only,
no UNIX socket or preload libraries, and reviewed private configuration. It
must not use a retired wrapper or run onboarding. No new database is created.
After binding, stop that maintenance PostgreSQL cleanly before starting its
new reviewed launcher. This helper never starts/stops PostgreSQL during bind.

```sh
umask 077
# PYTHON and BUNDLE are the retained real absolute paths described above.
"$PYTHON" -I -S -B "$BUNDLE/scripts/codifix-cutover/macos-runtime-rebind.py" \
  inspect --input "$PRIVATE_INPUT" > "$PRIVATE_MANIFEST"
# Independently review payload, archive/reconciliation proof and exact digest.
"$PYTHON" -I -S -B "$BUNDLE/scripts/codifix-cutover/macos-runtime-rebind.py" \
  bind --manifest "$PRIVATE_MANIFEST" --review-sha256 "$REVIEWED_SHA256" \
  --writers-stopped
```

Bind creates only its `.codifix-runtime-3.0.0-rc.1` managed files, private intent,
lock and final receipt. Configs enforce read-only services, verify-only
migrations and disabled automation/offline/background/MCP/cluster/federation.
Old subscription/cache IDs remain for historical read views. There is no sync
or watcher launcher. Every plist is inert (`Disabled=true`, `RunAtLoad=false`,
`KeepAlive=false`); bind does not bootstrap any job. Activation remains a
separate reviewed native gate.

The new PostgreSQL `run --kind postgres` works with PostgreSQL stopped: it
rechecks complete receipt/config/pins, old-job absence and `pg_controldata`'s
stable cluster system identifier, then executes pinned PostgreSQL on the
populated cluster. It does not query a running server or initialize anything.
After PostgreSQL is ready, each `run --kind area --area AREA` additionally
rechecks live cluster/database/role/project/migration/cache metadata before
executing the exact candidate with its managed config and clean environment.
Direct sync/watch/provisioning selections are rejected.

Retry only the same reviewed manifest. Partial bind never runs, files are
create-without-replace, and the final receipt is written last. Conflicting or
unknown files, symlinks, altered pins/config/credentials/receipt, wrong OIDs or
cluster identity cause refusal and preserve existing bytes. A crash leaving an
unpublished temporary file requires manual inspection; do not delete/adopt it
to force success. An uncertain result requires inspection of the intent,
managed manifest and final receipt before retry. Retain the old recovery set.
