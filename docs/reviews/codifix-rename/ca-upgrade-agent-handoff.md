# CA Trust: execute the Codifix upgrade in one console session

Instruction ID: `ca-codifix-upgrade-20261018-v1`.
Prepared for the CA-local agent, 18 October 2026. Target: the Trust application
on `ca.metatao.net`, not the certificate authority software itself.

## Authority, outcome and working method

Olivier requested: “Lets upgrade CA, write a instruction page in Notion,
detailed so it can be done by the CA agent in one shot”. This is the current
authorization for the scoped application cutover below, superseding the older
read-only-only Trust requests. Complete the local discovery, preparation,
backup, migration, installation, acceptance and reporting in one agent run.
Do not return after inventory just to request the same application approval.
Downtime is acceptable; there is one user. No rolling deployment or failover
system is needed.

Use existing local console access only. **Keep SSH masked and disabled.** Do not
change Step CA, OpenBao, signing keys, certificate issuance, trust roots, host
authentication, firewall, OS/glibc, PostgreSQL major version or unrelated
services. Do not enable Pi, fleet agents, provider/model calls or new disclosure
permissions. No CA key or OpenBao storage belongs in the application backup.

The approved artifact choice is the published, unsigned internal Codifix
3.0.0-rc.1 ARM64 package authenticated by the explicit hashes below. Record that
fact; do not call it signed or bypass a signed-release verifier. The old
single-use unsigned exception is historical, not the authority for this run.

Use a strong engineering agent for execution and Astra for bounded review of
the discovered mapping and final evidence. Read-only helpers may inspect
independent facts; one writer owns the host. If delegation is unavailable,
perform the same checks locally and report that independent review was absent.
Do not let model/tool availability turn this into another planning-only pass.

This is an agent execution runbook, not a blind paste-all shell script. Paths
marked as locally bound must be obtained from the current named service. The
agent is authorized to create a small private migration/configuration adapter
using the exact source contracts below, review it, and test it against the
restored application database before applying it. Do not weaken an invariant
to make an existing create-only or signed-upgrade helper accept this target.

Completion means: the native Trust service and ordinary CLI/TUI use Codifix,
the existing data and identifiers survive, old writers cannot restart, and
Step CA/OpenBao/SSH boundaries are unchanged. Fresh federation is a separately
reported part of the outcome: complete it only with valid existing operator
authority and newly issued same-scope bindings; otherwise leave it off and
report `LOCAL_COMPLETE_FEDERATION_PENDING`, not full fleet completion.

## Read first; do not replay historical instructions

- Current action: https://app.notion.com/p/3e78990715f48170a918f3f2c70277c9
- Result inbox: https://app.notion.com/p/3e78990715f481cd935ff0a897040e4e
- Rename plan: https://app.notion.com/p/3fc8990715f48157b7b0e48766c386b6

The newest CA inventory is **17 October, 21:17:38 UTC**; the earlier October 4
installation report is history. No v2 recovery or v3 binding follow-up was
present when this handoff was prepared. Resolve those facts locally during this
run; do not assume a new report exists. Known observations, to reconfirm:

| Item | Last observed value |
| --- | --- |
| Host | Debian 12, aarch64, glibc 2.36 |
| Old service | `memory-layer@trust.service`, active |
| Old executable | `/opt/memory-layer/instances/trust/release/bin/memory` |
| Executable SHA-256 | `477d1669e5f5fbaac81ac5be06ec2e64e10c7444ccade71b161b2d8a2d5c117e` |
| Process credential | `/run/credentials/memory-layer@trust.service/memory-layer.toml` |
| Application listener | loopback port 8091 |
| Database lead | PostgreSQL 17/trust, loopback/socket port 5433, `memory_trust` |
| Migration head | 65, all successful |
| Approximate historical counts | 508 canonical state rows, 557 entry versions, 2 projects |
| Stable publisher lead | `cc1bff80-d0e4-4687-9e62-9b8a5fe4bb16` |
| Distinct import-target lead | `9be66620-5710-4018-a2b8-e1f2ed2a43fa` |
| Old evidence lead | `/var/lib/memory-layer-native-preflight/trust-rc4-v24-20261004` |

