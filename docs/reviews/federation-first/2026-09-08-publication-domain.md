# Federation-first publication domain — 2026-09-08

Status: **reference-domain increment complete; production federation and the full
roadmap remain incomplete**. Continue from the [previous checkpoint](2026-09-07-implementation.md),
not from the historical spike base. The [approved checklist](README.md) remains
the execution authority: A1–A4, B1 and R2 are now complete at their stated scopes
(6 of 39 items); B2/B3 and downstream release gates remain open.

## Implementation

- `dea907d`: define the origin-owned publication domain contract before coding.
- `bc68122`: implement `mem_federation::publication` within the existing neutral
  crate, reusing canonical JSON/digests/capabilities, with no new dependency.
- `6976b32`: add shared Rust/Lean publication traces, scoped proofs, property and
  negative tests, and the formal CI change trigger for the new fixture directory.

The domain uses non-nil UUID instance/space/record IDs and origin-qualified stream
keys. Authenticated publisher is checked separately from original authorship.
Shared publication versions, local source provenance and mutable-state revisions
are independent. A new content version invalidates the applicability of the old
state assertion without erasing that assertion or resetting the state counter.

Evidence contains only explicitly selected text, a public label and observation
time bound to a space/stream. Review digest validation rejects changed bytes or
context, including on deserialization. It does **not** authenticate the reviewer
or prove a human approved the text. The service must establish and persist that
approval. There is no automatic local capture/session/path/source projection.

Only an origin writes its stream. Other publishers can propose changes against an
exact shared version. Origin approval atomically appends the exact proposed content
and evidence as a new shared version, retains the proposing author's attribution,
and records the origin's approval key and chosen local source provenance. Stale
approval cannot overwrite newer content; stale rejection is still allowed.
Relations retain assertion/retraction history and cannot switch endpoints/type.

The authority manages membership and supplies contiguous accepted order. Exact
delivery retries are no-ops; changed record/locator/digest/order bindings are
equivocation. Missing dependencies and gaps quarantine without changing state.
Revocation blocks later publication without deleting history. The reducer stages
multi-part transitions on a clone so failure cannot leak partial mutations. This
is a deliberately simple reference, not a scalable database transaction strategy.

## Conformance and proof boundary

Eleven shared JSON traces now pass in both runtimes: the nine legacy spike traces
and two normalized publication traces. The Rust publication harness expands the
commands into real `PublicationDelivery` values and calls `PublicationState`.
Expected outcomes, final stream state and publisher membership agree; every
non-applied step preserves the complete state. Reordered-delivery property tests
converge after an explicit ordered resynchronization, not by ignoring gaps.

The new Lean model proves exact retry idempotence, non-applied state preservation,
accepted memory ownership/publishing membership and next shared version,
verbatim accepted-command retention, content preservation under state-only edits,
next state revision, revocation denying subsequent memory application, and
recursive replay/reference-fold equality. These are scoped transition lemmas,
not extraction or equivalence proofs for the complete Rust implementation.

UUID validation, positive bounded input counters, canonical encoding, digest and
space checks remain Rust prerequisites. The new evidence review, proposal and
relation transitions have Rust regression tests but **are not yet formalized**.
Neither model proves authentication, cryptography, HTTP, repositories, database
atomicity, process crash recovery or deployment safety. B3 remains unchecked.

## Local validation

- `cargo test -p mem-federation -p mem-atproto --all-targets --locked --offline`:
  36 neutral unit/property tests, 24 adapter tests, and two integration harnesses
  covering all eleven shared fixtures pass.
- `cargo test --workspace --all-targets --locked --offline -- --test-threads=1`:
  passes with temporary isolated XDG directories and inherited service token /
  database variables unset. Loopback test servers ran outside the socket sandbox.
  **PostgreSQL-gated tests did not exercise a database.**
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`,
  `cargo fmt --check`, `CARGO_NET_OFFLINE=true scripts/check-boundaries.sh` and
  `git diff --check`: pass.
- Lean 4.33.1 `lake build`, `lake test`, `lake env leanchecker`: pass. New proofs
  contain no `sorry` or new axioms; existing legacy proof-linter warnings remain.
- The official pinned Lean CI action now also triggers on publication fixtures.
  Remote CI and an independent local nanoda run were **not** executed.

The fixture integration initially caught invalid whitespace-bearing author IDs,
and Lean's JSON reader required explicit defaults for omitted non-optional fields.
Both were corrected, preserving one shared fixture format. Strict Clippy also
required diagnostic `expect` messages in the integration harness. These failures
were fixed, not suppressed or excluded from the final passing runs.

## What remains next

1. B2: explicit local permission/gateway capability mapping. Local roles and
   delegated ManageMembership must not implicitly become the configured remote
   space authority.
2. B3: extend the shared formal model to evidence, proposals, relation retraction
   and causation. Add production publication wire mappings and compatibility /
   unknown-field retention tests before exposing schema version 2 externally.
3. C/R1: transactional local jobs/outbox, gateway acceptance/outbound intent and
   receiver inbox/projections/cursor, with disposable PostgreSQL integration tests
   and crash/retry cases. Authority sequencing needs serialization and fencing;
   this in-memory counter does not supply those guarantees.
4. D/E: authenticated, isolated gateway/PDS and independently operated instances,
   reviewed publication/retrieval/proposals, revocation and Web/CLI/TUI parity.

The old Atrium adapter/fixtures remain unchanged; their previous round-trip/MST
passes are not evidence of schema-version-2 transport compatibility. No service
route, database migration, production ingestion/publication, credential change,
deployment or namespace publication was added. Native Spaces stays untested alpha
with disposable data only. Notion, financial models, pitch deck and startup
documents remain unchanged; paid-pilot, commercial and operational gates remain
open. This checkpoint does not claim a working federated product.
