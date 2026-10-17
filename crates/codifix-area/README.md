# Area migration planning foundation

`codifix-area` is an offline partition planner for the four stable area IDs `trust`,
`homelab`, `workshop`, and `general`. The `codifix-area` binary reads JSON files and
writes JSON to stdout. It has no database/configuration/provider client, importer,
worker activation, credential access, or bundle-export fallback.

```sh
codifix-area inventory --input /protected/snapshot-inventory.json
codifix-area plan --inventory /protected/snapshot-inventory.json
codifix-area verify --inventory /protected/snapshot-inventory.json --manifest /protected/reviewed-manifest.json
```

Exit status is `0` for a valid inventory, emitted draft, or valid reviewed plan;
`2` for validation blockers; `1` for I/O or JSON errors. A successfully emitted
draft still has unresolved ownership. Every verification report states
`import_supported: false`. Invalid reviewed plans emit no partial record actions.

## Snapshot inventory contract

The serde types in [src/types.rs](src/types.rs) define inventory format version 1
and manifest formats 1 and 2. Collect
the inventory from one consistent source snapshot. Include every user table in
every schema, including empty tables and `public._sqlx_migrations`. The policy
catalog covers the 93 active tables of the public migration-0052 schema, plus
an exact ten-table supervisor extension at migration 0053 and the optional
historical evaluation schema `mab_full_20260516_150014`. Head 0052 must not contain
0053 tables; head 0053 requires all ten, including empty tables. Head 0054 requires
those ten plus exactly `public.supervisor_proposals`, which must be absent at
heads 0052 and 0053. Head 0055 additionally requires exactly
`public.supervisor_instance_binding` (retired runtime) and
`public.supervisor_model_contexts` (historical archive). Head 0056 requires those
plus `public.area_import_receipts`, `public.area_import_rows` and
`public.area_import_provenance` (historical archive). Each addition is forbidden
at earlier heads. Private supervisor proposal/context bodies and prior import
receipts never become retrievable knowledge or executable/approved work. Other heads and
unknown additional tables remain blocked. Supervisor authorities, policies,
actions, budget days, attempt states and journal heads are retired runtime;
attempts, settlements, action results and journal rows are historical archive.
None can be imported as active authority or retrievable knowledge. The 0052
catalog, inventory/manifest wire format and normalized hashes are unchanged.
The exact
historical catalog is `_sqlx_migrations`, `curation_runs`, `memory_chunks`,
`memory_entries`, `memory_relations`, `memory_sources`, `memory_tags`, `projects`,
`raw_captures`, `sessions`, and `tasks`. If any member is present, all eleven
tables must be inventoried, including empty tables. Every historical row receives
`protected_archive`; historical chunks are not rebuilt into retrievable areas.
Additional tables or other schemas still block planning and require an explicit
reviewed policy implementation. There is no historical-schema wildcard.
Do not filter a whole-database inventory to `public` to hide these blockers.

Each table carries its columns and foreign-key catalog, independently observed
row count, and every row's stable key and complete-row SHA-256. Hash the complete
original row, including fields not interpreted by this planner. The collector
must define and retain its deterministic row serialization alongside the full
protected snapshot so later reconciliation can reproduce the hash. Hashes are
inventory evidence; they are not the data to import.

Use a plain UUID string as the record key for `projects`, `memory_entries`, and
`memory_canonical_state`. Other keys must losslessly serialize an actual unique
key, including every component of composite keys. A positional row number is
not an identity. Preserve project IDs, slugs, names, root paths and creation
timestamps in the separate project catalog.

