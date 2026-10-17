# Codifix 3.0.0 release-specific reinstall

The technical rename is a breaking, single-user cutover. The old Memory Layer
installation is retired explicitly before installing Codifix. The new package
identities do not upgrade the old product and provide no old command aliases.
This document prepares the packaging part of the approved cutover; it does not
authorize installation, service activation or a live database transformation.

## Freeze and preserve first

Freeze one reviewed source commit and its SHA-256 artifact inventory. Keep the
old signed packages, original release manifests, signatures and release notes
unchanged. Preserve a matched recovery set containing the database snapshot,
attachments, configuration, environment files, state directories, credentials,
service definitions and exact old executables. Keep recovery material private.
Record which services and watchers were active; inactive ones stay inactive.

Semantic `memory_entries` tables, canonical IDs and area topology are preserved.
Rename product-owned database and role names only through the exact reviewed
per-instance inventory, including Monolith and the Mac onboarding names below.
Generic PostgreSQL names and neutral area/runtime roles remain unchanged.
Existing state is transferred only by the reviewed release cutover and
reconciliation procedure, never by installer hooks; no database is dropped,
recreated or renamed by a broad name match. The private Monolith command ledger
records proposed mappings and cold preparation steps; its existence does not
establish a completed rehearsal or authorize copying commands across clusters.

| Platform | Old installation | New installation |
| --- | --- | --- |
| Debian | `memory-layer`, `/usr/bin/memory`, `/etc/memory-layer`, `/var/lib/memory-layer` | `codifix`, `/usr/bin/codifix`, `/etc/codifix`, `/var/lib/codifix` |
| Debian named areas | `memory-layer@.service`, `memory-area-*`, `/etc/memory-layer-instances`, `/opt/memory-layer/instances` | `codifix@.service`, `codifix-area-*`, `/etc/codifix-instances`, `/opt/codifix/instances` |
| macOS CLI | `/usr/local/bin/memory`, `/usr/local/share/memory-layer`, `~/Library/Application Support/memory-layer` | `/usr/local/bin/codifix`, `/usr/local/share/codifix`, `~/Library/Application Support/codifix` |
| macOS apps | `Memory.app`, `Memory Network Access.app`, `dev.memory-layer.*` and `com.memory-layer.*` | `Codifix.app`, `Codifix Network Access.app`, `dev.codifix.*` |
| Windows user MSI | `%LocalAppData%\Programs\Memory Layer`, `HKCU\Software\Memory Layer` | `%LocalAppData%\Programs\Codifix`, `HKCU\Software\Codifix` |
| Windows server tools | `%ProgramFiles%\Memory Layer Native Server Tools`, `%ProgramData%\MemoryLayer` | `%ProgramFiles%\Codifix Native Server Tools`, `%ProgramData%\Codifix` |

## Linux

1. Stop the explicitly inventoried old writers/watchers and units. Disable the
   retired units so they cannot restart after reboot; retain their definitions
   and enabled/active-state record in the recovery set.
2. Remove the old package with `dpkg --remove memory-layer`. Do not purge it:
   the old configuration and state are part of the recovery set. Remove old
   source-installed commands/aliases only after recording their exact paths and
   recoverable copies. Never delete directories by a name wildcard.
3. Install the reviewed architecture-specific `codifix_3.0.0~rc.1_ARCH.deb`
   (stable: `codifix_3.0.0_ARCH.deb`). The maintainer scripts do not start or
   restart services, provision databases or run migrations.
4. Stage the reviewed Codifix configuration/state and area release bindings.
   Compare content/IDs/attachments and permissions before enabling any service.
   Bring Workshop up first, keeping background and federation effects off until
   their separate cutover gates pass.

Before renaming any database or role, bind the procedure to the exact cluster
system identifier, data directory, database/role OIDs, owner/grant inventory and
current connection state. Preserve SCRAM verifier bytes and object identities;
refuse MD5 credentials, unexpected clients or target-name collisions. A bootstrap
administrator that must itself be renamed needs the reviewed temporary-session
authorization technique described below, rehearsed with that instance's exact
mapping. Keep disabled restore-check databases disabled. The shared host dev
cluster requires its own client and privilege inventory and remains separate
from Workshop; do not stop unrelated PostgreSQL services or change authentication
rules to make the rename possible. The [four-cluster SQL/authentication rehearsal](../docs/reviews/codifix-rename/evidence/monolith-name-rehearsal.md)
passed 114 recorded checks using synthetic credentials and fixture-bound catalog IDs;
it does not establish a live cluster match or a populated operational transfer.

The disposable hosted installer check covers fresh installation, retained
configuration/data, repair and removal. It is not a populated old-name cutover
rehearsal. That rehearsal remains a separate release gate.