Counts may have increased. Establish a fresh baseline; never overwrite current
data with the old report or a Monolith Trust subset. Health's transient instance
ID is not the stable publisher or area ID. Preserve all of these identities in
their actual roles.

## 1. Fetch and authenticate the exact release before downtime

At the existing root console, create a new private work directory. If a prior
directory for this instruction exists, inspect its journal instead of starting
again or overwriting it.

```bash
set -euo pipefail
umask 077
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
WORK=$(mktemp -d /var/lib/codifix-ca-upgrade-20261018.XXXXXX)
chmod 700 "$WORK"
cd "$WORK"
BASE=https://github.com/3vilM33pl3/codifix/releases/download/v3.0.0-rc.1
curl --fail --location --proto '=https' --tlsv1.2 \
  "$BASE/codifix_3.0.0.rc.1_arm64.deb" -o codifix-arm64.deb
curl --fail --location --proto '=https' --tlsv1.2 \
  "$BASE/codifix-3.0.0-rc.1.tar.gz" -o source.tar.gz
printf '%s  %s\n' \
  9f6bd3b5ad4cf58423846ef8c7ab80138a1f62170edd4b8a7b24c3a55c2540dd codifix-arm64.deb \
  85203fdde0e45f7d873794f553a37d72a5653b2f8ab6f3f6113dc26cb35bfaed source.tar.gz \
  | sha256sum --check --strict -
dpkg-deb --field codifix-arm64.deb Package Version Architecture
mkdir package source
dpkg-deb --extract codifix-arm64.deb package
tar --extract --gzip --file source.tar.gz --directory source --strip-components=1
SOURCE="$WORK/source"
printf '%s  %s\n' \
  c28797c1adc2eeab2d57fb3de39f339ffc637a7f673e3174a156e5adacb6714c \
  package/usr/bin/codifix | sha256sum --check --strict -
package/usr/bin/codifix --version
```

Expected package: `codifix`, version `3.0.0~rc.1`, architecture `arm64`;
CLI `codifix 3.0.0-rc.1`. GitHub's download filename uses `.rc.1`, while the
Debian package version/canonical checksum filename uses `~rc.1`.
Exact release source is `66c2d1ecc4ca9b0c052e1d854f71a0703f998b71`.
No Linux ARM64 tarball or `.asc` signature exists in this release.

If outbound download is unavailable, transfer these exact two files through the
existing console-approved mechanism and run the same hash checks. Do not enable
SSH, change proxy/firewall trust or select a different release. Do not execute
anything until both hashes match. Inspect archive layout before extraction if
it differs from the documented release layout.

Static release ABI validation passed against Debian 12 ARM64/glibc 2.36:
the main executable requires GLIBC at most 2.34, GLIBCXX 3.4.29 and CXXABI 1.3.13.
Verify native loader/library availability and `--version`; do not upgrade glibc.
Read the authenticated source's `packaging/CODIFIX-3-REINSTALL.md`,
`packaging/debian/codifix@.service`, `packaging/debian/run-instance`,
`packaging/debian/verify-instance-release`, and
`scripts/codifix-cutover/README.md`.

## 2. Bind the existing installation and prepare an exact local plan

Capture service metadata and full configuration privately under WORK; never
paste those raw files into Notion. Record old active/enabled states, process
executable/hash, credential source, drop-ins, protected paths and restart
behavior. Read `LoadCredential` from the current unit before stopping it:
the `/run/credentials/...` view disappears on stop. Preserve its persistent
source and any federation credential separately.

