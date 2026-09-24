# CA fast replacement: Memory 2.1.0-rc.1 and PostgreSQL 17

Authorized direction, 24 September 2026: replace the existing PostgreSQL 15
cluster with PostgreSQL 17 and upgrade the existing Memory singleton to the
latest signed deployable release. Do not create a parallel `17/trust` cluster.

This is one autonomous maintenance operation. The CA agent performs internal
checks and reports once. It keeps PostgreSQL 15 only through the upgrade and
acceptance window, then deletes the old cluster and major-version packages after
PostgreSQL 17 and Memory 2.1 pass. The verified encrypted backup and local
evidence remain. The previous Memory binary remains rollback material.

## Observed completion

The CA agent reported `UPGRADED` at 2026-09-24 21:55 UTC for operation
`303ed4b9-c592-4ade-b468-d4cdd4077487`. This is CA-reported evidence retained
under
`/var/backups/memory-layer/ca-fast-upgrade-303ed4b9-c592-4ade-b468-d4cdd4077487`.

- PostgreSQL `17/main` is online on port 5432 and its inventory matches the
  former `15/main` cluster. The old cluster was retained through acceptance and
  then deleted.
- Memory `2.1.0-rc.1` is active with database health `up`, the expected stable
  `service_id`, successful authenticated read/write, and passing Web/TUI smoke
  checks. The per-process `instance_id` changed as expected.
- Step CA and OpenBao were confirmed not to be PostgreSQL clients and were not
  changed.
- The encrypted backup, isolated-restore proof, upgrade receipts and previous
  Memory artifact remain retained.
- PostgreSQL 15 server/client package binaries remain installed. The required
  no-autoremove purge simulation would also upgrade the PostgreSQL metapackage
  and install unauthorized PostgreSQL 18 packages, so no package transaction was
  run. This is an accepted safe deviation: the retired data cluster is deleted,
  and the inert packages must not be purged until a future simulation is both
  bounded and version-safe.

Receipt hashes reported by the CA agent:

| Receipt | SHA-256 |
| --- | --- |
| PostgreSQL upgrade check | `ae831adb0cd7b0a19b1e636ad6ec06bf65ccacf7fffcea3b02a0b9d4c6be5a5d` |
| PostgreSQL upgrade receipt | `664e37433d5e8439dbf58962ac4715797dd46ef6880405654a81806e7e97152e` |
| Encrypted backup | `962fccf1176d652ef50b604fea58927c2071441d273efa7aac4ba5618d275dd7` |
| Isolated restore receipt | `573150e503d1bf8c28375f3d8775a718cd8156f15bd7fbf93ceae4cb4730ed24` |

The database, backend and interactive toolchain replacement are complete. The
CA agent reported `RECONCILED` at 2026-09-24 22:32 UTC:

- The ordinary `memory` command resolves to `/usr/bin/memory`, reports
  `2.1.0-rc.1`, and matches SHA-256
  `897c368547e608062fdcd050902fc0c8906c47da6b928989f115c617c35ab119`.
- The backend executes `/usr/bin/memory`, reports `2.1.0-rc.1`, and remains
  healthy against PostgreSQL `17/main` on port 5432.
- Two legacy `2.0.1` command-shadow binaries were found. Both had the same
  authenticated hash, were preserved in the private evidence root, and their
  command paths now redirect to the packaged binary.
- No watcher manager was installed, the legacy watcher was inactive and
  disabled, and no repositories were manager-tracked. Consequently no watcher
  was enabled and no repository skill files were changed.
- A fresh packaged TUI passed, the legacy TUI was closed, rollback copies remain
  retained, and Step CA and OpenBao stayed active and unchanged.

Trust import, the four-area split, federation and Pi activation remain separate
work.

## Post-upgrade toolchain reconciliation

The retained `/usr/local/bin/memory` can shadow the packaged
`/usr/bin/memory`. In addition, a `2.0.1` TUI labels its TUI, service, manager
and watcher rows with its own compile-time version; those repeated labels are
not independent measurements of the running processes.

