# Mac immutable runtime rebind evidence — 2026-10-17

Implementation commit: `f948e2ec`, based on `eab911ad`. No live Mac mutation,
network/provider call, Cargo build or native activation was performed for this
slice. The tests use an independently populated synthetic schema-2 filesystem
fixture and a fake native boundary; they do not claim exact live transfer.

## Scope and operator contract

The release-specific `macos-runtime-rebind.py` preserves a completed old journal,
its original prerequisite pins/stages, root receipt and plist bytes. Inspection
requires exact archived stage inventory, every original job/label and prior
state, an absent old root and an already populated destination. The private
manifest has a canonical SHA-256 seal and records new runtime dependencies,
archive hashes, preservation references, managed-output hashes and stable
cluster identity. No old journal is rewritten or replayed. Secret bytes are
used locally to validate/reproduce protected configs; they are never serialized
into the manifest or evidence document.

Input JSON has exactly these keys:

- `format`: `codifix-3.0-macos-runtime-rebind-v1-input`.
- `old_root`, `new_root`: exact absolute source/destination mapping.
- `archive`: `journal` and `root_receipt`, each `{path, sha256}`.
- `candidate`: `{path, sha256, version}`; exact `codifix 3.0.0-rc.1` or `codifix 3.0.0`.
- `postgres`: the original onboarding prerequisite object with `bin_dir`,
  `major`, `tool_sha256` (postgres/initdb/pg_ctl/psql/pg_config/pg_controldata),
  `vector_library`, `vector_library_sha256`, `vector_control`,
  `vector_control_sha256`, `vector_version`, `vector_sql` (name→hash).
  The original PostgreSQL major and pgvector version are required. PostgreSQL
  tool hashes may change within that major when explicitly reviewed and pinned;
  this guard does not require byte-identical dependency binaries.
- `reconciliation`: `{path, sha256}` for the reviewed private proof below.
- `old_jobs`: exact `{label, prior_state, plist:{path, sha256}}` entries;
  `prior_state` is active/inactive/failed. Schema-2 two-area/two-repository/two-cache
  inventory has seven old jobs. Original labels and ports remain exact in the
  manifest; synthetic fixtures must have their own genuine identities.

Reconciliation JSON has exactly `format`
(`codifix-3.0-macos-runtime-rebind-v1-reconciliation`),
`cluster_system_identifier` (decimal string), `admin_role_oid` (original bootstrap
OID 10), `preservation_receipt_sha256`, and `areas`. Each area has exactly `name`,
`database_oid`, `role_oid`, sorted unique `project_ids`, `migrations_sha256`.
The migration digest is canonical JSON SHA-256 of the ordered list of
`{version, checksum, success}` values, versions 1 through 85. Prepare it from
independently verified candidate checksums. The preservation-receipt digest is
an operator-reviewed reference, not a full row-content verification performed
by this helper. Retain the separate matched data/grant/count/digest proof.

Archives, proof, input and manifests must be private regular current-user
files, single-link, with no symlinks/traversal and trusted ancestors. Sources
and candidate/native prerequisites must have protected trusted paths. Retain
the four-file bundle layout in the operator README outside the checkout/root;
all source files, resolved Python executable, Mach-O tools, pgvector and exact
candidate are pinned. Moving the checkout last does not invalidate launchers.
Replacing or relocating this retained bundle does invalidate them.

## Metadata SQL and cold startup

Inspection and binding use a separately reviewed loopback-only maintenance
PostgreSQL on the populated renamed cluster. Old jobs are unloaded and old
application/sync/watch writers drained; this is not an application restart.
The helper does not start that maintenance server or perform renames. After
bind completes, stop it cleanly before the new PostgreSQL launcher is used.
The native rehearsal must verify this maintenance step too.

All metadata queries run through pinned `psql -X -w`, exact loopback port,
private role-specific pgpass and `ON_ERROR_STOP=1`. Each script is wrapped in:

```sql
BEGIN READ ONLY;
SET LOCAL statement_timeout='4000ms';
SET LOCAL lock_timeout='1000ms';
SET LOCAL search_path=pg_catalog;
-- one SELECT from the guarded queries below
ROLLBACK;
```

These are the exact fixed administrator and area SELECTs in the implementation:

```sql
SELECT json_build_object('system_identifier',(SELECT system_identifier::text FROM pg_control_system()),'data_directory',current_setting('data_directory'),'port',current_setting('port')::integer,'admin_oid',(SELECT oid::bigint FROM pg_roles WHERE rolname=current_user),'admin_scram',(SELECT rolsuper AND rolcanlogin AND rolpassword LIKE 'SCRAM-SHA-256$%' FROM pg_authid WHERE rolname=current_user));
```

```sql
SELECT json_build_object('database_oid',(SELECT oid::bigint FROM pg_database WHERE datname=current_database()),'owner_oid',(SELECT datdba::bigint FROM pg_database WHERE datname=current_database()),'role_oid',(SELECT oid::bigint FROM pg_roles WHERE rolname=current_user),'role_restricted',(SELECT NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole AND NOT rolreplication FROM pg_roles WHERE rolname=current_user),'project_ids',COALESCE((SELECT json_agg(id ORDER BY id) FROM public.projects),'[]'::json),'migrations',(SELECT json_agg(json_build_object('version',version,'checksum',encode(checksum,'hex'),'success',success) ORDER BY version) FROM public._sqlx_migrations),'bindings_disabled',NOT EXISTS(SELECT FROM public.laptop_curated_bindings WHERE enabled));
```