```bash
systemctl show memory-layer@trust.service \
  -p MainPID -p ActiveState -p SubState -p FragmentPath -p DropInPaths \
  > "$WORK/old-service-metadata.txt"
systemctl cat memory-layer@trust.service > "$WORK/old-unit-private.txt"
systemctl show step-ca.service openbao.service \
  -p Id -p MainPID -p ActiveState > "$WORK/protected-services-before.txt"
systemctl is-enabled ssh.service > "$WORK/ssh-enabled-before.txt" || true
systemctl is-active ssh.service > "$WORK/ssh-active-before.txt" || true
ss -ltn > "$WORK/listeners-before.txt"
curl --fail --silent --show-error --max-time 10 \
  https://ca.metatao.net/health > "$WORK/ca-health-before.json"
curl --fail --silent --show-error --max-time 10 \
  https://secrets.metatao.net/v1/sys/health > "$WORK/bao-health-before.json"
```

Resolve the actual current configuration and any environment overrides with the
same precedence as the old binary. Correlate its PID/socket with PostgreSQL
metadata. The read-only binding helper in the authenticated source can assist;
its historical pinned hash is
`8a98b258264306e98ebe2bd79c1c0905427c030daae3e3458881cf10c9ddf5a3`.
A helper refusal is a diagnostic, not grounds to skip binding: inspect the
named service locally, explain any drift, and bind the real target. Stop if it
is another area, a remote/shared database, or a different host than expected.

As the existing local database administrator, through the already functioning
Unix peer-authenticated route, record cluster system identifier, database OID,
owner/runtime role OIDs, schema owners, grants, extensions, migration history,
stable area/publisher/project IDs and exact application state paths. Do not
alter HBA or credentials to obtain access. Example only after binding 5433:

```bash
/usr/sbin/runuser -u postgres -- /usr/bin/psql -X -w \
  -h /run/postgresql -p 5433 -d memory_trust -v ON_ERROR_STOP=1 -At \
  -c 'SELECT system_identifier FROM pg_control_system();' \
  > "$WORK/cluster-system-id.txt"
```

Build an explicit mapping: `memory_trust` database to `codifix_trust`; rename
the role `memory_trust` to `codifix_trust` only if it is the actual product-owned
application role. Preserve neutral roles such as `postgres`. Never rename by
substring or wildcard. Preserve OIDs, owners, grants and exact SCRAM verifier
bytes. An MD5 verifier is a stop condition because role renaming clears it.

Map only this instance's config/release/state/cache, user watcher/manager,
operator routing and skills paths to Codifix equivalents. Preserve real
DynamicUser/private-directory layout; do not recursively chown the old tree to
a guessed numeric UID. Use the new service's managed StateDirectory and copy
only the inventoried application-owned state with correct service access.
Identify old client tools/TUI/watchers and offline queues as well as the daemon.

Have the reviewing agent check this finite mapping, planned grants and sandbox
delta now. No second human approval is needed for these described application
changes. If an unexpected protected-system change would be required, stop that
part and report the concrete reason.

## 3. Stop application writers and make one matched recovery set

Stop and disable only the enumerated old Trust application, its user manager,
watchers and other confirmed Trust writers. Record user login-manager state
and preserve the existing `Linger=no` behavior. Keep unrelated PostgreSQL,
Step CA and OpenBao running. Stop the named Trust service explicitly:

```bash
systemctl stop memory-layer@trust.service
systemctl disable memory-layer@trust.service
```

Confirm that all Trust writers are drained and no unexpected sessions remain
on the bound database. Do not kill unidentified clients. Reconcile pending
offline data first; preserve unresolved payloads and stop if they would be lost.
Take a fresh custom-format database dump plus a target-scoped role export, exact
config/credentials, attachments, offline stores, state, units/drop-ins and old
release binaries. The role export contains credential verifiers: keep it private.
Copy SQLite/DuckDB files only after their writers are stopped. Record file
hashes, content/table fingerprints, sequences, IDs and migration rows privately.

```bash
/usr/sbin/runuser -u postgres -- /usr/bin/pg_dump -w \
  -h /run/postgresql -p 5433 -Fc memory_trust > "$WORK/trust-before.dump"
sha256sum "$WORK/trust-before.dump" > "$WORK/database-backup.sha256"
```

