# Run against an operator-prepared database

By default, Memory applies pending migrations on startup (`migrations = "apply"`).
For installations with a separate schema owner, prepare migrations before starting
the application and configure its own non-owner runtime login:

```toml
[database]
url = "postgresql://RUNTIME_LOGIN:SECRET@PRIVATE_DATABASE/memory"
migrations = "verify_only"
```

Keep the real URL in protected configuration, never shell arguments or logs.
`verify_only` reads `public._sqlx_migrations` and requires every compiled migration
version, original checksum and successful status to match exactly. Missing,
changed, unsuccessful or extra migration history blocks startup, including in
the development profile. An unavailable prepared database also blocks startup:
this explicit mode does not fall back to an offline or relay service. It does not
create the migration table or run DDL.

The runtime login needs normal application table/sequence permissions but does
not need `CREATE` on the schema, ownership of tables, or writes to
`_sqlx_migrations`. Application bootstrap still records the local owner and loop
definitions; this is not a read-only evaluation mode. An evaluation profile
continues to use its existing separate sealed-dataset path.

Stop all writers and make a verified backup before an owner-run upgrade. Never
edit migration history to force a match or grant the application owner privileges
to bypass an error. This check verifies recorded migration history, not physical
schema drift, database identity, backup recovery, permissions or writer fencing.
