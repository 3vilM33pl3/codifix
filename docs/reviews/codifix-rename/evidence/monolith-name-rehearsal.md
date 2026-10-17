# Monolith exact database and role name rehearsal

Prepared evidence; no live cutover occurred. Four fresh isolated PostgreSQL 17.11
clusters used the pinned pgvector image and synthetic credentials. Each area
fixture used its actual bootstrap role at OID 10, including the General and
gateway branded bootstrap names. Guards extracted from the private ledger were
rebound to the fixture catalog OIDs and cluster system identifiers; this does not
claim a live OID match. The original ledger remained byte-identical.

All 106 checks passed across Workshop, Homelab, General and gateway. Exact
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

## Private evidence bindings

| File | SHA-256 |
| --- | --- |
| `rehearse.py` | `1deac1933d52d23a8a9d8dfb90308236ee53b44cf9b3675aab895da3f4f8888d` |
| `report.json` | `56a44fc298b1306ee8743c56159a12cc97257fadae6514ebd7714cfb559a845d` |

The source ledger and synthetic fixture credentials remain in the private local
recovery directory. No credentials or SCRAM verifier bytes are included here.
