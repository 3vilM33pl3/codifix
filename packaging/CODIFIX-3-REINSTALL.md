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

The database names, roles, semantic `memory_entries` tables, canonical IDs and
area topology are preserved. Do not rename a database or recreate it merely
because its name contains `memory`. Existing state is transferred only by the
reviewed release cutover and reconciliation procedure, never by installer hooks.

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
