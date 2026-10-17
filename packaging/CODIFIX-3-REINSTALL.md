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
Database names and roles remain unchanged except for the exact product-owned Mac
onboarding names listed below. Existing state is transferred only by the
reviewed release cutover and reconciliation procedure, never by installer hooks;
no database is dropped, recreated or renamed by a broad name match.

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
2. Connect to its `postgres` maintenance database using a **separate, already
   existing, reviewed superuser**, not any role being renamed. Use the existing
   private password file and `psql -X -w`, the explicit loopback host/port,
   `-U` for that maintenance login and `-d postgres`. No credentials belong in
   command arguments or output. If no separate maintenance superuser exists,
   stop here for a separately reviewed recovery procedure: do not create a new
   privileged login, weaken authentication or edit PostgreSQL catalogs.
3. Record a private metadata-only before snapshot of the exact database and
   role OIDs, database-owner OIDs, attributes, grants, area/project IDs and
   content/attachment counts. Match the actual selected areas to the exact
   names below; omit both statements for an area that was never installed.
   Refuse a partial role/database pair, any target-name collision, an unexpected
   owner, or another active connection. Preserve the metadata with the matched
   recovery set, not in Git.
4. Check that every old login below uses SCRAM. As the reviewed maintenance
   superuser, the following returns only names and booleans, never a verifier:

   ```sql
   SELECT rolname, rolpassword LIKE 'SCRAM-SHA-256$%' AS scram_password
   FROM pg_authid
   WHERE rolname IN ('memory_onboarding_admin', 'memory_laptop_workshop',
                     'memory_laptop_homelab', 'memory_laptop_general')
   ORDER BY rolname;
   ```

   Require `true` for every expected row and the exact expected row count.
   An MD5 or missing verifier is a stop condition: a role rename clears an MD5
   password, so it cannot meet this password-preserving procedure. PostgreSQL
   also forbids renaming the current session user. See
   [ALTER ROLE](https://www.postgresql.org/docs/17/sql-alterrole.html).
5. In that same separate-maintenance session, run `\set ON_ERROR_STOP on`,
   then the reviewed exact SQL below. Retain only the area pairs proven present
   in step 3. The maintenance database is `postgres`, so it is not a database
   being renamed. No other database sessions may remain; inspect and resolve
   an unexpected session instead of terminating it automatically.

   ```sql
   BEGIN;
   ALTER DATABASE memory_laptop_workshop RENAME TO codifix_laptop_workshop;
   ALTER ROLE memory_laptop_workshop RENAME TO codifix_laptop_workshop;
   ALTER DATABASE memory_laptop_homelab RENAME TO codifix_laptop_homelab;
   ALTER ROLE memory_laptop_homelab RENAME TO codifix_laptop_homelab;
   ALTER DATABASE memory_laptop_general RENAME TO codifix_laptop_general;
   ALTER ROLE memory_laptop_general RENAME TO codifix_laptop_general;
   ALTER ROLE memory_onboarding_admin RENAME TO codifix_onboarding_admin;
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

A failure before commit requires `ROLLBACK` and inspection. A failure after
commit keeps all clients stopped until configuration/credential reconciliation
or matched restoration has completed; do not alternate old and new clients.

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