Through the bound administrator connection, export only the discovered
application owner/runtime roles' attributes, credential verifier bytes and
relevant memberships, plus any required application tablespace metadata.
Select roles by the recorded OIDs, not name patterns; keep neutral/shared roles
as identity references without exporting their credential verifiers. Save this
target-scoped export privately and append its hash to the backup manifest.
Do not use cluster-wide `pg_dumpall --globals-only`: the Trust connection does
not establish ownership of every role or global object in that cluster.

Do one real restore test into an explicitly named, new disposable Trust-only
database using existing PostgreSQL tools. Use a separate pristine database for
the candidate schema reference. These are maintenance databases, never new
production replacements; do not point a writer/service at either. Refuse name
collisions; record their OIDs. Restore ACL/owner semantics or separately verify
the exact original grants; `--no-owner --no-acl` alone is not full recovery proof.
Check the restore against the fresh before fingerprints and migration history.
Do not restore global roles over the live cluster. Keep recovery copies.

## 4. Rehearse and apply schema 65 to 85 as the database owner

There is **no migration-only Codifix CLI**. Do not start the runtime with
superuser credentials or grant it migration authority. Use a short local
maintenance adapter with existing owner access, rehearsed on the restored
database, following this exact contract:

1. Read raw `migrations/*.sql` bytes only from the authenticated release source.
   Require exactly versions 1–85, one file per version. Validate every existing
   `_sqlx_migrations` row: success true, no duplicates/unknown versions, raw-file
   SHA-384 checksum equal to the stored bytes. Original 1–65 files are unchanged
   from the old rc4 source. Preserve those rows exactly.
2. For each missing version in order, execute its exact SQL and insert its SQLx
   success row in the **same transaction**, as the existing schema owner.
   Description is the filename stem after its numeric prefix, underscores
   replaced by spaces; checksum is SHA-384 of raw file bytes; execution_time may
   be zero. Commit only if both SQL and history insert succeed. On error roll
   back that transaction; never stamp a failed/not-executed migration successful.
3. Require exactly 85 successful rows with exact source checksums afterward.
   On interrupted execution re-read history and resume only the missing suffix.
   An uncertain commit is inspected, never blindly replayed.
4. Capture new schema objects and compare privileges. Retain pre-existing
   owner/runtime separation and grants. New objects remain schema-owner-owned;
   add only the exact table/sequence/function privileges needed for retained
   Trust operations. Do not use blanket GRANT ALL, ownership transfer or a
   default-privileges policy spanning unrelated objects. Keep runtime migration
   mode `verify_only`. Verify grants in the clone before applying the same
   explicit grant script live.
5. Compare old-table data/IDs, attachments and original migration rows. Explain
   only changes made by the authenticated migration SQL. Retirement changes
   are checked separately in the next step.

Apply the same successful adapter to the bound live database with all old
writers still stopped. A failed rehearsal does not authorize improvising on
live data. The CA agent may fix its small adapter and repeat the disposable
test in this session; it must not modify release migrations or forge history.

Rename the live database and the verified product-owned role through the
existing administrator connected to `postgres`, with no other target clients:

```sql
-- Execute only after the recorded OID/owner/SCRAM checks and target absence.
BEGIN;
ALTER DATABASE memory_trust RENAME TO codifix_trust;
ALTER ROLE memory_trust RENAME TO codifix_trust;
-- Compare database/role OIDs, owner OIDs, attributes and verifier bytes here.
COMMIT;
```

Omit the role statement if the observed runtime role was neutral/different;
record the exact chosen mapping. Do not run this as the role being renamed.
On connection loss inspect names/OIDs before retry. Update only the parsed
database/username fields in private connection settings and exact product paths;
retain passwords and tokens byte-for-byte. Rename any application-specific
pgpass entries without exposing their contents. Do not change PostgreSQL port,
cluster identity, authentication or CA/OpenBao bindings.

## 5. Retire legacy federation evidence without deleting it

