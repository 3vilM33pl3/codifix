# Materialized source accounting (files only)

This report answers **where every original row was preserved**, not whether a
service is usable or a database import has passed. It opens no database, starts
no process or worker, and makes no network/provider request. It never emits
knowledge bodies, evidence, paths, project names, proposal contents or arbitrary
status/review text.

After successful materialization, create a mode-0600 contract in a mode-0700
directory. Each hash is the exact file SHA-256, not a normalized inventory or
manifest hash. Use the final approved ledger and the same manifest used by
`memory-area-import prepare`.

```json
{
  "schema_version": 1,
  "kind": "materialized_source_accounting",
  "prepared": { "path": "/protected/prepared/prepared.json", "sha256": "EXACT_FILE_SHA256" },
  "manifest": { "path": "/protected/reviewed-manifest.json", "sha256": "EXACT_FILE_SHA256" },
  "ledger": { "path": "/protected/ownership-ledger.json", "sha256": "EXACT_FILE_SHA256" },
  "archive": { "path": "/protected/prepared/archive.ndjson", "sha256": "EXACT_FILE_SHA256" }
}
```

```sh
node --max-old-space-size=4096 scripts/areas/account-staging.mjs \
  --contract /protected/accounting-contract.json \
  --contract-sha256 REVIEWED_CONTRACT_FILE_SHA256 \
  --output /protected/new-source-accounting.json
```

The archive is streamed and independently rehashed, with exact complete-row
hashes, strict ordering and a required completion footer. Missing rows, changed
inputs, unknown tables/destinations, incomplete histories or inconsistent
prepared per-area counts fail closed. Empty inventoried tables remain in the
report. Original row totals must match the prepared receipt and all destination
categories must sum to that total. Existing reports cannot be overwritten.

Interpretation:

- `exact_area_copy` counts original rows selected for unchanged copying.
- `protected_archive` counts originals kept outside target knowledge, including
  raw context, proposals, protected histories and unshared relations.
- `archive_and_rebuild` preserves derived source rows without treating them as
  a completed target index or graph rebuild.
- `archive_and_retire` preserves runtime evidence without restoring jobs,
  credentials, leases or authority.
- `archive_and_project_provenance` preserves original evidence privately and
  creates only separately reviewed reference metadata, not searchable evidence.

Every original row is preserved in the full archive—including selected rows.
These categories describe the **additional target disposition**, not different
archives. A project metadata row can be copied to several areas while remaining
one original source row; the separate `target_copy_rows` counts make this visible.
Per-area totals are materialized payload counts, not evidence that a database
has already imported those rows. Pair this report with
[read-only target verification](verify-staging-README.md) and import receipts.

The protected backlog includes public raw captures with null `curated_at`,
pending replacement proposals, pending loop proposals, protected canonical
histories, cross-area edges and derived rows requiring a future rebuild. Counts
come from exact original rows; they do not create actionable live jobs or approve
any proposal. Status strings other than the known `pending` value are never
included in output. Sensitive/test/evaluation history review classifications
remain explicit preservation categories, not automatic import candidates.

The source remains writable. A new snapshot and full-history/dependency delta
revalidation are required before any future cutover. This report does not
re-approve labels or complete customer recovery, application activation or CA
migration.

```sh
node --test scripts/areas/account-staging.test.mjs
```
