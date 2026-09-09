# Private PDS prerequisite probe

## Stage E application and client gate

`--runtime` now also runs the three-database Memory HTTP workflow against the
same pinned PDS. Set `MEMORY_FEDERATION_E_CLIENTS=1` to additionally launch two
separate Memory service processes, real browser sessions, CLI commands and a
private-socket tmux PTY. Prerequisites: a fresh `cargo build -p mem-cli --locked`,
`npm --prefix web ci`, `npm --prefix web run build`, Playwright Chromium installed
from `web/`, Python 3 and tmux. The CLI and service fixtures use private temporary
TOML configuration, not developer/customer profiles. The test issues synthetic
sessions/tokens; OIDC login itself is not exercised.

```bash
# Supply only an explicitly disposable pgvector database with CREATE DATABASE
# and CREATE ROLE rights. Never point this gate at a customer/dev Memory database.
MEMORY_FEDERATION_E_CLIENTS=1 node tests/federation-pds/run.mjs --runtime
```

The application database roles are distinct non-superusers and cannot connect
to the other instance's database. Schema creation in each own database supports
the service's normal migration startup; the separately run isolation gate tests
the gateway's narrower six-table production role. Application sessions, source
content and PDS credentials are synthetic. No public identity or customer record
is published. All test-owned services, temporary configurations and PTY sockets
are cleaned up. Databases left by an assertion failure remain only in the caller's
explicitly disposable PostgreSQL fixture; remove that fixture after diagnosis.

`MEMORY_FEDERATION_E_SCREENSHOTS` optionally retains only synthetic desktop/mobile
screenshots in a chosen local directory. `MEMORY_FEDERATION_E_AGENT_BROWSER=1`
adds an independent agent-browser initial-page check when that tool is available.
CI runs the real-PDS browser/CLI/PTY gate on relevant Rust, CLI and Web changes.

Run `node tests/federation-pds/run.mjs` from the repository root. Requires Linux,
Docker Compose, Node and OpenSSL. Run the encryption-envelope unit tests with
`node tests/federation-pds/backup.test.cjs`. Pull the pinned image before running on an air-gapped
host. This is a disposable protocol test, **not a production deployment**.

The runner creates a uniquely named internal Docker network, isolated PDS volumes,
and one-day TLS certificate and ephemeral secrets. No host ports are exposed and
no existing Memory or homelab container is changed. It removes its own containers,
volumes, network and temporary credentials after the run, including handled
interrupts/timeouts. The cached image remains. If Docker cleanup fails, the
temporary directory is retained for diagnosis; remove it after cleaning up that
exact reported Compose project. No existing user volume is a cleanup target.

Pinned upstream image:
`ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12`.
Inspected metadata: distro 0.4.5027, embedded `@atproto/pds` 0.5.27,
source revision `374cf1d4ba782d4391bbb73e4e2d3f320d4846d6`, created 2026-08-11.

The probe uses a reserved example.com hostname, private TLS CA and `did:web`
bootstrap service JWT. It does not register anything in PLC or public DNS.
Development mode and the SSRF-disable flag remain false. Unknown spike records
use the explicit upstream `validate:false` mode; schema validation must be supplied
by Memory, not confused with upstream Lexicon registration.

Current evidence: validated internal-only Compose configuration and production
security flags, production-mode readiness, private BYOD provisioning, signing
key handoff, activation, all six spike schema definitions, actual Rust adapter
request-schema validation (including malformed-record rejection), record readback,
signed CAR verification, and wrong-key rejection. A second PDS imports the CAR
through the supported account-migration path, rotates to a distinct signing key,
and retains the original record. The independent verifier is the upstream SDK
bundled in the image. These assertions also run in the scoped CI prerequisite job.

## Cold disaster recovery evidence

The expanded gate passed locally on 2026-09-07. After the migration checks it
stops both PDS writers, encrypts the complete recovery PDS volume together with
its synthetic account state and runtime credentials, removes the source volume
and unencrypted seed state, then restores into a fresh volume. AES-256-GCM
authenticates the complete bounded fixture archive before any extraction;
corruption and a wrong key must fail with an untouched restore volume.

