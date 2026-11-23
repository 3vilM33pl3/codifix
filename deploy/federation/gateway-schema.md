# Dedicated gateway schema preparation

`codifix-gateway-schema` is a one-shot schema-owner operation, built from
`codifix-area-ops`. It reuses the vetted fresh-schema migrator used by area migration;
it is not an area import, the Codifix app, the gateway service or a test fixture.
The federation runtime does not depend on area migration tooling.

It supports only a **new, empty, dedicated local PostgreSQL database**. It creates
no database, login, credential, grant, gateway identity, publication, membership,
job or listener. It does not upgrade an existing database or repair lost history.
Do not use `isolation-admin` or start the Codifix service to initialize a gateway.

## Review first

```sh
cargo build --release -p codifix-area-ops --bin codifix-gateway-schema --locked
codifix-gateway-schema /private/company/gateway-schema.json
```

The single-path default is equivalent to `review /private/company/gateway-schema.json`.
It opens no database connection. Supply an operator-owned mode-0600 regular file
in an owned 0700 directory, with trusted, non-symlink ancestry. The JSON is limited
to 8 KiB; duplicate and unknown fields are rejected. For example:

```json
{
  "format_version": 1,
  "space_id": "8593c0d4-2101-4392-8d35-e088fcb43fbc",
  "connection": {
    "kind": "unix",
    "directory": "/private/company/postgres-socket",
    "port": 5432
  },
  "database": "company_gateway",
  "owner": "gateway_schema_owner",
  "password": null
}
```

These are illustrative identifiers, not a provisioned space or credential.
`space_id` explicitly binds the intended future gateway space in the review; this
command does not enroll or persist that space. The connection may instead be
`{"kind":"loopback","port":5432}`, meaning exactly `127.0.0.1`. Arbitrary remote
URLs/hostnames, PostgreSQL environment overrides and `.pgpass` discovery are not
supported. Connections are local Unix sockets or loopback without TLS. Use only
a reviewed local namespace/socket binding; this is not a remote TLS client or
proof of physical cluster identity.

`password` may be a value only in this private file, or null for separately
configured local authentication. The tool does not create or change PostgreSQL
authentication policy. Never put owner credentials in command-line arguments,
shell history or gateway/participant mounts. For a container database, an
explicitly provisioned owned socket directory can be bound to the maintenance
operator without publishing a TCP port. Review UID/socket permissions and exact
database ownership separately; do not broaden permissions merely to make it run.

Review output includes the database, owner, local connection, space UUID,
`target_sha256`, exact `schema.schema_sha256`, compiled migration head and
`review_sha256`. It excludes password bytes and the input-file path. The digest
binds the complete private target bytes (including its credential and whitespace)
and exact executable migration identity. Any change requires a new preview.

## Confirmed fresh initialization

First independently verify the target's identity, empty state, backups and writer
fence. Keep all application/gateway clients stopped and prevent new connections.
The command does not terminate clients or establish a durable cluster-wide fence.

```sh
codifix-gateway-schema execute /private/company/gateway-schema.json EXACT_REVIEW_SHA256 \
  --writers-stopped --fresh-database
```

The flags are explicit operator attestations, not permission to clear an existing
database. Execution checks the actual connected database, login and database
owner against the reviewed descriptor. It refuses other connected clients or any
pre-existing user tables, functions, types, schemas, public namespace objects
(including collations/operators/text-search objects), extensions (except default
`plpgsql`), event triggers, foreign wrappers/servers, default ACLs, publication/
subscription state or large objects.

All 60 compiled repository migrations, their SQLx history and final empty-state
checks run in one outer transaction under the shared fresh-schema advisory lock.
The existing compatibility rule omits migration 0004's invalid HNSW index on the
dimensionless vector column; 0016 removes that obsolete index. The original
historic SQLx checksums are preserved, and the review separately binds the actual
executed SQL. No migration file or historic row is rewritten or fabricated.
The owner needs the privileges required by those migrations, including pgvector
extension installation; runtime credentials must never inherit those privileges.

Success means only `gateway_schema_initialized_only_not_enrolled_or_activated`.
Migration-defined inert seed rows are validated exactly; application projects,
principals, jobs, gateway state and publications remain empty. A new migration
head requires an explicit operator code/validation update, not silent expansion.

If execution fails or its acknowledgement is lost, stop and inspect the exact
target's schema/history. A committed target cannot be initialized again. Do not
drop it, delete migration history or assume a database effect was rolled back
because the client reported an error. A late migration failure is covered by the
shared migrator's full-transaction rollback test.

## Separate runtime grants and activation

After verified initialization, separately create a non-owner gateway login and
review `gateway-grants.sql`. Replace **both** occurrences of its example database
identifier `memory_isolation` with the exact reviewed gateway database; if the
runtime role is not `gateway_runtime`, replace each role occurrence deliberately.
Run the reviewed statements as the owner against that exact database. This
operator neither substitutes nor executes the grant template.

Grant only the listed federation tables and permitted mailbox cleanup operations.
Do not grant application-table access, schema ownership, TRUNCATE, role membership
or migration authority. Verify runtime startup's privilege checks separately.
PDS provisioning, gateway receipt keys, fresh gateway state, membership enrollment
and collection consent remain explicit later gates. Keep the schema-owner file
out of the running gateway and participant containers.

## Verification

```sh
CODIFIX_TEST_REQUIRE_DB=1 cargo test -p codifix-area-ops --all-targets --locked -- --test-threads=1
```

Set `CODIFIX_TEST_DATABASE_URL` to an authorized disposable PostgreSQL server
with pgvector and test database/role creation privileges. Tests create uniquely
named temporary databases and roles. Coverage includes offline/redacted preview,
exact digest confirmation, private file/environment rejection, actual owner
binding, no bootstrap, preserved migration checksums, competing clients, nonempty
targets including standalone namespace/replication objects, replay refusal,
existing area behavior and late-transaction rollback.
