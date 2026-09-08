# Origin-owned publication conformance

These are normalized **test commands**, not an ATProto record schema. Both
`crates/mem-federation/tests/publication_traces.rs` and
`formal/memory-federation/MemoryFederation/PublicationTrace.lean` consume these
same two files, expected outcome tags, final publisher sets and stream summaries.
Both assert every non-applied transition leaves the entire modeled state unchanged.
The Rust test expands commands into the real `PublicationState`, not another
test-only reducer, and compares replay with its step-by-step result.

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
- `grant`/`revoke` affect Publish only. Other capabilities and authority
  self-revocation defenses are tested in Rust, not modeled by these commands.
- Missing unused numeric fields mean zero; author defaults to `expert`, text
  to `knowledge`, and tombstone to false. Rust asserts unused fields are default
  so equality of well-formed commands matches equality of expanded deliveries.
- Memory content uses the same text/summary, decision type, confidence 800000 ppm,
  importance 3, no tags and no evidence. State status is active, confidence is
  800000 ppm. Causation is absent in this subset.
- Rust checks space, schema, canonical digest and the separately supplied
  authenticated publisher. Lean checks publisher equality but assumes other
  transport/encoding/shape checks have already succeeded. This is not a general
  JSON parser conformance test.

`lifecycle.json` covers independent counters, tombstones, changed authorship,
cross-origin ID collisions, revocation and replay. `order-and-ownership.json`
covers forged publishers, wrong origin, version/state/order gaps, missing content,
equivocation, stale state/order and unauthorized membership management. A stale
state must not silently describe a newer content version.

Evidence review, proposals, relation retraction, arbitrary locator tampering and
all-capability changes have additional Rust tests. They are not formalized here.
The old nine spike traces remain separate; their ATProto round-trips do not prove
that the new publication schema already has an interoperable wire adapter.
