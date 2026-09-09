# 2026-09-09: Stage D runtime repository verification

First bounded D2 increment, continued from clean C commit `c718524`. The
[increment contract](d-signature-contract.md) does not close D1–D4 or authorize
customer deployment. The master roadmap remains **11/39** complete.

## Implemented boundary

`mem-atproto-verify` adds exactly pinned Atrium crypto 0.1.3 alongside API 0.25.8
and repository 0.1.8. It is separate from both the neutral domain/store and the
original offline adapter. Dependency guards enforce those boundaries and prohibit
network/database/service dependencies in the verifier.

The gateway requires an administrator-enrolled repository DID and canonical
compressed public `did:key`. No resolver, record-derived URL, TOFU, unsigned
fallback, key rotation or production credential mutation is introduced. Service
configuration now requires `signing_key`; enrollment failure occurs before the
gateway listener or writer fence is initialized.

PDS success requires exact readback and a complete, bounded signed CAR snapshot.
The implementation checks framing before allocation, SHA-256 block hashes,
canonical v3 commit fields, DID binding, low-S K256/P256 signature, future-clock
sanity, complete MST and exact record inclusion. Untrusted nodes are parsed with
bounds and reconstructed into a fresh trusted Atrium tree; root equality checks
canonical ordering, prefix compression, topology and layer placement. Arbitrary
CAR block order and duplicate identical blocks are supported.

Limits: 16 MiB CAR, 32,768 physical blocks, 1 MiB per framed block, 4 KiB header
and commit, 8,192 records/nodes, 64 tree edges. This is O(repository) verification
per publication, not a production-scale incremental proof implementation. One
download/verification slot per publisher bounds work across clones; cancellation
retains the slot while the blocking verifier finishes. Invalid proofs never
acknowledge the durable outbox and emit only a sanitized failure category.

## Verification evidence

The actual pinned PDS path passes (9.04 seconds for the Rust three-database
workflow), including lost successful create reply, deterministic retry/readback,
catch-up/resync, reviewed evidence, tombstones, current permissions and revocation.
The upstream JavaScript SDK independently validates the resulting signed CAR and
Lexicon records and rejects a wrong key. Test PDS volumes/network/secrets were
removed; the harness reports that raw PDS reads remain public *inside* its private
network, reaffirming the unimplemented D1 deployment gate.

Final local validation:

| Check | Result |
| --- | --- |
| `cargo test -p mem-federation -p mem-atproto -p mem-atproto-verify -p mem-federation-runtime --all-targets --locked` | 83 passed, zero failed; real database required |
| `cargo test --workspace --all-targets --locked -- --test-threads=1` | 704 passed, zero failed/ignored; isolated XDG configuration and disposable PostgreSQL 17, database required |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --check`, `git diff --check`, `scripts/check-boundaries.sh` | Passed |
| `lake build && lake test` | Passed all 14 shared traces; existing proof linter warnings, no model changes or cryptography proof claim |
| `node tests/federation-pds/backup.test.cjs` | 3 passed |
| `node tests/federation-pds/run.mjs` | Provisioning, signed migration, encrypted cold recovery after source-volume removal and restored signed publication passed |
| `node tests/federation-pds/run.mjs --runtime` | Signed Rust gateway/PDS workflow and independent upstream verification passed |

The verifier has nine tests, including property coverage for arbitrary framing,
block ordering/duplicates, bounded signed resource attacks, both signing curves,
unknown-field preservation, malformed/noncanonical proofs and an upstream golden.
Four runtime HTTP tests include plausible exact JSON with invalid/absent/oversized
repository proofs; acceptance tests check forged publisher and cross-space writes
do not alter the accepted log. Service tests require the signing-key setting and
reject malformed enrollment before network startup.

CI path filters now include the verification crate in database and pinned-PDS
jobs; the standard workspace Rust job covers its tests/Clippy. These are locally
verified commands and committed CI changes, **not** a claim of a remote CI run.

### Interoperability finding and regression

The first real-PDS attempts correctly blocked at CAR validation. Inspection of
the pinned serializer showed that `serde_ipld_dagcbor 0.6.0` sorts map entries but
writes struct fields in declaration order. Our initial metadata declarations and
Atrium 0.1.8's node/entry declarations therefore produced deterministic but
noncanonical bytes. The older spike's insertion-order checks did not establish
byte compatibility with an independently implemented repository.

The security adapter now declares metadata fields in canonical encoded-key order
and wraps Atrium's **locally generated** MST node writes in `CanonicalMstStore`.
It never rewrites incoming proof bytes to accept them. This requires no PDS fork,
dependency patch, crypto change, relaxed incoming validation or core change.

`upstream_golden_proves_canonical_mst_not_just_local_determinism` is a minimal
reproduction: three keys with the same synthetic `{ "n": 1 }` record produce a
different root with raw pinned Atrium, but the canonical adapter matches the
independently generated upstream root. The golden fixture records its generator
and image under `fixtures/federation/atproto/verification/mst-canonical.json`.
Both K256/P256 signature fixtures and the real PDS pipeline pass after the fix.
This is evidence for the bounded adapter, not blanket Atrium or native Spaces
compatibility. Future dependency upgrades must rerun these independent tests.

## Intentionally open

- D1: deploy/review gateway-only PDS access, private database/network/TLS boundary
  and negative CAR/firehose/raw repository/cross-space access tests.
- Remaining D2: trusted identity enrollment operations, safe discovery if needed,
  key lifecycle/rotation and receiver proof validation. This constructor assumes
  a trusted DID/key binding; it does not establish ownership itself.
- D3: imported-content/model-forwarding policy and all retrieval enforcement.
- D4: durable independent audit receipts, retention/deletion/legal holds and
  complete offline-copy/revocation user policy.

Inclusion in a signed snapshot is not factual correctness, current membership,
freshness, append-only history or proof of human approval. The future-revision
check is not rollback protection. Gateway acceptance order remains transactional;
repository keys/revisions do not replace it. Receivers still trust their pinned
gateway transport; proof metadata is not yet persisted or carried to them.
Existing `Published` rows are not retroactively certified by this change.

No SQL migration, native Spaces substitution, PDS fork, security bypass, deployed
customer workload, production Lexicon, Notion edit or startup-document change.
Neuralese and Aeneas remain deferred.