Keep old federation workers stopped. Preserve signed records, receipt chains,
cached projections, original bindings and accepted work. Do not rewrite old
signatures, replay old queues into a new namespace, or truncate any table.
The source's `scripts/codifix-cutover/federation-retire.py` has SHA-256
`72cd387aedbaa5c4cb027e5c041085afcb40cfbe68c0310a1251ac7a762b5336`.
It requires Python 3 and psycopg2. Use existing tools; if absent, a narrowly
scoped `python3-psycopg2` installation from the existing configured Debian
repository is within this application-maintenance scope. Inspect the package
transaction first; do not upgrade the OS or alter repositories.

Run with explicit private `CODIFIX_CUTOVER_DATABASE_URL` bindings. Prefer local
Unix peer authentication as the existing owner; never print a credential URL
or put it in a command-line argument. The operator's schema-reference uses the
**pristine candidate database**, not the populated production target.

For peer-authenticated execution as the database OS administrator, stage a
separate authenticated source-only copy readable by that identity and a new
0700 owner-private directory for retirement inputs/outputs. Execute the Python
commands and their redirections inside that existing maintenance identity's
shell. In that shell, SOURCE and WORK below refer to these two accessible paths;
the root console retains its original WORK value and recovery files. Do not
make the root recovery directory traversable or copy passwords merely to solve
a filesystem-permission error. Copy resulting manifests back into the root
evidence set afterward. Apply the same source-access arrangement to the owner
migration adapter; preserve existing peer authentication.

```bash
# First select the pristine schema85 reference connection privately.
python3 "$SOURCE/scripts/codifix-cutover/federation-retire.py" \
  schema-reference > "$WORK/schema-reference.json"
# Then select the bound populated codifix_trust connection privately.
python3 "$SOURCE/scripts/codifix-cutover/federation-retire.py" inspect \
  --schema-reference "$WORK/schema-reference.json" \
  --local-instance "$ACTUAL_STABLE_LOCAL_INSTANCE" > "$WORK/retirement.json"
# Review payload scope and its exact sha256; do not auto-approve an unknown target.
python3 "$SOURCE/scripts/codifix-cutover/federation-retire.py" apply \
  --manifest "$WORK/retirement.json" --review-sha256 "$REVIEWED_SHA256" \
  --writers-stopped
```

Set SOURCE to the authenticated extracted source's real absolute path and use
only the stable local federation identity proved from current configuration and
database rows, not an ephemeral health ID. Review source manifests privately
with the bounded reviewer. Verify preserved fingerprints and receipt effects.
Retry only the same manifest after a crash. If no legacy spaces exist, report
that verified fact and skip retirement; do not invent a manifest. A schema or
identity mismatch stops this phase; never edit the operator to ignore it.

## 6. Install and stage the new named Trust service

Neither the create-only `console.py` nor `upgrade.py` can adopt the old
`memory-layer@trust` installation. Do not invoke them as a cross-product
upgrade. Use the following explicit placement; package hooks do not migrate
databases or start services.

```bash
dpkg --remove memory-layer       # no purge; after old writers and backups
dpkg --install "$WORK/codifix-arm64.deb"
/usr/bin/codifix --version
```

Inspect dependency failures instead of running an unrestricted apt repair.
Install only missing declared package dependencies from existing repositories
after checking that the transaction does not replace protected services.
Create these exact new paths, refusing unexpected existing contents:

```text
/opt/codifix/instances/trust/release/bin/codifix
/opt/codifix/instances/trust/release/web/
/etc/codifix-instances/trust/codifix.toml
/etc/codifix-instances/trust/codifix.sha256
/etc/codifix-instances/trust/activation-reviewed
```

Copy binary and web assets from the verified extracted package. Release paths
and ancestors must be real, root-owned and not writable by group/others, with
the binary a regular executable (not symlink). Keep configuration and digest
source files root-private. `codifix.sha256` contains exactly the binary hash
above and a newline. Keep a full tree manifest for web/source placement.

