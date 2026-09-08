# Origin-owned publication conformance

These are normalized **test commands**, not an ATProto record schema. Both
`crates/mem-federation/tests/publication_traces.rs` and
`formal/memory-federation/MemoryFederation/PublicationTrace.lean` consume these
same five files, expected outcome tags, final capability sets, stream summaries,
retained evidence, proposal resolutions and relation history.
Both assert every non-applied transition leaves the entire modeled state unchanged.
The Rust test expands commands into the real `PublicationState`, not another
test-only reducer, checks every expanded delivery's serialization round-trip,
compares accepted history byte-for-byte with supplied deliveries, checks canonical
evidence bytes against their review digests, and compares replay with its
step-by-step result.

Mapping and prerequisites:

- Space is UUID integer 100; the configured authority is instance UUID integer 1.
- Positive integer identities become non-nil UUIDs using `Uuid::from_u128`.
  Lean abstracts UUID validation; Rust constructors reject invalid identifiers.
- `id` becomes the publication record ID and `record:{id}` locator. Publisher
  qualifies both. Lean record equality models the normalized delivery binding;
  it does not prove digest or arbitrary locator handling.
- `sequence`, `shared`, `local` and `revision` use positive `u32` where relevant.
  `local` is source-version provenance; shared order never follows it. Source
  record UUID integer 999 and authored time `2026-09-08T00:00:00Z` are fixed.
- `capability` is `publish` (default), `pull` or `manage_membership`; an empty
  string on `revoke` means revoke all. Removing the authority's membership-management
  capability is rejected. Grants to other managers do not change space authority.
- Missing unused numeric fields mean zero; author defaults to `expert`, text
  to `knowledge`, tombstone to false, `active` to true, `evidenceSpace` to 100 and
  `relationType` to `related_to`. Rust asserts unused fields are default
  so equality of well-formed commands matches equality of expanded deliveries.
- Memory content uses the same text/summary, decision type, confidence 800000 ppm,
  importance 3 and no tags. State status is active, confidence is 800000 ppm.
- `evidence` references and optional `causation`/`proposal` are qualified
  `{publisher,id}` keys. Missing, duplicated, wrong-kind and cross-origin evidence
  references are rejected/quarantined without changing projections.
- Evidence uses `text`, a fixed `reviewed-extract` public label and fixed observation
  time. Rust computes a real canonical snapshot digest and a receipt from reviewer
  `reviewer`; changed scope is modeled using `evidenceSpace`. Lean starts after
  review-byte validation; malformed receipt/hash tests remain Rust-only.
- Proposal target/base/content/evidence use `origin`, `canonical`, `shared`,
  `text` and `evidence`. Resolution requires the proposal's exact causal key;
  positive `local` accepts with origin-selected source provenance, zero rejects.
  Approval preserves proposer authorship/content/evidence and the origin's key.
- Relations use `relationId`, `targetOrigin`, `targetCanonical`, `relationType`,
  `revision` and `active`. Relation ID is scoped by publisher, independently of
  the immutable publication event ID. Retraction appends history.
- Rust checks space, schema, canonical digest and the separately supplied
  authenticated publisher. Lean checks publisher equality but assumes other
  transport/encoding/shape checks have already succeeded. This is not a general
  JSON parser conformance test.

`lifecycle.json` covers independent counters, tombstones, changed authorship,
cross-origin ID collisions, revocation and replay. `order-and-ownership.json`
covers forged publishers, wrong origin, version/state/order gaps, missing content,
equivocation, stale state/order and unauthorized membership management. A stale
state must not silently describe a newer content version.

`evidence-and-proposals.json` tests immutable evidence, dependency/scope failures,
origin-only atomic approval, stale/closed/missing proposals and post-revocation
approval. `relations-and-causation.json` tests missing causes/targets, endpoint/type
stability, revision gaps, retraction/replay and cross-origin relation ID collisions.
`capabilities.json` tests independent capability grants/revocations and authority
protection. Arbitrary locator/digest tampering and malformed review receipts have
additional Rust tests outside the normalized formal inputs.

The old nine spike traces remain separate; their ATProto round-trips do not prove
that the new publication schema already has an interoperable wire adapter.
