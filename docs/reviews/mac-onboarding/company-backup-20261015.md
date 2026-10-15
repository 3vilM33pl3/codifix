# Company owner backup preparation — 2026-10-15

Status: **blocked before data capture**. This is not a verified fresh backup or
authorization to migrate either live owner.

## Confirmed scope and inventory

Approved-plan checkpoint: `c3b4ac9a-63ce-447c-b384-973270144f21`, capture
`ffc3768d-06e7-4610-9faa-b16be4f61356`. Preparation covers only Homelab and
Workshop owner databases, exact active application mounts and container metadata.
It excludes a whole-gateway/PDS backup or any service/network/authentication
change.

| Owner | Live database container | Database | Schema head / successful rows | Approx. size |
| --- | --- | --- | --- | --- |
| Homelab | `memory-homelab-preview-db-20260919` | `memory_homelab_preview_20260919` | 76 / 76 | 39 MB |
| Workshop | `memory-workshop-preview-db-20260918` | `memory_workshop_preview_20260918` | 76 / 76 | 188 MB |

Inventory observed around `2026-10-15T06:23Z`. Both live database containers use
image `sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180`.
Their `pg_dump` is PostgreSQL 17.11; host `pg_restore` is 17.9.

The exact active Workshop `/config.toml` mount is
`/home/olivier/.local/state/memory-workshop-preview-20260918/questions-renewed-20260919-grant7d/app.toml`.
The root Workshop `app.toml` is not its active container configuration. Homelab
uses `/home/olivier/.local/state/memory-homelab-preview-20260919/app.toml`.
Both applications bind their executable from
`/home/olivier/.local/state/memory-company-20260919/runtime-dependable-20260919/memory`.

## Noninteractive key prerequisite

The existing recipient was read from protected configuration
`/home/olivier/.local/state/memory-company-20260919/dependable-backup-config.json`.
A synthetic, nonsensitive public-key encryption probe succeeded. Decrypting that
probe with `--batch --no-tty --pinentry-mode error` failed with
`public key decryption failed: No pinentry` and `decryption failed: No pinentry`.
No interactive pinentry, key unlock, credential change or alternate recipient
was attempted.

Preparation stopped before any live database dump or configuration capture.
The private directory
`/data/tmp/memory-owner-backup-20261015-BohNpprC` contains only `prepare.mjs`;
there are no fresh ciphertexts, dump hashes, restore receipts or off-host copies
from this attempt. All original files and running services remain intact.

The prepared operator reuses the repository's row/sequence inventory and role
restore helpers, but **has not completed** its backup/restore path. Its intended
checks are encrypted owner custom dumps, separately encrypted roles and exact
active configuration/runtime mounts, successful decryption, `pg_restore --list`,
and separate network-none restore containers with complete row hashes, sequence
states, schema, owners and grants compared. It will preserve the fresh restore
containers stopped and retain their volumes; no live restore is planned.

## Sufficiency and remaining gaps

This attempt is **insufficient for owner-only migrations** because no fresh
backup was captured or restored. Noninteractive decryption must become available
through the existing authorized key before the prepared end-to-end verification
can succeed; then rerun into a fresh private directory.

Even a successful live `pg_dump` would give a coherent snapshot of each database
separately. PostgreSQL sequence values are nontransactional; roles and filesystem
files are separate reads. Two owner dumps do not establish gateway/PDS/global
consistency, a stopped-writer cutover point or recovery of later live writes.
Independent restore verification would prove that snapshot's recovery, not new
application startup or migration compatibility. An owner-only migration still
needs its exact upgrade tested on the recovered clone and a fresh final backup
appropriate to the authorized cutover. A whole-company rollback, off-host
redundancy and gateway/PDS recovery remain outside this scoped preparation.