Prepare the config by parsing the original persistent credential, retaining
identities, auth settings, private credentials and application behavior, with
only the explicit renamed database/role/path fields changed. Initial activation
must include:

- `[service] bind_addr = "127.0.0.1:8091"`, `read_only = true`, web_root pointing
  to the new exact release web directory; retain the original API token.
- `[database] migrations = "verify_only"`; the owner performed all DDL already.
- Disable cluster discovery, offline replay, background work/internal automation,
  embeddings creation, curation, reinforcement, consolidation, procedural work,
  provenance scheduling and telemetry for initial read acceptance.
- MCP remains read-only and with its original exposure disabled unless a
  specific existing integration is being restored; never make HTTP MCP public.
- No old federation environment/config is loaded. Pi/supervisor/model egress
  remains off. The stock finite launcher supports federation only through a
  separate explicit flag and credential; do not smuggle it through ambient env.

Use the new `codifix@.service` and finite launcher from this package. Translate
the **actual old Trust-specific sandbox restrictions** into a new named drop-in:
retain all signing/OpenBao/backup/private-path denials, no capabilities,
NoNewPrivileges, protected system/home and original loopback boundary. The
generic template alone is insufficient evidence of CA-specific exclusions.
Remove old launcher paths/env names from the new unit; do not copy old ExecStart
over the new finite launcher. Keep source credentials inaccessible inside the
service namespace apart from the systemd-delivered credential files.

Record the reviewed configuration, binary/tree and sandbox digests in the
private execution receipt, then write the activation marker. Do not fabricate
an old installer journal or a signed release manifest. Keep the legacy unit
disabled/masked and its old release archived.

```bash
systemctl daemon-reload
systemctl start codifix@trust.service
systemctl is-active codifix@trust.service
curl --fail --silent --show-error --max-time 10 \
  http://127.0.0.1:8091/healthz
```

## 7. Accept local reads, then restore the intended local workflow

Require native version3.0.0-rc.1, correct process executable hash, schema85 with
exact checksums, the bound database/role OIDs and original stable identities.
Check authenticated project browsing, a known canonical memory, its original
history and a local query with original citations; preserve IDs/text/digests.
Do not count an unconfigured-model fallback as a generated model answer.
Check unauthenticated/cross-project denials and that read-only activation does
not bootstrap auth, register new work or replay queues. Compare old data/receipt
fingerprints; count zero alone is not evidence of preserved history.

Then restore only the previously approved local write/watch behavior: change
service.read_only to false only after preservation is verified, retain
verify_only migrations and the original suggestion-only/no-model policy,
restore approved local background features as individually inventoried, and
restart the named service. Record expected startup/audit additions separately.
Use a small explicitly labeled upgrade-completion memory as the permitted write
probe; verify the stored result rather than deleting it to hide the test.

Install/refresh the current Codifix skills from the same release and migrate
the operator-owned Trust routing/client configuration to its new paths. Preserve
the exact area UUID/project mapping and private auth. Ordinary user and root
`codifix` commands must select Trust; unconfigured General must still fail
closed. Restore only previously active user manager/watchers with `codifix`
commands and the same area/project scopes. Do not enable linger or create broad
tokens. Inspect the new CLI `--help` before invoking a management command.

Verify a service restart, then enable only `codifix@trust.service` and those
previously enabled local application jobs. Do not reboot the CA. Verify old
units cannot start, old executable links are no longer discoverable, and no
running process uses old Memory code. Preserve the archived executable and old
config/history; remove source-installed old commands only from an exact file
inventory, never a wildcard delete. Do not remove the PostgreSQL runtime or
shared application support files while they are still referenced.

## 8. Re-enrol federation with the same scope, or report it separately

The approved rename strategy archives legacy evidence and creates fresh Codifix
enrolments. Local upgrade does not automatically grant gateway authority.
Use only the existing authenticated operator enrollment workflow with the
coordinator, preserve stable Trust ownership and original approved collections,
and request no new model-disclosure scope. Preserve original signing history;
new namespaces must not reinterpret old signatures. Never fabricate a gateway
receipt, edit immutable accepted history, or reuse a retired queue/binding.

