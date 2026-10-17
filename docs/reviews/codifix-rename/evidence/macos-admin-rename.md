# macOS onboarding administrator rename rehearsal

The supported temporary-session procedure removes the requirement for a second
persistent administrator. The original bootstrap administrator can create an
uncommitted `NOLOGIN SUPERUSER`, switch session authorization to it, rename the
original administrator, reset session authorization to the original OID under
its new name, assert that identity, drop the temporary role and commit. No
password reset, catalog edit, authentication weakening or persistent extra
principal is needed.

This is preparation evidence for the
[release-specific reinstall procedure](../../../../packaging/CODIFIX-3-REINSTALL.md#existing-native-onboarding-databases-and-roles).
The approved live-cutover gates still apply. No live Mac, service installation or
existing shared test database was modified. No Cargo build ran.

## Environment and identity

The rehearsal used a new disposable PostgreSQL **17.11** cluster on Linux from
the already available pinned pgvector image. Its actual bootstrap role was
`memory_onboarding_admin`, OID **10**, with a newly generated synthetic SCRAM
credential. Both host and local authentication used SCRAM. Connections used the
explicit disposable loopback endpoint; the environment did not expose a real
Mac or production database connection.

The fixtures included an administrator-owned database/table and explicit grants
to a non-superuser area role. The additional documented-transaction rehearsal
included all three old area database/login pairs, each with a synthetic table.
Equality comparisons used database, owner, role, table, grantor and grantee
**OIDs**, rather than ACLs rendered with role names. The latter correctly change
their display names after a rename.

Credentials and verifier values were compared privately in the process and
never written to this evidence. Recorded results contain only check names,
booleans, server version and the bootstrap OID; there are no connection URLs,
passwords, verifier values or credential-file links here.

## Initial 22 checks

All 22 passed:

| Check | Result |
| --- | --- |
| Exact server major is PostgreSQL 17 | Pass |
| Host authentication uses SCRAM | Pass |
| Original administrator is bootstrap OID 10 | Pass |
| Original administrator has a SCRAM verifier | Pass |
| Direct current-session-user rename is rejected | Pass |
| `SET ROLE` alone is insufficient; failure rolls back | Pass |
| A colliding temporary name is retained unchanged | Pass |
| A colliding target name is retained; temporary creation rolls back | Pass |
| Abort after rename restores the original role OID and exact verifier | Pass |
| Abort preserves the owned database/table/data | Pass |
| Original password still authenticates after abort | Pass |
| MD5 precondition refuses the rename | Pass |
| `RESET SESSION AUTHORIZATION` restores the original authenticated OID with its new name | Pass |
| Administrator OID, login/superuser flags and exact SCRAM verifier survive commit | Pass |
| Temporary principal is absent after commit | Pass |
| Owned database/table OIDs, ACL identities and data survive commit | Pass |
| Original password authenticates the renamed administrator | Pass |
| An incorrect password is rejected for the renamed administrator | Pass |
| The old login name no longer authenticates | Pass |
| Area role's original password and explicit data grant still work | Pass |
| Retry after commit refuses the missing old role and leaves no temporary role | Pass |
| Only the expected user-defined roles remain | Pass |

Direct current-session rename and the `SET ROLE` attempt returned SQLSTATE
`0A000`. Name collisions returned `42710`; an injected arithmetic failure after
the rename returned `22012`. Each failed connection closed with the transaction
uncommitted, leaving the original role and credential usable. Authentication
failure was checked through the client's connection-failure result, not through
SQLSTATE output unavailable before a session starts.

## Exact documented transaction: 11 additional checks

The SQL block was extracted directly from the reinstall guide at source base
`967c52e8`, including its current transaction preconditions and final OID/name
assertion. All 11 additional checks passed:

| Check | Result |
| --- | --- |
| Deliberately mismatched saved OID fails the final assertion and rolls back all area/admin renames | Pass |
| Documented SCRAM guard rejects an MD5 verifier and restores the original credential on rollback | Pass |
| Documented temporary-name guard retains an existing colliding role unchanged | Pass |
| Documented target-name guard retains an existing colliding role unchanged | Pass |
| Exact documented transaction preserves all three area database/role/table OIDs, ownership, grants and data | Pass |
| Exact transaction preserves bootstrap OID/verifier and removes the temporary role | Pass |
| Original administrator password authenticates after documented commit | Pass |
| Original homelab role password authenticates after documented commit | Pass |
| Original general role password authenticates after documented commit | Pass |
| Documented retry refuses the renamed state without residue | Pass |
| No temporary role or additional user-defined principal survives | Pass |

The final assertion checks both `session_user` and `current_user` against
`codifix_onboarding_admin` and compares the restored role OID with the
transaction-local OID captured before the rename. It executes before temporary
role removal and commit. The temporary role receives no objects or grants; its
successful `DROP ROLE` is also a dependency check. Failed assertions or
collisions must roll back, never commit an intermediate state. After an uncertain
commit acknowledgement, inspect names/OIDs before deciding whether to retry.

## PostgreSQL constraints and remaining gates

[SET SESSION AUTHORIZATION](https://www.postgresql.org/docs/17/sql-set-session-authorization.html)
requires the authenticated user to be a superuser when changing to another
identity; it is supported inside a PostgreSQL transaction. `RESET` restores the
original authenticated identity. [ALTER ROLE](https://www.postgresql.org/docs/17/sql-alterrole.html)
forbids renaming the current session user and clears an MD5 verifier on rename.
The documented procedure therefore refuses MD5/missing verifiers and temporarily
changes the session identity. [ALTER DATABASE](https://www.postgresql.org/docs/17/sql-alterdatabase.html)
requires connection to a different database for a rename; the procedure uses
`postgres` and keeps other clients quiescent.

The result establishes PostgreSQL transaction/authentication behavior for the
synthetic fixtures. It does not establish live Mac cluster identity, native
service/Keychain access, populated configuration/journal transfer, signing or
package acceptance. Private pgpass/config/profile references must be reconciled
with the new names while preserving password bytes. Pin-bound old onboarding
journals remain evidence and cannot be blindly edited or replayed as an upgrade.
