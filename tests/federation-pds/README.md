# Private PDS prerequisite probe

Run `node tests/federation-pds/run.mjs` from the repository root. Requires Docker
Compose, Node and OpenSSL. Pull the pinned image before running on an air-gapped
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

An exit code of zero means only that the implemented probe assertions passed.
It does **not** establish the full production gate: disaster backup/restore,
gateway access controls, two Memory nodes, and all production release gates must
also pass. No production integration is enabled by this fixture. CAR migration
is evidence for repository recovery, not restoration of every account database,
credential, outbox or cursor. In-place signing-key rotation is not claimed;
the tested operational route uses a replacement PDS and controlled DID update.