The top-level project catalog describes `public.projects` only. Historical
`source_project_id` metadata, when present, is resolved against
`mab_full_20260516_150014.projects`, even if the same UUID also occurs in public.
Historical project provenance is retained in the complete hashed project row and
protected snapshot (and the collector's bound per-schema project sidecar); it
creates no public ownership or area-project binding. Historical rows need not
satisfy the modern public canonical-state/version layout. Declared historical
foreign keys and every original row hash still require complete accounting.

An unresolved `source_project_id` on a table with mandatory `historical_archive`
or `retire_runtime` policy produces an explicit `retained_unresolved_project_reference`
notice. The notice preserves the table, original row key, unchanged project UUID
(including nil UUIDs), schema-local project table and mandatory retention policy.
It records a target absent from this snapshot; it does not infer why it is absent.
Inventory and verification output expose `notices`; draft manifests include
`inventory_notices`. Verification derives notices again from the hash-bound
inventory and rejects suppressed or altered manifest notices, including omission
through a deserialization default. Original IDs and row hashes are never rewritten.

This retention rule does not apply to owned knowledge, project provenance,
derived data or unknown tables. Missing project metadata targets there remain
blockers. Every declared FK and explicit logical reference still requires an
inventoried target, even on historical or retired rows. No notice authorizes
retrieval, runtime reactivation or promotion of the archived record.

For every row, list each declared foreign key once, including `target: null`
for SQL NULL. A non-null target identifies an inventoried table and key.
Additional non-FK relationships use `logical:` constraint names. This format
requires `logical:src_canonical_id` and `logical:dst_canonical_id` references
from relations to canonical state, and a nullable `logical:target_memory_id`
from memory sources to version rows. The collector must also include other
logical relationships needed for lossless reconstruction; the planner cannot
discover relationships hidden inside opaque hashes or JSON columns.

`memory_entries`, `memory_canonical_state`, `memory_sources`, `memory_tags` and
`memory_source_checks` require canonical/project metadata. Resolve child metadata
through source relationships. Version rows also require a positive version
number and tombstone flag. Every inventoried version requires canonical state;
duplicate canonical versions and contradictory child ownership are blockers.
Sessions, tasks, raw captures, relations and commits receive individual reviews,
even if a collector believes they have one apparent owner.

Manifest v2 also supports explicitly reviewed relations with no single
`source_project_id`, as in the current source schema. Both non-null canonical
endpoints must resolve to the **same destination area**, and each endpoint's
original project must have its own area binding. The relation retains its
original ID, bytes, endpoint IDs and null project metadata; no project owner is
invented and no inventory hash is rewritten. Its individual source/project/
content review is still mandatory. Cross-area or archived endpoints are blocked
unless the relation itself remains archived. This does not grant project access
or federation disclosure. Format 1 retains its missing-project rejection.
Materialization checks the actual endpoint bytes against these reviewed logical
references; a matching row hash alone is insufficient.

## Reviewed manifest and protected retention

The draft pins the complete normalized inventory digest, including its schema,
row hashes/counts, source/snapshot IDs and project provenance. It creates explicit
table policies and unresolved lineage/record decisions. No project-name heuristic
chooses an area. Reviews require source, project and content evidence. Tests,
evaluations, ambiguous material and mixed sources must receive a reasoned
`protected_archive` disposition. Every canonical lineage has exactly one decision
covering all versions, tombstones and canonical supporting records.

Target area project bindings retain original IDs and all project provenance;
format 1 cannot merge or rename projects. Original source rows must remain in the
protected source snapshot regardless of their target destination. Derived data
is marked for rebuild while its source is retained. Operational history remains
in an operator-only, non-retrievable archive. Runtime state, jobs, tokens, grants,
subscriptions and pending actions are retained as evidence and marked retired;
table-policy overrides cannot reactivate them.

An area row referencing another area or an archived row blocks planning, except
for the narrowly reviewed format-2 provenance projection described below.
The planner only permits references to original project metadata when an explicit
binding exists for that area. Authorship and timestamps are covered by complete
source row hashes. The library can materialize the offline instructions below;
it does not connect to a database or execute an import.

## Opt-in reviewed provenance projections (manifest 2)

Inventories and generated drafts remain format 1. Existing format-1 JSON stays
byte-compatible: the new `provenance_projections` field is omitted when empty.
Format 1 still rejects archive-reference transformations. An operator may
explicitly opt into manifest `format_version: 2` and supply per-source decisions;
the draft generator never fabricates projections or approval attestations.

The current supported boundary is only the declared
`public.memory_sources.task_id -> public.tasks.id` foreign key. The task and its
declared session must remain `protected_archive`. Other edge kinds, null/unknown
targets, another area's records, arbitrary logical references and unreviewed
edges remain blocked. Ordinary reference validation also prevents area-owned raw
captures from retaining a foreign key to that archived task.
Any non-null `memory_sources.target_memory_id` also blocks projection, even when
the target version belongs to the same area: dropping an existing typed memory
reference is not part of the reviewed projection contract.

Each `ProvenanceProjection` has its own format version (currently 1), unique
decision UUID and exact `ProvenanceBinding`: source instance, snapshot, inventory
digest, destination area, canonical lineage, original version record and hash,
version number/tombstone flag, source record and hash, task constraint, and
archived task record and hash. The source's declared `memory_entry_id` FK must
identify that exact historical version, not merely another version in the same
lineage. Duplicate source decisions and approval-record reuse within a manifest
fail validation.

The complete allowed projection content is one of:

- `{"kind":"archive_reference_only"}`: only the bound provenance identities and
  hashes, without any source excerpt or original context fields.
- `{"kind":"reviewed_excerpt","text":"...","text_sha256":"..."}`: exactly the
  operator-reviewed UTF-8 text and its SHA-256, nonempty and at most 16 KiB.

Unknown payload fields are rejected. Original source file paths, task titles,
prompts, session/capture bodies, and original source excerpts are never inherited
by a projection. A reviewed excerpt is separately supplied content; the planner
cannot prove its derivation or absence of sensitive information from opaque row
hashes. That substantive check belongs to the external reviewer.

`operator_attestation` requires explicit approval, an operator label, non-nil
approval-record UUID, review time, source/destination/content evidence and exact
`decision_sha256`. The `decision_digest()` method hashes the domain prefix
`mem-area-provenance-projection-v1\n` followed by compact serde JSON containing,
in order, `format_version`, `decision_id`, `binding` and `content`. It excludes
the attestation. Changing the snapshot, destination, source/version bytes or
excerpt invalidates that binding. Computing a digest is not approval. These
strings and digests **do not authenticate a human**, prove an external approval
record exists, or establish a persistent global anti-replay ledger. Re-verifying
the same unchanged reviewed manifest is intentionally deterministic/idempotent;
reusing its decision for another snapshot/version/destination is rejected.

Valid output marks the original source row `archive_and_project_provenance` with
its one target area and emits its reviewed projection separately. That is never
an instruction to copy the original source row. The original row ID/hash remains
accounted for in protected storage. All dependent `memory_source_checks` records
retain their IDs/hashes in protected archive: old observations cannot attest a
different projected source or expose original resolved paths. All canonical
versions/tombstones retain their single lineage owner unchanged. No original row
is dropped, and any blocker suppresses both planned rows and projection output.

The existing `verify` CLI validates both formats; its JSON output may contain
reviewed excerpt text. Keep inputs/output in protected storage and do not send
them to operational logs. `import_supported` remains false. A future importer
must implement a safe projection representation and archive-reference storage,
fresh target checks, reconciliation and transactional/idempotent application;
this planner output alone does not claim an existing target SQL schema can store it.

## Offline exact-byte materialization library

`materialize(&inventory, &manifest, payload_reader, &mut protected_archive)`
re-verifies the complete reviewed plan, then consumes a strict `PayloadEvent`
NDJSON stream. Its success result contains opaque `MaterializedMigration`,
`MaterializedPartition`, row and projection objects with getters and `Serialize`
but no public constructors, mutable fields or `Deserialize`. The CLI above does
not expose materialization. Neither the library nor a digest authenticates human
approval, grants import authority, chooses a target or opens a database.

Input order is `header`, then `table_start`, all `row` envelopes, `table_end` for
each table, followed by `complete` and EOF. Table names and row keys are sorted
by UTF-8 byte order, not locale or numeric order. Empty tables are mandatory.
Header fields are `format_version: 1`, exact `source_instance`, `snapshot_id`,
`inventory_sha256`, and `serialization_id: "postgresql-to-jsonb-utf8-utc-v1"`.
Table envelopes contain `table` and exact `row_count`; rows contain `table`,
`key`, `row_sha256`, and `row_json` as a JSON **string**. The footer contains
exact `table_count` and total `row_count`. Unknown envelope fields, extra,
missing, duplicate, reordered or truncated records/tables, and trailing input
fail closed. A payload exporter for this ordered stream remains separate work;
the metadata inventory collector does not retain these complete-row strings.

The serialization ID means PostgreSQL `to_jsonb(complete_heap_row)::text` encoded
as UTF-8 without an added newline, under UTC, `DateStyle = ISO, YMD`,
`IntervalStyle = postgres`, `extra_float_digits = 3`, and `bytea_output = hex`.
Every row's exact bytes are SHA-256 checked against the inventory. Top-level JSON
must have each inventoried column exactly once, no unknown/missing column, and
no NULL in a non-nullable column. Opaque JSON values and large numbers are never
converted through floating point or reserialized for an untouched row.

For the eleven materializable public tables, actual payload primary keys must
match inventory record keys: UUID `id`, canonical-state `canonical_id`, signed
bigint source-check `id`, or PostgreSQL JSONB-array text for tag
`[memory_entry_id, tag]`. Required FK shapes, each actual FK value/NULL and the
supported logical reference values must match the inventory. Unknown logical
edges or non-primary target shapes block. Project catalog fields, version flags,
and direct/derived project provenance are checked against the payload and its
referenced inventory records. Tasks inherit their session's project; raw
captures inherit their task's project. Archived operational tables still retain
exact key/hash/count/column accounting, but their varied SQL PK/unique-FK
semantics are not independently reconstructed: inventory v1 has no PK catalog.
The independently verified source inventory remains the trust anchor. A target
importer must also check the actual SQL schema/types; this is not a PostgreSQL
type engine or a replacement for that check.

Each area receives exact original owned rows plus original project rows only
where a project binding explicitly names that area. A format-2 projection
instead emits a separate `memory_sources` target row: original source/version
IDs and creation timestamp, `source_kind: "note"`, the exact reviewed excerpt
or NULL, and NULL for task, path, commit, symbols, line range, content hash and
memory-target fields. Its fourteen-column schema is explicitly allowlisted;
added or changed projection columns block. It carries only bound provenance
IDs/hashes, decision ID/digest and reviewed content, not the original attestation
evidence strings. Original source/context/check rows are retained in the archive,
never copied into the projected target.

`target_sha256()` always hashes the exact materialized wire bytes. It equals the
source hash for an untouched row. Projected JSON is deterministic compact Rust
serialization, **not** PostgreSQL's JSONB text rendering. A database importer must
check projected JSONB semantic/type round trips and separately record the
PostgreSQL-rendered target hash. `manifest_digest()` hashes compact serde manifest
JSON. `partition_sha256()` hashes `mem-area-materialized-partition-v1\n` followed
by compact serialized partition JSON with its `partition_sha256` field blank;
this binds area, source/snapshot, migration head, inventory/manifest digests,
ordered target bytes/schema and projection metadata.

The protected archive sink receives a header, complete table catalogs, every
original row with exact bytes/hash and planned disposition once, and a complete
footer. The result reports its full-stream SHA-256 and byte/table/row counts.
It is **provisional** until materialization succeeds and the caller durably
publishes it. On any error no trusted result is returned, but a partial archive
prefix may exist and must be discarded. The caller must use exclusive private
files (0600 inside an owner-controlled 0700 directory), durable file/directory
fsync and atomic publication. `Write::flush` is called; generic writers cannot
promise filesystem durability. Never send archive or partition JSON to logs.

Input envelope size is capped at 64 MiB, and retained target payload plus counted
metadata defaults to 256 MiB; exceeding either fails rather than truncates.
`materialize_with_budget` accepts a validated `RetainedTargetBudget::from_mib`
from 256 through 1024 MiB inclusive for explicitly reviewed larger snapshots.
`materialize` retains the default. No unbounded API or envelope-limit override
is exposed, and changing the budget does not change successful output bytes or
partition hashes. The operator CLI binds the chosen budget into its preparation
receipt and requires the same value for import (see `codifix-area-ops`). Original
archive bytes stream without whole-archive buffering. Inventory/verification
metadata, temporary row parsing, schema/index overhead and caller buffers are
additional memory; the cap is not a total-process RSS guarantee. Errors are
static and omit source text, keys, JSON-parser and underlying I/O details.

Current collector limitation: `memory_relations` has no direct project column
and is inventoried with `source_project_id: null`; the planner requires explicit
project provenance for area-owned rows. Such relationships remain blocked for
ownership (or explicitly archived), not silently assigned or imported. A future
reviewed collector contract could derive provenance from matching endpoint
projects; cross-project endpoints need a separate explicit policy. No existing
inventory/classification is rewritten by this materializer.

## Evidence boundary

Validation checks the supplied inventory and review, including full table/record
accounting, catalog FK accounting, known logical references, unchanged provenance,
hash binding and single lineage ownership. Planning alone does not attest a collector's
honesty/completeness, recompute hashes from source bytes, or infer undisclosed JSON
references. A forged inventory and freshly forged matching manifest are not
detectable without independently trusted source evidence.

Materialization additionally verifies complete payload bytes and produces offline
projected target instructions plus the protected archive described above. It
does not collect/export a database, write an import journal,
verify actual database target bytes/counts, import idempotently,
restore a backup, establish isolation, or approve a live cutover. Those remain
separate implementation and operational gates. A valid plan is not migration or
demo completion evidence.

Synthetic tests exercise the planner, materializer and actual binary, with a regression that
compares the explicit policy catalog against the repository's SQL migrations.

## Large inventories

The restored baseline contains approximately 2.33 million rows. Format 1 still
loads every row into memory, builds reference indexes and materializes every
planned record during verification; successful small tests do not prove a full
baseline run will fit. Inventory hashing now sorts borrowed references and streams
the normalized JSON directly into SHA-256, preserving format-1 digests without
cloning all source rows or constructing another full JSON buffer. Reference
validation also indexes borrowed table/key strings. These changes reduce
avoidable allocation but do not make the planner fully streaming. Measure peak
memory, elapsed time and output size on the isolated snapshot before treating it
as migration-ready; keep sensitive inventories and build caches on `/data`.