This completed follow-up is retained as the reconciliation procedure. It must
not change PostgreSQL, Memory data, Step CA, OpenBao, Trust data or federation:

1. Record `type -a memory`, `command -v memory`, both binary hashes and both
   `--version` results. Verify `/usr/bin/memory` is `2.1.0-rc.1` with SHA-256
   `897c368547e608062fdcd050902fc0c8906c47da6b928989f115c617c35ab119`.
   Independently record backend, watcher-manager and watcher executable paths
   using systemd and `/proc/<pid>/exe`.
2. Preserve the old `/usr/local/bin/memory` and its hash inside the existing
   private evidence root. Atomically replace the shadowing path with a symlink
   to `/usr/bin/memory`; keep the preserved rollback copy. Refresh the shell
   command cache and verify that the ordinary `memory` command resolves to the
   authenticated packaged binary.
3. As the existing watcher-manager owner, retain its config path, disable the
   manager with `/usr/bin/memory` so its tracked old watchers stop, and re-enable
   it with the same config using `/usr/bin/memory`. Restart the legacy system
   `memory-watch.service` only if it was active; do not enable a previously
   disabled watcher.
4. For each manager-tracked repository only, make the skills command fall back
   to the installed package template by setting
   `MEMORY_LAYER_GITHUB_SKILL_TEMPLATE_ARCHIVE_URL` to a verified nonexistent
   local file URL. Run `/usr/bin/memory upgrade --dry-run --json`, require all
   selected template paths to be under
   `/usr/share/memory-layer/skill-template`, then run
   `/usr/bin/memory upgrade` without `--force`. Retain the automatic backups and
   do not replace unrelated or newer custom skills.
5. Close the running old TUI and launch `/usr/bin/memory tui`. Reconfirm backend
   version and health, then verify actual manager/watcher executables through
   `/proc/<pid>/exe`; do not treat repeated TUI version labels as proof.
6. Report the resolved command path and version, independently observed process
   executable paths, the number of repositories checked and upgraded, skill
   backup roots, health status and any deviations. Keep usernames, credentials,
   configuration contents and private repository paths out of Notion.

## Why this is the fastest path

- The signed ARM64 `2.1.0-rc.1` release already contains the current application
  functionality. Changes after its source revision are packaging, handoff,
  receiver and documentation work, not a newer Memory runtime release.
- The existing CA Memory singleton is reported to use the shared Monolith
  database. Upgrading its application does not need to wait for a Trust dataset
  split or a new local Memory database.
- Debian's `pg_upgradecluster` can replace `15/main` with `17/main` on the
  original port while retaining the old cluster stopped and manual for rollback.
  Use copy-mode `pg_upgrade`, not dump/restore, link mode or clone mode.
- The signed Memory package does not automatically restart the existing service.
  Package installation and the one explicit service restart remain separate and
  observable.

## Exact targets

| Resource | Target |
| --- | --- |
| OS | Existing Debian 12 ARM64; unchanged |
| PostgreSQL production cluster | `17/main`, original port 5432 |
| PostgreSQL temporary rollback cluster | Existing `15/main`, retained stopped/manual only until PostgreSQL 17 and Memory acceptance, then deleted |
| Memory package | Signed `memory-layer_2.1.0-rc.1_arm64.deb` |
| Memory package SHA-256 | `34894b000cd1d9e71890d1415e3ab7ac5e6c47f06f7494b48b9e1283a17ab71a` |
| Packaged `/usr/bin/memory` SHA-256 | `897c368547e608062fdcd050902fc0c8906c47da6b928989f115c617c35ab119` |
| Memory source revision | `b35faf2983d20c69bc086fb9bcb7e95c51bd8f47` |
| Memory schema compatibility | floor 60, head 60, no migration |
| Release manifest SHA-256 | `5d06127ec4db38e0d56eb0942a41f26bee32987645f1bf81391fb929e19ee1d4` |
| Release signing subkey | `4EB55288032C60EF65C73A0352EE81AD6D98F325` |

