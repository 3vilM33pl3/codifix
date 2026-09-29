# Per-item backlog disposition ledger

`backlog-ledger.mjs` creates a private streaming NDJSON disposition for **every
original source row**, including captures, sessions, tasks, replacement and loop
proposals, protected histories, relations, derived rows and retired runtime
state. The complete marker binds all preceding output bytes. No row text,
source path, arbitrary status, prompt or opaque composite record key is copied.
UUID/integer IDs remain useful for local follow-up; all record keys also have
SHA-256 bindings to the archive.

This is an agent's preservation-policy review using the already reviewed
ownership decisions. It does not claim Olivier approved individual items or
that raw content has been read, curated, imported, made searchable or executed.
Pending proposals remain pending owner decisions, raw captures remain pending
curation, sessions/tasks remain pending ownership review, protected relations
remain pending sharing review, and derived rows become an explicit rebuild
backlog. Source runtime rows are preserved as retired historical state.

Prepare an owner-only contract using the same exact file bindings as
`account-staging.mjs`:

```json
{
  "schema_version": 1,
  "kind": "materialized_backlog_dispositions",
  "reviewer": { "kind": "agent", "id": "codex-rc2" },
  "prepared": { "path": "/protected/prepared.json", "sha256": "EXACT_FILE_HASH" },
  "manifest": { "path": "/protected/manifest.json", "sha256": "EXACT_FILE_HASH" },
  "ledger": { "path": "/protected/ownership-ledger.json", "sha256": "EXACT_FILE_HASH" },
  "archive": { "path": "/protected/archive.ndjson", "sha256": "EXACT_FILE_HASH" }
}
```

```sh
node scripts/areas/backlog-ledger.mjs \
  --contract /protected/backlog-contract.json \
  --contract-sha256 EXACT_CONTRACT_FILE_HASH \
  --output /protected/backlog-dispositions.ndjson
```

The output directory must be owned mode 0700. Inputs and output are private
files. Existing outputs are never overwritten; incomplete work is not published.
The tool streams the large archive, validates framing, row hashes, ordered
unique record identities, lineage decisions and complete accounting using the
existing accounting validator, then publishes only after all bindings match.
Memory consumption is bounded by existing ownership metadata and a one-MiB
output buffer, not the number of source rows. Allow disk space for one metadata
line per source row. The stdout summary includes only counts and hashes.

Re-run against a fresh prepared archive after source changes. Stable IDs alone
do not authorize reusing old decisions: source, snapshot, inventory, manifest,
archive and ownership-ledger hashes are included in the header. Use the private
original archive to resolve per-item follow-up; this ledger does not grant new
disclosure or execution authority.
