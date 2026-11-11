# Upgrading a Network/Inbox backend

Use the artifact for the actual operating system and architecture. Verify its
source revision, manifest and checksums. Keep machine names, addresses, concrete
deployment paths, installation receipts and operator-specific commands in
private operational documentation outside the public repository.

1. Inventory the selected backend's executable/image, launch definition,
   configuration and database identity. Installing a package does not switch a
   service pinned to a different release or container image.
2. Rehearse against an isolated restored database. Stop all writers sharing the
   selected database before cutover, and preserve verified backups, configuration
   and the old release. Do not mix old and new writers during migration.
3. Install/stage the verified release without automatically enabling services.
   Where applicable, switch the exact release reference, matching web assets and
   executable digest through the deployment's established verification process.
   Preserve signature checks; an unsigned candidate is not a signed release.
4. Apply the reviewed migration chain through the established owner procedure.
   Keep a least-privileged runtime's `verify_only` setting intact. The companion
   supports `codifix-agency --config PATH migrate`; its `database_env` names the
   securely supplied connection-string environment variable. Configuration
   `check` does not test database connectivity. Verify runtime access to newly
   created tables and sequences before starting the selected backend.
5. Verify the running version, migration status, health, existing sign-in and
   project reads. Restart only the intended backend after migration succeeds.
   Feature activation and authority grants require their own explicit setup.

Network/Inbox requires its home service and companion to share PostgreSQL and
the configured home instance identity. See [operator setup](network-inbox.md#operator-setup)
and [agency operations](autonomous-specialists.md). Membership or linked AT
identity does not confer action authority.

After migrations or new writes, do not assume a binary-only downgrade is safe.
Use a compatible forward correction or a coordinated restoration of the
verified database backup, configuration and matching prior binaries.

Installation does not authorize public discovery publication, external ticket
writes, key-recovery ceremonies or new standing grants. Consult the candidate's
acceptance evidence for platform validation and unresolved dependency findings.
