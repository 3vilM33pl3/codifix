# Federation Protocol Spike

This is the evidence report for the offline, non-production Memory federation
spike. The experiment tested a transport-neutral Rust model, an isolated
ATProto/Atrium adapter, and a handwritten Lean model. It did not add a product
federation path.

## Final Outcome

**Proceed with ATProto as an adapter/profile.**

Keep `mem-federation` as Memory's authoritative semantic and reducer boundary.
Use the six unpublished ATProto Lexicons as one external encoding and repository
profile over that boundary. Do not make ATProto types, repository layout, DIDs,
AT URIs, CIDs, or authorization assumptions part of the neutral core.

The result does not justify designing a bespoke external protocol now. The
neutral core preserves that option if a later private-space, security, or live
network spike finds an ATProto constraint that cannot be handled in the adapter.

## Decision Evidence

| Gate | Evidence | Result |
| --- | --- | --- |
| Required fields round-trip | All common envelope fields and every field of memory-version, canonical-state, evidence, relation, membership, and revocation records round-trip through typed Atrium objects and `Unknown`. Tests compare the original and decoded neutral envelopes using their canonical JSON encodings. | Pass |
| Unknown fields survive | Atrium `Object<T>` wrappers retain an unknown top-level field and an unknown field inside a related-memory object through typed-object/`Unknown` conversion. Extension data stays in the adapter and is not added to the neutral core. | Pass |
| Memory semantics survive | The neutral reducer enforces authority-controlled membership, active publishing membership, revocation, idempotence, equivocation rejection, contiguous versions, dependency quarantine, tombstones, and mutation-free rejection. | Pass |
| AT repository behavior is deterministic | Atrium MSTs built from real DAG-CBOR record CIDs have identical root CIDs and ordered entries under forward and reverse insertion of the vendored example keys. | Pass |
| The core remains transport-neutral | `mem-federation` has no Atrium, async-runtime, database, configuration, or HTTP dependency, and the boundary script rejects those dependencies and Atrium source symbols. | Pass |
| Formal model agrees with executable traces | Lean proves the six reducer properties and its executable checks the same seven JSON files as the Rust conformance test. | Pass |
| Isolated tooling defect | No Atrium, Rust, or Lean defect blocked the spike. | Not present |

## Implemented Spike Surface

`mem-federation` contains the neutral envelopes, six record variants,
algorithm-tagged SHA-256 digests over RFC 8785 JSON, boxed standard-library
transport futures, the `MemoryEntryResponse` projection, and the transactional
replica reducer. Federation capabilities remain `Pull`, `Publish`, and
`ManageMembership`; the spike does not map them to application `PermissionSet`
roles.

`mem-atproto` pins `atrium-api` 0.25.8 with default features disabled,
`atrium-xrpc` 0.12.4, and `atrium-repo` 0.1.8. Its six handwritten,
unpublished Lexicons use explicit fields and UUID record keys. The adapter also
covers AT-URI/CID metadata, integer parts-per-million confidence, fake
`createRecord`, `applyWrites`, and `listRecords` calls, syntax fixtures, and MST
construction. It does not include OAuth, identity resolution, signing, or a
network client.

`formal/memory-federation` is pinned to Lean 4.33.1 without Mathlib or Aeneas.
It models the reducer after transport authenticity and proves:

1. duplicate replay is idempotent;
2. applied data implies an active publishing membership;
3. accepted memory versions remain strictly monotonic;
4. accepted memory projections preserve author, source, locator, digest, and
   evidence references;
5. revocation blocks later writes while preserving prior state;
6. ordered replay equals the reference fold.

The model contains no `sorry`, `admit`, or new axioms. The shared traces cover
happy path, duplicate delivery, equivocation, version gap, missing evidence,
tombstone, and a post-revocation write. Canonical-state and relation behavior is
covered by Rust reducer tests rather than additional cross-language traces.

## Validation Evidence

The final tree passed:

- `cargo test -p mem-federation -p mem-atproto --all-targets --locked`;
- the full `cargo test --workspace --all-targets --locked` suite;
- `cargo fmt --check`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `scripts/check-boundaries.sh`;
- `lake build`, `lake test`, and `lake env leanchecker MemoryFederation.Proofs`.

The focused Rust run contains 22 neutral-core unit/property tests, one shared
trace test, and 20 ATProto tests. The vendored CC0 syntax files, MST example
keys, and licence were byte-compared with the declared interoperability-fixture
commit `056e5741bb330757205d6b16db5266fffcae937b`.

CI has a scoped `formal` change bucket and uses
`leanprover/lean-action@38fbc41a8c28c4cbaec22d7f7de508ec2e7c0dd9` to run
Lake build/test, `leanchecker`, and nanoda with `nanoda-allow-sorry: false`.
This branch was not published merely to trigger remote CI; the action remains
the CI enforcement path.

## Explicitly Not Proven

None of the following is treated as passed by this spike:

- private permissioned project spaces;
- signatures or transport authenticity;
- encryption or key distribution;
- OAuth or identity resolution;
- unified live cursors;
- gateway operation;
- network liveness or relay behavior;
- live PDS compatibility and operational record-size limits;
- a service, database, CLI, UI, or production publication path.

These are separate security and operations decisions. In particular, the
adapter result is not evidence that a public AT repository is already suitable
for private Memory content.

## Deferred Ideas

Neuralese remains a possible application-level communication representation,
not a federation transport or authorization mechanism. Aeneas remains a
possible later route for relating Rust execution to Lean. Neither is required
to justify the adapter/profile result, and this spike adds no latent neural
payload field for either idea.