Use the repository and package identities in
[the PGDG binding](PGDG-BOOKWORM-ARM64-BINDING.md). Install the bound
PostgreSQL-17 server/client and any exact PostgreSQL-17 equivalents required by
extensions actually present in `15/main`. Do not install development packages.

Latest independently verified repository observation, 24 September 2026
20:53:49 UTC: InRelease SHA-256
`37f6aa488afe294015f1328834843b215fbcbde62c41e339c9ff1dcf9930c34b`
and ARM64 `Packages.gz` SHA-256
`e6dec1f2e121c9f47903a85438c742ecf5442aa695d8d8a6340ca71dfc235f69`.
PGDG legitimately regenerates its signed index. A later index identity is not a
stop condition by itself: verify its signature under the bound fingerprint,
verify its signed ARM64 index entry, and require every authorized package's
exact version and `.deb` hash to remain unchanged. Stop for a signing-key,
signature, architecture, version or `.deb`-hash mismatch. Do not select a newer
candidate.

## Execute once, in this order

1. **Supersede the old plan.** Do not create `17/trust`. If an earlier operation
   already changed the host, inventory it and reconcile safely before continuing.
2. **Capture the real topology with local privilege.** Record current cluster
   identity, databases, sizes, extensions, roles, tablespaces, subscriptions,
   replication slots and PostgreSQL-dependent services. Inspect the protected
   Memory configuration only far enough to report `database_binding=monolith`,
   `local`, or `other`; never publish the URL or credentials.
3. **Preserve rollback inputs.** Take and hash an encrypted logical backup of
   all PostgreSQL-15 globals and databases, preserve its configuration and exact
   package inventory, and prove an isolated restore. Preserve the current Memory
   package/binary, configuration, offline DuckDB and service/unit state. Record
   process start identities for Memory, PostgreSQL, Step CA and OpenBao.
4. **Install but do not switch.** Authenticate and install the pinned PGDG
   repository/packages. Suppress automatic service starts and automatic cluster
   creation. Verify no PostgreSQL-17 cluster was created. Authenticate the signed
   Memory ARM64 package and stage/install it; its package hook must not restart
   Memory. Do not start a second Memory instance.
5. **Run the PostgreSQL compatibility check.** Install exact PostgreSQL-17
   equivalents for every external extension found in the old cluster. Run the
   copy-mode upgrade precheck. Stop if it reports an unresolved extension,
   incompatible setting, untrusted source-superuser object or missing space.
   The retained `postgresql-server-dev-15` package is build-time headers, not a
   runtime extension. If it is the only package-parity failure, simulate removal
   without autoremove. Continue only if the simulation removes exactly
   `postgresql-server-dev-15` and no reverse-dependent installed package; then
   remove only that obsolete header package and rerun the unforced precheck. Do
   not install `postgresql-server-dev-17` and do not use `--force-packages`.
6. **Take the short maintenance window.** Stop Memory and any locally confirmed
   clients of `15/main`. Do not change their configuration, keys or identities.
   Run the Debian cluster upgrade equivalent of:

   ```text
   pg_upgradecluster -v 17 --method=upgrade --no-start --jobs=<bounded CPU count> 15 main
   ```

   Do not use `--link`, `--clone`, `--keep-port` or an unpinned destination.
   The resulting `17/main` must own the original production port 5432. The old
   `15/main` must remain present, stopped and manual on its reassigned port.
7. **Validate PostgreSQL 17 before clients return.** Start `17/main`; verify
   server version 17.11, database/role/extension inventories, ownership,
   tablespaces, representative counts and application read/write probes. Run all
   upgrade-generated extension/reindex/statistics actions before declaring the
   affected objects usable. Confirm `15/main` is stopped and was not deleted.