## macOS

1. Rehearse connection metadata and Keychain transfer/access before uninstalling
   the old app. The namespace changes to `dev.codifix.*`; preserve exact
   principal/profile binding and prove the new app can read the transferred
   entries. The repository's `scripts/codifix-cutover/macos-state.swift` is the
   reviewed release-specific state tool; inspect its supported commands first.
2. Quiesce the explicitly inventoried old LaunchAgents and sync helpers. Record
   their active states and preserve app/CLI/configuration/runtime bytes.
3. Uninstall the old Homebrew formula through Homebrew, or retire the exact
   inventoried old PKG payload through the reviewed file inventory. Forgetting a
   PKG receipt alone does not remove its payload. Keep the original receipts and
   packages available for recovery. Do not delete old data or Keychain entries.
4. Install the standard `codifix-VERSION-macos-ARCH.pkg` and separately reviewed
   Codifix application, then stage/reconcile the preserved state. The standard
   PKG uses `dev.codifix.pkg`; runtime receipts use
   `dev.codifix.runtime.<manifest-sha256>`.
5. Use `codifix-VERSION-macos-ARCH-application-update.pkg` only after a Codifix
   installation exists. Its guard requires `/usr/local/bin/codifix`; it cannot
   migrate an old Memory installation. Its separate `dev.codifix.application`
   receipt preserves independently owned runtime bytes.

Both Intel and Apple Silicon need native signing, notarization, fresh install,
state-transfer and access checks. Source/synthetic tests do not establish those.
The Homebrew formula is intentionally HEAD-only until a real frozen Codifix
source archive and independently calculated release checksum exist. Publish a
pinned formula only after that artifact gate; do not reuse the old 2.1.0 hash.

### Existing native onboarding databases and roles

New installations use `codifix_onboarding_admin` plus matching
`codifix_laptop_workshop`, `codifix_laptop_homelab` and
`codifix_laptop_general` database/login pairs for the selected areas. The old
product-owned names need the following one-time cold rename during the approved
single-user window. This is a manual exact-name procedure, not an installer
migration or an automatic adoption path.

1. Preserve the matched cluster/configuration/credential recovery set. Quiesce
   the old application, sync, watcher and onboarding LaunchAgents so nothing can
   reconnect. Run only the PostgreSQL maintenance server for the exact reviewed
   local cluster and port; keep application and background services stopped.
2. Verify the exact reviewed PostgreSQL executable/major, local data directory,
   cluster system identifier and loopback port against the recovery inventory.
   Connect to its `postgres` maintenance database as the original
   `memory_onboarding_admin` bootstrap superuser using the existing private
   password file, `psql -X -w -v ON_ERROR_STOP=1`, and explicit loopback host/port.
   Do not put credentials in command arguments or output. Confirm its recorded
   bootstrap OID is `10`, its login/superuser attributes and SCRAM credential
   match the reviewed source, and no other client sessions remain. A mismatched
   cluster, OID or login is a stop condition; do not discover another target or
   weaken authentication. No second persistent administrator is required.
3. Record a private metadata-only before snapshot of the exact database and
   role OIDs, database-owner OIDs, attributes, grants, area/project IDs and
   content/attachment counts. Match the actual selected areas to the exact
   names below; omit both statements for an area that was never installed.
   Refuse a partial role/database pair, any target-name collision, an unexpected
   owner, or another active connection. Preserve the metadata with the matched
   recovery set, not in Git. The exact temporary role
   `codifix_cutover_admin_20261017_01` must also be absent: a collision is a stop
   condition, never an existing role to adopt or delete.
