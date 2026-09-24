# CA fast replacement: Memory 2.1.0-rc.1 and PostgreSQL 17

Authorized direction, 24 September 2026: replace the existing PostgreSQL 15
cluster with PostgreSQL 17 and upgrade the existing Memory singleton to the
latest signed deployable release. Do not create a parallel `17/trust` cluster.

This is one autonomous maintenance operation. The CA agent performs internal
checks and reports once. It retains PostgreSQL 15 and the previous Memory binary
as rollback material; operational replacement does not mean immediate deletion.

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
| PostgreSQL rollback cluster | Existing `15/main`, retained stopped/manual on the port assigned by `pg_upgradecluster` |
| Memory package | Signed `memory-layer_2.1.0-rc.1_arm64.deb` |
| Memory package SHA-256 | `34894b000cd1d9e71890d1415e3ab7ac5e6c47f06f7494b48b9e1283a17ab71a` |
| Memory source revision | `b35faf2983d20c69bc086fb9bcb7e95c51bd8f47` |
| Memory schema compatibility | floor 60, head 60, no migration |
| Release manifest SHA-256 | `5d06127ec4db38e0d56eb0942a41f26bee32987645f1bf81391fb929e19ee1d4` |
| Release signing subkey | `4EB55288032C60EF65C73A0352EE81AD6D98F325` |

Use the repository and package identities in
[the PGDG binding](PGDG-BOOKWORM-ARM64-BINDING.md). Install the bound
PostgreSQL-17 server/client and any exact PostgreSQL-17 equivalents required by
extensions actually present in `15/main`. Do not install development packages.

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
8. **Upgrade the existing Memory service.** Confirm the configured shared/local
   database is reachable and at schema 60. Explicitly restart only the existing
   `memory-layer.service`, then its watcher if it was previously enabled. Require
   `/healthz` to report version `2.1.0-rc.1`, database `up`, the same expected
   instance identity, and successful authenticated read/write and Web/TUI checks.
9. **Return dependent services.** Restart only Step CA or OpenBao if the topology
   check proved they were PostgreSQL clients and they were stopped for the
   maintenance window. Verify their native health checks and that no key,
   certificate, seal, storage or configuration identity changed.
10. **Finish without deleting rollback.** Leave PostgreSQL 15 and the old Memory
    artifact retained but inactive. Do not `pg_dropcluster`, purge packages,
    enable Trust, split the dataset, or change federation/Pi in this operation.

## Automatic rollback boundary

Before the first accepted write on PostgreSQL 17, a failed validation may return
clients to the retained PostgreSQL-15 cluster using the locally recorded port
and configuration reversal. After any accepted PostgreSQL-17 write, do not
blindly start PostgreSQL 15: preserve both sides and reconcile the write first.

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
reply_to: monolith-ca-20260924-008
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
  rollback: 15/main retained stopped/manual on <port>
  pg_upgrade_check_sha256: <hash>
  pg_upgrade_receipt_sha256: <hash>
  inventory_match: yes/no
backup:
  encrypted_backup_sha256: <hash>
  isolated_restore_receipt_sha256: <hash>
memory:
  health_version: <version>
  database: up/down
  instance_identity_match: yes/no
  authenticated_read_write: pass/fail
  web_tui_smoke: pass/fail
dependent_services:
  step_ca: unchanged | restarted_and_healthy | not_a_pg_client
  openbao: unchanged | restarted_and_healthy | not_a_pg_client
rollback_actions: none | <sanitized summary>
deviations: none | <sanitized summary>
private_evidence_root: <CA-local path only>
next_action: retain rollback material; Monolith continues without Olivier relaying output
```

Success means the existing Memory service runs `2.1.0-rc.1`, PostgreSQL
`17/main` has replaced `15/main` on port 5432, the old cluster remains stopped
for rollback, and all previously healthy dependent services are healthy. It does
not mean the four-area split, Trust import, federation or Pi activation is done.
