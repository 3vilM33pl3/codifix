# Monolith exact database and role name rehearsal

Prepared evidence; no live cutover occurred. Four fresh isolated PostgreSQL 17.11
clusters used the pinned pgvector image and synthetic credentials. Each area
fixture used its actual bootstrap role at OID 10, including the General and
gateway branded bootstrap names. Guards extracted from the private ledger were
rebound to the fixture catalog OIDs and cluster system identifiers; this does not
claim a live OID match. The original ledger remained byte-identical.

All 114 recorded checks passed across Workshop, Homelab, General and gateway. Exact
primary/restorecheck DB names and General/gateway role names were renamed, with
restorecheck connections remaining disabled. Generic postgres and neutral area/
runtime roles were preserved. The temporary NOLOGIN SUPERUSER/session-authorization
procedure preserved the renamed bootstrap identity and removed its temporary
principal before commit.

Checks compared role/database/table/owner/grantor/grantee OIDs, exact SCRAM verifier
bytes privately, flags and synthetic canonical/version/content values. Original
passwords authenticated renamed/neutral roles; wrong passwords and old renamed
login/database names were refused. Disabled renamed restorecheck DB connections
were refused. Target DB/role collisions and temporary-role collisions were retained
without partial changes; unexpected clients, incorrect fixture OID and MD5 verifier
guards refused; injected post-rename failure rolled back names, identities,
credentials, ownership, grants and content. Retry after commit refused the old
identity without temporary-principal residue.

Only the four newly created disposable PG fixture runtimes were started/stopped.
No application/worker/service, existing database, provider, CA, source checkout or
live credential was changed. No Cargo build ran. The fixture containers are stopped and
retained, along with private scripts, fixture SQL and check-name/boolean results.
The final two authentication checks were supplemented on these same isolated
fixtures after restart; the next-fresh-run script includes those checks inline.

This proves the bounded SQL/authentication procedure for synthetic fixtures, not
the populated cold physical transfer, native service configuration, live clients,
final matched recovery, artifact acceptance or live cutover. Those operational
gates remain open.

## Focused rejection-evidence correction

The original 20 connection-refusal checks accepted any OperationalError, so that
evidence was narrowed. Only those 20 negatives were rerun on the retained four
fixtures. The new matcher requires the specific PostgreSQL SQLSTATE when the
connection exception provides one; otherwise every diagnostic line must match
the exact PostgreSQL 17 fixture FATAL message for password rejection, missing
database, or a database not accepting connections. Unknown diagnostics and
transport errors fail the check; exception text and credential values are never
printed. A successful control connection follows each expected refusal.

Four actual connection-refused probes against locally reserved, non-listening
ports were rejected by every expected-PostgreSQL-error category and by the
asserting matcher. They did not increment the accepted refusal counter. Four
before/after snapshots confirmed that this focused rerun preserved role/database/
table OIDs, exact SCRAM verifier bytes, ownership/grants, flags and content. All
four exact fixtures were stopped afterward. No other original checks were rerun.

The resulting 114 checks comprise 86 unaffected original checks, 20 strictly
revalidated refusal checks, 4 transport-failure rejection proofs and 4 focused
preservation checks. Prior private script/report/draft versions are retained.

Private execution artifact SHA-256 values (metadata only):

- `rehearse.py`: `477255a645532ed8bd637fdbd5aa20cbf013d4e74084aad6f9ffc6d429728301`
- `connect_failure.py`: `67cd715ee98e1d3f91b5c3b8f5e0ad4a32a437fe5db272631515fabe797321da`
- `revalidate_auth.py`: `8456d9b74e27ae7ef5b883a18a0d8a77f28c1569f0b01bea8932d8efcb39f571`
- `report.json`: `66e936834bed8b94783f84adff0c4a7d113290c5e3d4c74b1477e65880513240`
