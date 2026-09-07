# Private PDS prerequisite probe

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
