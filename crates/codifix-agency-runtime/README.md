# Autonomous specialists

`codifix-agency` is an opt-in, separately permissioned companion service. It
uses Codifix's PostgreSQL jobs and effect journal, without turning the existing
proposal-only supervisor into an unrestricted executor. Existing memory and
expertise APIs remain unchanged.

Start with the [operator guide](../../docs/user/autonomous-specialists.md). The
[architecture](../../docs/developer/architecture/agency.md) records boundaries,
protocols, evidence and known limits. This binary is not started by the ordinary
Codifix service or desktop installer.

```sh
cargo build -p codifix-agency-runtime -p codifix-agency-openbao --release --locked
codifix-agency --config /etc/codifix-agency/config.json check
# Explicit operator step, against the intended backed-up database:
codifix-agency --config /etc/codifix-agency/config.json migrate
codifix-agency --config /etc/codifix-agency/config.json serve
```

The example configuration has fictional identities, no credentials, no published
profile, no model endpoints, no Linear authority and no OpenBao enrollment.
Replace reviewed identifiers and endpoints before use; `check` is syntactic
validation, not proof of deployment readiness. Secrets are named by environment
variable or protected-file reference, never embedded in task payloads.

```sh
CODIFIX_TEST_REQUIRE_DB=1 cargo test -p codifix-agency -p codifix-agency-runtime \
  -p codifix-agency-projects -p codifix-agency-openbao --all-targets --locked
cargo test -p codifix-agency-openbao --test ceremony \
  actual_openbao_shamir_two_of_three --locked -- --ignored
```

Set `CODIFIX_TEST_DATABASE_URL` to a disposable pgvector database for the first
command. The second uses a separately installed `bao` binary and only generated
synthetic shares in an isolated local vault. It checks unseal, restart sealed,
and automatic unseal again with unchanged enrollment. Never point these tests at
a live CA or place real unseal shares in environment variables or CLI arguments.
