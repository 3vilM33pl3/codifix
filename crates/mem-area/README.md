# Area migration planning foundation

`mem-area` is an offline partition planner for the four stable area IDs `trust`,
`homelab`, `workshop`, and `general`. The `memory-area` binary reads JSON files and
writes JSON to stdout. It has no database/configuration/provider client, importer,
worker activation, credential access, or bundle-export fallback.

```sh
memory-area inventory --input /protected/snapshot-inventory.json
memory-area plan --inventory /protected/snapshot-inventory.json
memory-area verify --inventory /protected/snapshot-inventory.json --manifest /protected/reviewed-manifest.json
```

Exit status is `0` for a valid inventory, emitted draft, or valid reviewed plan;
`2` for validation blockers; `1` for I/O or JSON errors. A successfully emitted
draft still has unresolved ownership. Every verification report states
`import_supported: false`. Invalid reviewed plans emit no partial record actions.

## Snapshot inventory contract

The serde types in [src/types.rs](src/types.rs) define format version 1. Collect
the inventory from one consistent source snapshot. Include every user table in
every schema, including empty tables and `public._sqlx_migrations`. The policy
catalog covers the 93 active tables of the public migration-0052 schema, plus
the optional historical evaluation schema `mab_full_20260516_150014`. The exact
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

An area row referencing another area or an archived row blocks planning. A
future importer must implement reviewed provenance references/excerpts for mixed
sessions; this slice does not silently drop, rewrite or copy those references.
The planner only permits references to original project metadata when an explicit
binding exists for that area. Authorship and timestamps are covered by complete
source row hashes; no row transformation is implemented here.

## Evidence boundary

Validation checks the supplied inventory and review, including full table/record
accounting, catalog FK accounting, known logical references, unchanged provenance,
hash binding and single lineage ownership. It does not attest a collector's
honesty/completeness, recompute hashes from source bytes, or infer undisclosed JSON
references. A forged inventory and freshly forged matching manifest are not
detectable without independently trusted source evidence.

This slice does not collect/export a database, write an archive/import journal,
verify target bytes/counts, import idempotently, transform mixed provenance,
restore a backup, establish isolation, or approve a live cutover. Those remain
separate implementation and operational gates. A valid plan is not migration or
demo completion evidence.

Synthetic tests exercise the planner and actual binary, with a regression that
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