For every retained cache ID (validated UUID), the area-role query selects
`subscription`, `device`, `scope` from `public.device_replica_projections`
(curated_project_v1) or `public.onboarding_project_projections` (shared),
restricted to that exact subscription. The complete JSON must equal archived
metadata. The final administrator query requires no `memory_onboarding_admin`
or selected `memory_laptop_AREA` role. No new subscription is obtained.

Missing/unreachable server, wrong pgpass credentials, SQL errors, timeout,
missing/multiple result lines or malformed JSON fail closed. Exact identity
comparison rejects wrong data_directory/port/system ID/admin OID/SCRAM status,
wrong DB/owner/role OIDs, excessive role privileges, changed project IDs,
incomplete/different migrations, enabled contribution bindings, missing or
changed cache scope/device/subscription, or old role names. No remediation SQL
runs. Existing secrets, role password verifiers and database data are preserved.

Once bound, `run --kind postgres` does **not** execute these SQL checks: it
validates current file pins/config/credentials/receipt and original-job absence,
then `pg_controldata` against the populated cluster system ID. It execs pinned
PostgreSQL with explicit managed config/HBA, loopback only, no UNIX socket or
preload libraries. It works while PostgreSQL is stopped, never calls initdb and
never hashes mutable cluster data. `run --kind area --area AREA` requires that
server ready and repeats the live SQL checks before exact candidate exec.
Direct sync/watch/provisioning commands are rejected before native operations.

## Bind and execution boundaries

One destination lock protects the exact private immutable intent and managed
root. Each file is published without replacement, conflicting/unknown files
are preserved and refused, and the receipt is published last. Exact retry may
finish an interrupted publication; a partial bind cannot run. A crash leaving
an unpublished atomic temporary file requires manual review. Do not adopt or
remove a foreign file to force success or blindly retry an uncertain outcome.

The managed configs enforce read-only, verify-only migrations, disabled
schema-2 automation/offline, background/MCP/cluster/federation off. They retain
historical cache IDs. Only PostgreSQL and selected area plists exist; all have
Disabled true, RunAtLoad false, KeepAlive false. Bind never bootstraps them.
Every run rebuilds/seal-checks the manifest, hashes current sources/native
prerequisites/credentials/preserved files/output config/receipt, applies native
preflight and clears inherited provider/PG/config environment before exec.
Native preflight includes `initdb --version` only, not initialization.

## Focused validation and limits

Command: `python3 scripts/codifix-cutover/test_macos_runtime_rebind.py`.
Result: **12 tests passed in 33.121 seconds**; AST parsing and `git diff --check`
also passed. Tests cover populated zero-init/create/secret-generation/authority,
seven old-job inventories, cold schema-2 configs/cache IDs and three inert
plists, exact retry, interrupted publication/no partial run, manifest/receipt/
config/prerequisite/credential/source tamper, symlinks, foreign/unknown files,
wrong system ID/OIDs/enabled bindings, resumed old jobs, incomplete archived
stages, forbidden sync/watch, dependency upgrade refusal, clean environment
and expected PostgreSQL/area command selection. Mutating synthetic cluster data
does not invalidate its stable identity binding.

Native gaps: real Darwin Mach-O/version checks, APFS publication behavior,
actual PostgreSQL SQL/result parsing, exact independently populated archived
journal acceptance, candidate verify-only service startup/read views and
zero-egress observation require native isolated rehearsal. Signing/Keychain,
actual live cold-window state transfer and launchd activation remain separate
acceptance gates. These Python tests neither execute a real native binary nor
establish live readiness. No activation bypass is provided.

## Original reviewed source hashes

- `scripts/codifix-cutover/macos-runtime-rebind.py`: `b0aad740d42e77c7390fe987769825b24d319efcd3f72ced5756784d4567779c`
- `scripts/codifix-cutover/test_macos_runtime_rebind.py`: `ab0d2414f7428194698b46c7a3fbf04a56ad785bb58e15a9268c3fb9a79cfc6a`
- `packaging/macos/onboard.py`: `33d09054db3b4602c9c7caa45ad49980359329203a41fbdf984c0404b8da654b`
- `packaging/macos/onboarding_files.py`: `7807e076ad80006439e1cddbd8aa1d8e5adb626b04e03d1bc50b9374b6e8d27e`
- `packaging/macos/onboarding_native.py`: `31f2e2138e0c04cdd52932db7f9692cba0a7e2358711122fdc7d01929a6e8a02`

## Actual cutover correction — 17 October

The first live native inspection reached the final legacy-role check and found
that bare PostgreSQL `EXISTS` returns `f`, while the shared query reader requires
JSON. Commit `b3453aaf` wraps that result in `to_json` without changing the
predicate. Corrected helper SHA-256:
`fc5df21d399bb8881755dc9486f7b856cf17b962d3654bae7dc4460ec9996d22`.
The retained Mac helper was replaced and its manifest regenerated before binding.
Real native inspect and bind then passed. A real PostgreSQL boolean serialization
check passed, and all 13 focused tests pass, including the native JSON reader's
false/true cases and rejection when a legacy role remains (`f031789c`).

The earlier native gaps above describe the original source review. Current live
deployment and remaining client/sync work are tracked in the
[live cutover ledger](../live-cutover.md).

PostgreSQL references: [pg_controldata](https://www.postgresql.org/docs/17/app-pgcontroldata.html),
[transaction read-only mode](https://www.postgresql.org/docs/17/sql-set-transaction.html),
[system information functions](https://www.postgresql.org/docs/17/functions-info.html),
[server configuration](https://www.postgresql.org/docs/17/runtime-config-file-locations.html).