8. **Upgrade the existing Memory service.** Verify `/usr/bin/memory` has the
   bound hash above and reports `2.1.0-rc.1`. Preserve the effective unit and
   drop-ins. If the active unit names `/usr/local/bin/memory`, add a reversible
   systemd drop-in that changes only the executable token to `/usr/bin/memory`,
   preserving the existing arguments, environment, config path, user and
   service identity. Confirm the configured shared/local database is reachable
   and at schema 60. Explicitly restart only the existing
   `memory-layer.service`, then its watcher if it was previously enabled. Require
   the running process to resolve to `/usr/bin/memory`; require `/healthz` to
   report version `2.1.0-rc.1`, database `up`, the expected stable `service_id`,
   and successful authenticated read/write and Web/TUI checks. The health
   `instance_id` is a process-start UUID created with `Uuid::new_v4()` and is
   expected to change on every restart; it is not an installation identity and
   must not block acceptance. Verify the stable `service_id` against the
   effective `cluster.service_id` and across a controlled restart instead.
9. **Return dependent services.** Restart only Step CA or OpenBao if the topology
   check proved they were PostgreSQL clients and they were stopped for the
   maintenance window. Verify their native health checks and that no key,
   certificate, seal, storage or configuration identity changed.
10. **Remove PostgreSQL 15 after acceptance.** Only after PostgreSQL 17 has
    accepted the verified application write and all PostgreSQL, Memory and
    dependent-service checks pass, record final old-cluster identity and then
    delete `15/main` with `pg_dropcluster --stop 15 main`. Simulate package
    removal and purge only PostgreSQL-15 major-version packages when the
    transaction would not remove PostgreSQL 17, shared client/common packages or
    another service dependency. Do not use autoremove. Retain the encrypted
    backup, restore proof, inventories, upgrade receipts and prior Memory
    artifact. Do not enable Trust, split the dataset or change federation/Pi.

## Automatic rollback boundary

Before the first accepted write on PostgreSQL 17, a failed validation may return
clients to the retained PostgreSQL-15 cluster using the locally recorded port
and configuration reversal. After the verified PostgreSQL-17 write and complete
acceptance checks, PostgreSQL 15 is no longer the rollback path and may be
deleted as authorized above. Do not delete it earlier.

If the Memory package fails while the database upgrade is healthy, restore the
previous authenticated Memory package/binary and restart only Memory. Do not
roll the database back merely because the application binary failed.

## Report once

Post one sanitized Notion result with the complete local evidence retained on
the CA:

```text
message_id: ca-monolith-<UTC-date>-fast-upgrade-result
recorded_at: <UTC timestamp>
actor: ca-agent
target: monolith-agent
kind: RESULT | BLOCKED
reply_to: monolith-ca-20260924-011
operation_id: <UUID retained across internal retries>
status: UPGRADED | ROLLED_BACK_SAFE | STOPPED_SAFE
database_binding: monolith | local | other
release:
  manifest_sha256: <hash>
  arm64_deb_sha256: <hash>
  installed_memory_version: <version>
postgresql:
  before: 15/main port 5432
  after: 17/main port 5432 online
  rollback: 15/main retained during validation, then deleted after acceptance | retained after rollback
  pg_upgrade_check_sha256: <hash>
  pg_upgrade_receipt_sha256: <hash>
  inventory_match: yes/no
backup:
  encrypted_backup_sha256: <hash>
  isolated_restore_receipt_sha256: <hash>
memory:
  health_version: <version>
  database: up/down
  service_id_match: yes/no
  process_instance_id: changed_as_expected | unchanged_unexpected | unavailable
  authenticated_read_write: pass/fail
  web_tui_smoke: pass/fail
dependent_services:
  step_ca: unchanged | restarted_and_healthy | not_a_pg_client
  openbao: unchanged | restarted_and_healthy | not_a_pg_client
rollback_actions: none | <sanitized summary>
deviations: none | <sanitized summary>
private_evidence_root: <CA-local path only>
next_action: retain encrypted backup and evidence; Monolith continues without Olivier relaying output
```

Success means the existing Memory service runs `2.1.0-rc.1`, PostgreSQL
`17/main` has replaced `15/main` on port 5432, the old cluster remains stopped
until acceptance and is then deleted, and all previously healthy dependent
services are healthy. It does not mean the four-area split, Trust import,
federation or Pi activation is done.
