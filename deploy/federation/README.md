# Private gateway deployment boundary

This is a **tested reference topology and dedicated gateway package**, not an
installed customer deployment. First complete the [release gates](../../docs/reviews/federation-first/README.md).
No public ports, native Spaces, PDS fork, public Bluesky or automatic discovery
are introduced. Container/host administrators remain trusted plaintext operators.

For the separate production-intended Workshop/Homelab blueprint, see
[company topology](company-topology.md). Its default-deny generator does not
install services, reuse fixture identities or grant collection consent; it needs
its own disposable runtime rehearsal before installation.

`topology.mjs` constructs the Compose graph used by the
[executable isolation test](../../tests/federation-isolation/README.md):

- Each space has a dedicated gateway database, PDS volume, audit key, repository
  binding and instance-token allowlist. Only its gateway joins all three private
  API/repository/database networks.
- A fixed-upstream TLS edge exposes only `/v1/accept`, `/v1/pull`, `/v1/retention`
  and, for explicitly enabled private expertise, `/v1/expertise`.
  Its API network does not include the PDS or database. There is no arbitrary
  forwarding, CAR endpoint or event stream on the edge.
  F1 requires the gateway's complete `expertise_fleet` schema inventory and
  independent local/membership/collection grants; routing this path alone grants
  no authority. The isolation fixture tests authenticated F1 discovery, wrong
  tokens and cross-space denial without enabling cleartext fixture mode.
- The PDS is on a separate internal network behind a fixed-upstream TLS proxy;
  the gateway is its only member-facing client. No participant receives its token,
  key, raw volume, repository interface or Docker socket.
- Each participant has its own local database network and local credentials.
  The member test process is replaced by an independently configured Memory
  instance for installation; never attach its general HTTP/MCP service to gateway
  private networks. The test uses two space authorities to check cross-space denial.
- No service publishes a host port. An approved installation connects only the
  TLS edges to a private authenticated ingress/VPN. Do not expose the gateway's
  cleartext backend listener or the PDS directly.

User-defined bridges isolate containers on different networks, not Docker-host
administrators. `internal: true` does not alone isolate containers sharing a
network; the separation above is essential. See [Docker bridge semantics](https://docs.docker.com/engine/network/drivers/bridge/)
and [Compose internal networks](https://docs.docker.com/reference/compose-file/networks/#internal).
The fixture validates IPv4 and disables IPv6. Re-run negative tests for every
enabled IP family, orchestrator ingress, host firewall and production topology.

## Packaging and installation contract

Build `cargo build --release -p mem-federation-runtime --bin memory-federation-gateway --locked`.
`Dockerfile` packages only this binary, with explicitly supplied, reviewed,
digest-pinned compatible Rust/OS images. It does not build or mount the Memory
app, watchers, model providers or private captures. The fixture builds a separate
debug image from local binary/libraries; that image is not a release artifact.
Review/scanning of release images, certificate renewal, host hardening and
operator onboarding remain pilot-readiness tasks.

Provision a **new dedicated** PostgreSQL database as a separate schema owner and
apply the repository migrations, including 0036, using the normal migration
workflow. Existing published history without receipts deliberately fails
enrollment; it requires a separately approved revalidation/migration exercise.
The test-only `isolation-admin` example uses `mem-test-support` to handle the
known legacy dimensionless-vector migration; do not install this privileged
fixture utility with the gateway.

Create a distinct non-owner `gateway_runtime` login and adapt the database name
in `gateway-grants.sql`. Grant only the listed federation tables, never application
tables, role membership, schema ownership, TRUNCATE or migration rights. After
migrations 0048–0050, reapply the reviewed grants: private Q&A storage is needed
for recovery fencing even while F1 routes remain disabled. DELETE is limited to
the expiring mailbox, delivery challenges and metadata journal; publication/audit history cannot
be deleted. Collection snapshots, consent and service grants remain origin-only.
The executable rejects superuser/create-role/create-database/replication/bypass-RLS
roles, database ownership and read access to local memories/principals. The source
and receiver use their own ordinary Memory databases and local authorization.

Mount each space's configuration and secrets read-only under its own unprivileged
UID. Tokens/database URLs and 64-hex-character audit private scalars must be private
0600 files. PDS repository keys and gateway audit keys must differ. Configure trusted
public receipt keys separately at every receiver; never obtain them from a network
response. The gateway creates no roles, schema, local grants, identities or user
memberships automatically.

Use HTTPS to both gateway and PDS. Optional `gateway_ca_file` / `pds_ca_file` PEM
roots replace built-in roots for that connection while retaining hostname and
certificate validation. Never use invalid-certificate acceptance, redirects,
environment proxies or record-derived URLs. Explicit endpoint allowlisting and
network restriction follow the [OWASP SSRF guidance](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html).

See [runtime configuration](../../docs/user/federation-runtime.md) and
[security/recovery semantics](../../docs/developer/architecture/federation-security.md).