4. Check that every old login below uses SCRAM. As the verified original
   bootstrap administrator, this returns only names, OIDs and booleans:

   ```sql
   SELECT oid, rolname, rolpassword LIKE 'SCRAM-SHA-256$%' AS scram_password
   FROM pg_authid
   WHERE rolname IN ('memory_onboarding_admin', 'memory_laptop_workshop',
                     'memory_laptop_homelab', 'memory_laptop_general')
   ORDER BY rolname;
   ```

   Require `true` for every expected row and the exact expected row count.
   An MD5 or missing verifier is a stop condition: a role rename clears an MD5
   password, so it cannot meet this password-preserving procedure. PostgreSQL
   forbids renaming the current session user. Its supported
   [SET SESSION AUTHORIZATION](https://www.postgresql.org/docs/17/sql-set-session-authorization.html)
   lets the authenticated superuser temporarily change that session identity;
   `RESET` restores the original authenticated role by OID, including its new
   name. See also [ALTER ROLE](https://www.postgresql.org/docs/17/sql-alterrole.html).
5. In the verified original-administrator session, run the exact transaction
   below with `ON_ERROR_STOP=1`. Retain only the area pairs proven present in
   step 3. The maintenance database is `postgres`, so it is not being renamed.
   The temporary superuser is `NOLOGIN`, has no password, receives no grants or
   objects, and is created and removed within this single transaction. Do not
   commit an intermediate state, reconnect as it, or add any other operations
   while it is the session identity. The assertion must pass before its removal
   and commit. The [disposable PG17 evidence](../docs/reviews/codifix-rename/evidence/macos-admin-rename.md)
   describes the tested identity/reset, rollback and credential behavior.

   ```sql
   \set ON_ERROR_STOP 1
   BEGIN;
   DO $cutover$
   BEGIN
     IF current_database() <> 'postgres'
        OR session_user <> 'memory_onboarding_admin'
        OR current_user <> 'memory_onboarding_admin'
        OR NOT EXISTS (
          SELECT FROM pg_authid WHERE oid = 10
          AND rolname = 'memory_onboarding_admin' AND rolsuper AND rolcanlogin
          AND rolpassword LIKE 'SCRAM-SHA-256$%'
        ) THEN
       RAISE EXCEPTION 'reviewed bootstrap identity/SCRAM required';
     END IF;
     IF EXISTS (SELECT FROM pg_stat_activity
                WHERE backend_type = 'client backend' AND pid <> pg_backend_pid()) THEN
       RAISE EXCEPTION 'other client sessions remain';
     END IF;
     IF EXISTS (SELECT FROM pg_roles WHERE rolname IN (
          'codifix_onboarding_admin', 'codifix_laptop_workshop',
          'codifix_laptop_homelab', 'codifix_laptop_general',
          'codifix_cutover_admin_20261017_01'))
        OR EXISTS (SELECT FROM pg_database WHERE datname IN (
          'codifix_laptop_workshop', 'codifix_laptop_homelab',
          'codifix_laptop_general')) THEN
       RAISE EXCEPTION 'rename target/temporary role collision';
     END IF;
     IF EXISTS (SELECT FROM pg_authid WHERE rolname IN (
          'memory_onboarding_admin', 'memory_laptop_workshop',
          'memory_laptop_homelab', 'memory_laptop_general')
          AND (rolpassword IS NULL OR rolpassword NOT LIKE 'SCRAM-SHA-256$%')) THEN
       RAISE EXCEPTION 'SCRAM required for every renamed login';
     END IF;
     PERFORM set_config('codifix.cutover_admin_oid',
                        (SELECT oid::text FROM pg_roles WHERE rolname = current_user), true);
   END
   $cutover$;
   ALTER DATABASE memory_laptop_workshop RENAME TO codifix_laptop_workshop;
   ALTER ROLE memory_laptop_workshop RENAME TO codifix_laptop_workshop;
   ALTER DATABASE memory_laptop_homelab RENAME TO codifix_laptop_homelab;
   ALTER ROLE memory_laptop_homelab RENAME TO codifix_laptop_homelab;
   ALTER DATABASE memory_laptop_general RENAME TO codifix_laptop_general;
   ALTER ROLE memory_laptop_general RENAME TO codifix_laptop_general;
   CREATE ROLE codifix_cutover_admin_20261017_01 NOLOGIN SUPERUSER;
   SET SESSION AUTHORIZATION codifix_cutover_admin_20261017_01;
   ALTER ROLE memory_onboarding_admin RENAME TO codifix_onboarding_admin;
   RESET SESSION AUTHORIZATION;
   DO $cutover$
   BEGIN
     IF session_user <> 'codifix_onboarding_admin'
        OR current_user <> 'codifix_onboarding_admin'
        OR (SELECT oid FROM pg_roles WHERE rolname = current_user)
           IS DISTINCT FROM current_setting('codifix.cutover_admin_oid')::oid THEN
       RAISE EXCEPTION 'renamed original administrator/OID not restored';
     END IF;
   END
   $cutover$;
   DROP ROLE codifix_cutover_admin_20261017_01;
   COMMIT;
   ```

   This changes names while retaining object OIDs, ownership/grants, SCRAM
   verifiers and database contents. It does not use `OWNER TO`, reset passwords,
   create replacement databases, rewrite rows or run application migrations.
   PostgreSQL requires a different connected database for
   [ALTER DATABASE RENAME](https://www.postgresql.org/docs/17/sql-alterdatabase.html).
6. Keep clients stopped while updating exact private references: the username
   and database path in every selected area's TOML `database.url` and replica
   profile's `local_database_url`; explicit environment/connection-service
   overrides; and any name-specific authentication rules. Keep the URL password
   bytes unchanged. Rename `memory_onboarding_admin.pgpass` to
   `codifix_onboarding_admin.pgpass` and each installed
   `memory_laptop_AREA.pgpass` to `codifix_laptop_AREA.pgpass`, refusing existing
   targets. Change only their username field and any exact database field from
   the old to the new name; retain wildcard database fields, host, port and
   password bytes. Keep ownership and mode `0600`. The existing
   `local-secrets.json` and `admin-password` values are unchanged.
7. Reconnect through each new private password-file/role/database binding and
   compare the saved OIDs, owner OIDs, grants, attributes, content digests and
   counts before bringing up Codifix. Validate the new admin login separately.
   Retain old journals/configuration/credential copies as recovery evidence.
   Old onboarding journals bind old helper digests and generated-file bytes:
   do not edit or replay them as an upgrade, and do not rerun `initdb` or
   create-only onboarding against the populated root. Register only the
   reviewed new runtime bindings after the configuration/state cutover gates
   pass; leave retired wrappers and previously inactive jobs disabled.

A failure before commit requires `ROLLBACK` and inspection; the rename and
transaction-created temporary role must both disappear. If the connection is
lost or the commit acknowledgement is uncertain, inspect old/new names, the
original role OID, temporary-role absence and database OIDs before doing anything
else. Do not blindly rerun: a committed rename makes the old login invalid, and
a conflicting role must never be adopted or removed. A confirmed commit keeps
all clients stopped until configuration/credential reconciliation or matched
restoration has completed; do not alternate old and new clients. This rehearsed
procedure is preparation only; live application still requires all approved
cutover gates.

## Mac preserved-runtime binding

The old completed onboarding journal is immutable recovery evidence. Use the
release-specific [Mac runtime handover operator](../scripts/codifix-cutover/README.md#mac-preserved-runtime-handover-30-only)
for the already populated, renamed root; retain its four-file operator bundle
at a durable private path independent of the final checkout rename. Never edit
old journal pins/stages or replay `onboard.py apply` against this root.

This is a separate gate after the preceding role/database/filesystem and private
credential/configuration reconciliation. `inspect` and `bind` check the exact
renamed cluster through a separately reviewed loopback-only maintenance
PostgreSQL while old jobs/application writers are stopped. Bind publishes only
cold configs/inert plists and a final receipt; it does not start PostgreSQL or
services. Stop maintenance PostgreSQL cleanly after binding. Its new guarded
PostgreSQL launcher can then start the stopped populated cluster without a
running-database dependency. Area launchers require that exact server ready and
repeat the SQL identity checks. No sync/watch launcher is provided.

Review the [input/SQL boundary and current evidence](../docs/reviews/codifix-rename/evidence/macos-runtime-rebind.md)
before activation. Python fixture tests establish refusal/retry boundaries;
actual Darwin binaries, an independently populated native fixture and the live
cold-window acceptance gates remain required. Do not infer live readiness from
a successful bind or register any inert plist automatically.

## Windows preview

1. Preserve the exact installed user/server-tools MSI, ZIP, named releases,
   configurations, database clusters, Credential Manager entries and SCM/task
   state. Stop and disable the exact old services/tasks before removing tools.
2. Uninstall the old user MSI and old server-tools MSI explicitly using Windows
   Apps/installed-product records. They retain different upgrade families from
   Codifix. Do not ask Windows Installer to treat the new product as an upgrade
   of the old one. Retain the old `%ProgramData%\MemoryLayer` state.
3. Install the reviewed Codifix ZIP/user MSI/server-tools MSI. The Codifix
   per-user UpgradeCode is `C956DBF2-EE64-5A94-B05E-77D35B330F67`; server tools
   use `356D7E23-5C49-5D9B-BE8E-D6BB9A02F958`.
4. Stage the reviewed state/configuration and exact protected runtime releases,
   then explicitly validate native service identity, credentials and readiness.
   Do not activate an installation merely because an MSI completed.

The release-specific installer ledger allocates `3.0.0-rc.1` build 1 (MSI
`3.0.1`) and stable `3.0.0` build 2 (MSI `3.0.2`). Never republish different
bytes under a used application/build identity. Further candidates require a new
reviewed allocation. Windows remains preview; no native dependency/runtime
parity is claimed by the rename or a successful package build.

## Recovery

Before new remote federation effects, stop Codifix and restore the matched old
package/configuration/state/database/credential set together, then restore only
the previously active services. Keep both old and new recovery sets. After
remote effects, recover forward or coordinate restoration across participants;
a local package rollback cannot undo published records. Delete no recovery
material without explicit approval.
