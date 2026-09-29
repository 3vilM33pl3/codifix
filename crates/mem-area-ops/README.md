# Private area import operator

`memory-area-import` is an opt-in local operator executable, not a service route
or an automatic deployment. It never discovers configuration, connects to a
source database, creates credentials or starts workers. `initialize-schema` is
a separate explicit operation for an empty, independently provisioned target;
ordinary inspection/preparation/import never runs migrations.

## Inputs and preparation

Use an existing mode-0700 directory owned by the operator; all input files must
be owned mode-0600 regular files with one link. Symlinks (including ancestors),
shared writable non-sticky ancestors, changed inputs and existing outputs fail
closed. Root and the same operator UID remain trusted. Do not run this tool
against directories another process under that UID is modifying.

### Explicit reviewed preview selection (offline)

For a bounded preview, `preview-manifest` accepts a protected selection supplied
by the operator, not project-name heuristics or generated approvals:

```sh
memory-area-import preview-manifest \
  --inventory /protected/inventory.json \
  --selection /protected/selection.json --selection-sha256 SELECTION_FILE_SHA \
  --output-dir /protected/new-preview
```

It opens no database or network connection and prints no success data to stdout.
The directory must already exist, owned mode 0700. It exclusively publishes
`preview-manifest.json` and `preview-receipt.json` as mode-0600 files, with file
and directory synchronization. Existing final or `.partial` files block replay;
an interruption may leave private partial artifacts, which are not import
authority. A manifest without its receipt is an incomplete operator result.
Do not run the helper while another same-UID process modifies its inputs.

The strict version-1 selection shape is defined by `preview::Selection` in
[src/preview.rs](src/preview.rs):

```text
format_version: 1
source_instance, snapshot_id: exact source UUIDs
inventory_sha256: normalized complete inventory digest
area: trust | homelab | workshop | general
project_id: original project UUID (not its slug)
review:
  explicitly_approved: true (required operator assertion, no default)
  operator, reviewed_at, authority_basis
  source_evidence, project_evidence, content_evidence
lineages[]:
  canonical_id
  canonical_state_sha256: inventory's complete canonical-state row hash
  versions[]: EVERY inventoried version, including tombstones
    id, version_no, is_tombstone
    row_sha256: inventory's complete version-row hash
    canonical_text_sha256: SHA-256 of exact UTF-8 canonical_text
    row_json: exact complete source row JSON encoded as a JSON string
sources[]: EVERY source attached to the selected lineages
  source_id, row_sha256, task_id, task_row_sha256
  decision_id, approval_record_id: supplied stable non-nil unique UUIDs
  explicitly_approved: true (required per-source assertion)
```

All fields are required; unknown fields are rejected. Full-row hashes use the
inventory collector's exact complete-row JSON serialization; the text hash
includes its original whitespace. Selected complete-row strings are needed to
verify the content hashes rather than merely trust a content-hash assertion.
They remain in the private selection and are not copied into the manifest or
receipt. The selection is capped at 16 MiB, 64 lineages, 256 versions per lineage,
4096 sources and 8 MiB per complete version row. Review strings must be nonempty
and at most 4096 bytes. The normalized inventory digest independently binds all
unselected evidence; separate payload verification/preparation is still required.

The helper retains the original project and the entire selected histories.
Unselected lineages, raw sessions/tasks/captures and individually reviewed
records are excluded into protected archive, with short explicit evidence that
no completed content review is claimed. Existing mandatory historical, derived
rebuild and retired-runtime table policies remain intact; no original row is
discarded or runtime reactivated. Selected source checks stay archived.

Every selected source becomes only `ArchiveReferenceOnly`, bound to the exact
version/source/task hashes and supplied decision identifiers. A null/missing
task, typed memory target, missing source review, partial version set, changed
content/project/snapshot/hash, duplicate approval ID or unknown relationship
blocks output. No original source excerpt, path, task or session context is
copied. A no-task source needs a separate reviewed contract, not a fallback.

Review labels and digests **do not authenticate a human**, establish an approval
ledger, or grant remote/model disclosure consent. The receipt binds the exact
selection/inventory files and normalized manifest digest and explicitly reports
`authenticated_human_approval: false`, `full_partition_complete: false`, and
`remote_or_model_consent_granted: false`. Delegated engineering review must be
labelled honestly in the supplied operator/authority fields. This command does
not approve or execute import, activation, retrieval exposure or model calls.

1. Preserve the encrypted backup, restoration evidence and metadata inventory.
   Complete the ownership/provenance review in a separate manifest. A computed
   digest or syntactically valid manifest is **not** human approval.
2. Run `memory-area-import inspect --inventory /protected/inventory.json
   --manifest /protected/reviewed-manifest.json`. Record the normalized
   `inventory_sha256`, `manifest_sha256`, `plan_valid` and blocker count. This is
   read-only; it does not inspect row payloads or connect to PostgreSQL.