The new PDS starts with credentials recovered from that archive. The probe
checks password login, pre-backup access and refresh credentials, signing key,
exact commit, records and blob bytes, then creates and independently verifies a
new record. It also tests restored administrator access. The original PDSs do
not run during this verification. All services remain on an internal network.

The archive key is separate from the encrypted archive and is mounted only into
the maintenance helpers. Generated TLS material is kept separately. This is a
bounded synthetic cold-backup experiment (64 MiB limit), **not** an offsite backup
service, production key-custody design, online/PITR backup, or arbitrary archive
importer. Secrets stay in temporary mode-0600 files and are never printed.

An exit code of zero means only that these PDS prerequisite assertions passed.
It does **not** establish the full product gate: actual gateway access controls,
two Memory nodes, transactional outbox/inbox/cursor recovery, writer fencing and
all production release gates remain required. No production integration is
enabled by this fixture. In-place signing-key rotation is not claimed; the
tested rotation route uses a replacement PDS and controlled DID update.

## Stage C: real Rust publication and synchronization

The additional runtime mode uses the same pinned PDS, internal-only network,
private DID and production security flags. It provisions/migrates a synthetic
account, keeps the DID fixture alive, and runs the real Rust gateway/PDS/receiver
workflow with three newly created independent databases. It needs an explicitly
disposable pgvector/PostgreSQL 16 or 17 URL whose test role can create/drop databases:

```bash
cargo test -p mem-federation-runtime --test c_workflow --locked --no-run
MEMORY_LAYER_TEST_DATABASE_URL=postgres://TEST_USER:TEST_PASSWORD@127.0.0.1:TEST_PORT/TEST_DB \
  node tests/federation-pds/run.mjs --runtime
```

Do not supply a development or customer database. Rust creates UUID-named source,
gateway and receiver databases, and drops only those exact databases after success.
Compose services expose no host ports; on the Linux Docker host, the test reaches
the recovery PDS's internal bridge IP. This runner therefore requires Linux host
bridge access, not Docker Desktop's VM-only container addresses. No broad public
network or PDS security bypass is introduced to accommodate other hosts.

The test exercises actual existing `mem-curate` validation mutation code and the
deferred source hooks, explicit evidence review, acceptance before publication,
lost successful PDS response, source disconnection/restart, duplicated/tampered
delivery, a 32-record page boundary, complete resync, independent state revisions,
tombstones, local permission removal and immediate gateway revocation. The proxy
drops the first successful upstream create reply; the production Rust publisher
must recover using exact URI/CID/value readback. Since the first Stage D increment,
the fixture also supplies its provisioned public signing key to the Rust publisher,
which requires bounded signed full-CAR inclusion before acknowledging any record.
The unsigned fake path has been removed: synthetic XRPC tests generate real
signatures too. The upstream SDK independently
validates all stored v2 Lexicon wrappers and the signed repository, rejecting a
wrong signing key. All data and credentials are ephemeral and private.

Both the original cold-recovery mode and runtime mode pass locally on 2026-09-09.
The scoped CI job runs both, after compiling the Rust integration test. Runtime
mode establishes C's software integration, **not** D/E's complete deployed trust
boundary, receiver proof/audit path, browser workflows or customer release gates.

## Canonical MST regression

Pinned Atrium 0.1.8 plus `serde_ipld_dagcbor 0.6.0` serializes MST structs in
noncanonical declaration order. Local insertion-order determinism alone misses
this. `mem-atproto-verify::CanonicalMstStore` canonicalizes only locally generated
nodes before hashing, while incoming bytes remain subject to strict validation.
The independent synthetic upstream golden in
`fixtures/federation/atproto/verification/mst-canonical.json` records the exact
generator/image and inputs. Reproduce the raw-Atrium mismatch and corrected root:

```bash
cargo test -p mem-atproto-verify upstream_golden_proves_canonical_mst_not_just_local_determinism --locked
```

The full live PDS test validates the integration independently, including canonical
commit signatures. No PDS fork, dependency patch or weakened security flag is used.