If the CA agent has current, valid operator access and the required new
coordinator binding, complete enrollment and enable only the reviewed new
federation credential/network destination. Verify same-scope sync and one
deterministic local-human cited exchange/replay using the established workflow.
Do not enable Pi/agent/model calls or add public listeners to achieve this.

Otherwise finish and verify local Codifix operation, keep federation off, and
post one concise coordination request in CURRENT ACTION with sanitized stable
instance/project/collection IDs and required capabilities. Report
`LOCAL_COMPLETE_FEDERATION_PENDING`. This is an external dependency, not a
reason to abandon the completed local upgrade or ask to repeat its approval.

## 9. Recovery and resumption

Keep a private journal of started/completed phases, exact object IDs, hashes and
effects. On interruption inspect that journal plus actual database/unit/process
state before taking another action. Do not replay a fresh install, regenerate
a retirement manifest over partially retired state, or assume a lost command
response means failure.

Before live schema/data changes, the old exact release/config can be restored
and only its previously active jobs restarted. Once schema85 or new writes
exist, do not simply restart rc4 against that database. Prefer fixing forward
with application writers stopped. A matched old database/config/state restore
is allowed only before new user/federation work and only from this run's verified
recovery set; preserve the failed candidate state too. Never restore an old
database over accepted new work or independently roll back published records.
If no bounded forward repair is possible, leave application writers stopped
and report exact status and next action. Step CA/OpenBao continue unchanged.

## 10. Final verification and Notion result

Repeat the initial HTTPS health checks, protected service PID/state comparison,
SSH masked/inactive/no-port22 check and listener-scope comparison. Require CA
and OpenBao health, no new public application port, and real Trust sandbox
denial of protected paths under its actual service identity. Do not read secret
contents to prove denial. A changed protected-service PID must be explained;
do not conceal it or automatically restart a protected service.

Post results into RESULT INBOX's page body immediately before `CA_RESULT_END`,
preserving prior history. Do not rely on comments. Fetch the page again and
verify your saved report. Link the result from CURRENT ACTION and mark this
instruction completed or partially completed accurately. Return only sanitized
metadata, hashes and aggregate counts, never config, connection strings,
passwords, tokens, keys, private records or full command logs.

Use this report shape, with actual values rather than claimed defaults:

```json
{
  "instruction_id": "ca-codifix-upgrade-20261018-v1",
  "status": "COMPLETE | LOCAL_COMPLETE_FEDERATION_PENDING | BLOCKED | FAILED",
  "captured_at_utc": "actual UTC timestamp",
  "source_commit": "66c2d1ecc4ca9b0c052e1d854f71a0703f998b71",
  "package_sha256": "9f6bd3b5ad4cf58423846ef8c7ab80138a1f62170edd4b8a7b24c3a55c2540dd",
  "artifact_authentication": "explicit_pinned_hash_unsigned_internal",
  "binary_sha256": "observed running executable hash",
  "schema_before": 65,
  "schema_after": 85,
  "old_migrations_unchanged": "observed boolean",
  "database_role_oids_preserved": "observed boolean",
  "knowledge_history_attachments_preserved": "observed checks and aggregate differences",
  "recovery_restore_verified": "observed boolean and private manifest hash",
  "retirement": "receipt hash or verified no-spaces",
  "local_reads_and_write_probe": "observed result",
  "service_restart": "observed result",
  "operator_cli_tui_skills_watchers": "observed result",
  "federation": "same-scope new binding verified or explicit pending request",
  "protected_services_unchanged": "observed result",
  "ssh_disabled": "observed boolean; no listener",
  "remaining_work": [],
  "private_evidence_location": "non-secret root-private path",
  "independent_review": "actual reviewer and result, or unavailable"
}
```

Do not mark the whole rename complete merely because package installation or a
health endpoint passed. Conversely, do not delay a verified local cutover for
unrelated Pi, signing/notarization, OS changes or additional generic test suites.