3. Collect exact row payloads from the already isolated restore with
   `node scripts/areas/payload.mjs --descriptor /protected/restore-descriptor.json
   --source-instance UUID --snapshot-id UUID
   --inventory-sha256 INVENTORY_SHA --output /protected/new-payload.ndjson`.
   This collector is bound to the explicitly reviewed isolated restore container
   descriptor, not an arbitrary server. It uses a read-only transaction, exact UTF-8 JSON
   bytes and lexical key ordering. The supplied inventory digest is only a
   claimed binding; preparation independently checks every row against it.
   Before ownership review is finished, independently validate that export:

   ```sh
   memory-area-import verify-payload \
     --inventory /protected/inventory.json \
     --payload /protected/new-payload.ndjson \
     --inventory-sha256 INVENTORY_SHA
   ```

   This reads protected files only and checks all tables, row bytes/hashes,
   identities, columns, references, ordering and complete counts against the
   inventory. The metadata reports both exact file hashes and retained notice
   counts. No manifest, database connection, output archive, partition or
   approval is created. Failure produces no partial success report. Passing is
   source integrity evidence, not a replacement for preparation or review.
4. In a new empty private output directory, run:

   ```sh
   memory-area-import prepare \
     --inventory /protected/inventory.json \
     --manifest /protected/reviewed-manifest.json \
     --payload /protected/new-payload.ndjson \
     --inventory-sha256 INVENTORY_SHA --manifest-sha256 MANIFEST_SHA \
     --output-dir /protected/prepared
   ```

   The tool publishes `archive.ndjson` and `prepared.json` exclusively, after
   file and directory synchronization. The archive contains **every original
   row**, including private raw context and retired runtime state. It must not
   enter a target's retrieval corpus or a repository. The receipt is metadata
   only and binds the exact input files, archive and per-area partitions.
   Retain its printed `prepared_sha256`; back up the archive securely and verify
   that backup before importing. Temporary files retained after interruption are
   not authority to import; inspect them and use a new output directory.

### Explicit retained-target budget for larger snapshots

Preparation and import default to **256 MiB** of retained target payload and
counted metadata across all four areas. A large reviewed snapshot may exceed
that ceiling even though every individual row is valid. The static error
`retained target payload exceeds its materialization limit` identifies this
case without printing source data. Other materialization errors remain errors;
a larger budget is not a workaround for failed validation.

After checking available memory, explicitly add
`--max-retained-target-mib 512` to **both** `prepare` and `import`. Accepted
integer values are 256 through 1024 inclusive; zero, smaller, larger and invalid
values fail before opening inputs or creating partial output. There is no
unbounded option. The full materializer still retains the inventory, indexes
and parsing buffers separately: this budget is not a total-process RSS limit.
The per-envelope 64 MiB cap, source-file bounds and every row/schema/hash check
remain unchanged.

The format-1 `prepared.json` receipt records `max_retained_target_mib`, included
in its exact file hash. Import must repeat the same value; changing or omitting
a non-default budget fails before archive rematerialization or any database
connection. Older receipts without this field remain readable and mean 256 MiB.
Budget changes do not change the archive or per-area partition digests for the
same validated inputs. Preserve failed `.partial` artifacts as protected
evidence and use a fresh output directory when preparing with a larger budget;
they cannot be resumed or used for import.

## Explicit target and import

Provision an independent local target with the exact supported migrations
(explicitly supported heads 0056 through 0061), no application/worker connections and no prior knowledge,
credentials, jobs or federation authority. Migration and writer fencing are
separate operator steps. A target UUID is a binding, not proof of host identity.

Create a private target descriptor using the intended credentials, never through
command arguments or chat. Its strict shape is:

```json
{
  "format_version": 1,
  "instance_id": "REPLACE_WITH_NEW_TARGET_UUID",
  "area": "general",
  "connection": { "kind": "loopback", "port": 5432 },
  "database": "REPLACE_WITH_FRESH_TARGET",
  "username": "REPLACE_WITH_TARGET_OPERATOR",
  "password": null
}
```

`password: null` is only appropriate for intentionally configured local
authentication. An explicit Unix socket is also supported with
`{"kind":"unix","directory":"/absolute/socket/directory","port":5432}`.
Remote hosts, URLs, `.pgpass`, application configuration and all `PG*`
environment overrides are excluded. Loopback/socket authentication and process
isolation must already be secured by the operator. Hash the descriptor with
`sha256sum`; do not print its contents.

### Schema-only preparation, before importing

Do **not** start Memory merely to apply migrations: normal service startup also
creates administration and loop definitions, which correctly makes a target
ineligible for fresh import. For a newly provisioned **empty** dedicated database,
use the reviewed executable's `memory-area-import schema-plan` first. It makes
no connection and prints the exact compiled schema digest and fixed head 0060.
Review that digest against the selected release, then explicitly run:

```sh
memory-area-import initialize-schema \
  --target /protected/target.json --target-sha256 TARGET_FILE_SHA \
  --schema-sha256 REVIEWED_SCHEMA_SHA \
  --execute --writers-stopped --fresh-database
```

The flags are operator attestations, not proof of fencing or target ownership.
All application clients must be disconnected; the command does not kill them.
It refuses existing tables (even empty), functions/types/views, other user
schemas, large objects, event triggers, foreign servers and non-default
extensions. The PostgreSQL server must already support pgcrypto, pgvector and
pg_trgm; installing server extensions or creating database roles is a separate
operator step. Standard `plpgsql` in an otherwise empty database is allowed.

All 60 migrations and their history run in one transaction with bounded waits;
the established migration-4 pgvector compatibility rule retains its original
checksum. Required-DB tests inject a failure at migration 55 and confirm that
earlier tables, history and extension creation roll back. No credentials, loop
definitions, jobs, imports or supervisor identities are bootstrapped.

Expected status: `fresh_schema_initialized_only_not_imported_or_activated`.
This is not a target identity binding or permission to start the service. After
an interrupted/ambiguous result, inspect the target: an initialized database is
deliberately refused on replay. Never drop/reinitialize it to force a retry. A
confirmed pristine supported schema may proceed to the separately reviewed
import below. No existing database is upgraded by this command.

### Reviewed import

Once the target schema and archive are independently verified, run:

```sh
memory-area-import import \
  --inventory /protected/inventory.json \
  --manifest /protected/reviewed-manifest.json \
  --payload /protected/new-payload.ndjson \
  --inventory-sha256 INVENTORY_SHA --manifest-sha256 MANIFEST_SHA \
  --prepared /protected/prepared/prepared.json \
  --prepared-sha256 PREPARED_FILE_SHA \
  --target /protected/target.json --target-sha256 TARGET_FILE_SHA \
  --operation-id STABLE_IMPORT_UUID \
  --execute --writers-stopped --archive-backed-up
```

The last three flags are explicit operator attestations, not checks that backups
or fences exist. All original inputs and the full archive are reverified before
connecting. Only the selected area's opaque validated partition is imported.
The transactional importer retains its fresh/inactive/schema/identity checks;
automation remains disabled. No auth tokens or provider settings are restored.

Save the metadata receipt. After an ambiguous acknowledgement, reuse the exact
operation ID, inputs and descriptor: an inactive target is reconciled against
all stored rows and receipts rather than blindly imported again. Activated or
changed targets cannot be replayed. Never invent a new operation to bypass a
failure. Errors suppress SQL, row contents, credentials and source paths.

## Verification and limits

Required-database tests invoke the real binary using generated synthetic
databases and protected temporary files. They cover preparation, exact hashes,
artifact tampering, private-file controls, inactive-target replay, complete
archives and reviewed projections. They do not establish human approval,
encrypted off-host durability, host identity, live writer fencing, deployment
activation or CA compatibility. Real-data import remains blocked until those
gates and the reviewed ownership manifest are complete.

## Reconcile an existing area

Use `memory-area-import reconcile` with the same `--inventory`, `--manifest`,
`--payload`, digest, `--prepared`, target descriptor and `--operation-id`
arguments as import. The target must already be on migration 0061 and have an
immutable matching area/instance binding. The original `import` command still
refuses active or populated installations.

Without `--execute`, reconcile opens a read-only repeatable-read snapshot and
prints `{ "preview_sha256": "...", "preview": {...} }`. Save the nested
`preview` object as a private 0600 JSON file in an owned 0700 directory. Inspect
each row's `conflict` field; any conflict prevents execution. This preview may
run while the existing service is available, but changes invalidate it.

After external writer fencing, regenerate the final preview and invoke the
same command with `--execute --writers-stopped --archive-backed-up`,
`--preview /private/preview.json` and `--preview-sha256 DIGEST`. The digest is
the reported normalized preview digest, not the file's whitespace-dependent
hash. Keep the same operation ID for recovery after an ambiguous acknowledgment.
The operator checks the prepared archive before connecting and emits only
sanitized errors and a metadata receipt. A successful replay verifies persisted
results; it never bypasses a changed source or destination.

Review conflicts by identifier/hash. For an existing non-version row, the
operator can explicitly set `keep_destination=true` in the preview. Compute
the revised digest with `memory-area-import reconciliation-digest --preview
/private/preview.json`. Source histories stay preserved in the prepared archive;
retaining local metadata does not declare it identical to the source. Immutable
version conflicts require revising the ownership review. Do not alter other
preview fields. This command will not silently replace diverged
histories, canonical state or project metadata. Deployment writer fencing and
future cutover remain the caller's responsibility.
