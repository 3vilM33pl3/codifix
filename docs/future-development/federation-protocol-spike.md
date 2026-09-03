# Federation Protocol Spike

This spike is an offline, non-production proof for Memory federation. It exists to test a neutral Rust model, an isolated ATProto/Atrium adapter, and a handwritten Lean model against the same conformance traces before any protocol choice is made.

## What It Must Cover

- a pure `mem-federation` crate that stays transport-neutral and depends on `mem-record` only;
- six neutral records: memory version, canonical state, evidence, relation, membership, and revocation;
- `SharedRecordEnvelope`, `TransportRecord`, `DedupKey`, `ProjectSpaceTransport`, and the replay/apply reducer surface;
- a separate `mem-atproto` adapter that keeps ATProto concerns isolated from the neutral core;
- a `formal/memory-federation` Lean model that proves the reducer invariants with no `sorry` or new axioms;
- shared JSON traces for happy path, duplicate delivery, equivocation, version gap, missing evidence, tombstone, and post-revocation write.

## Boundaries

This spike adds no database migrations, service routes, CLI/UI behavior, live PDS, gateway operation, or production publication path. It also avoids signing, encryption, OAuth, live cursors, and network-liveness claims.

## Decision Gate

The spike report must end with exactly one outcome:

1. Proceed with ATProto as an adapter/profile.
2. Design a bespoke external protocol.
3. Inconclusive only for an isolated Atrium/tooling defect.

That recommendation is evidence-based and should not be made before the Rust, adapter, and Lean slices all run against the same traces.
