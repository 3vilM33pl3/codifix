# Memory Federation Lean Spike

This package is an isolated Lean 4 slice for the federation protocol spike.

It intentionally stays outside the Rust workspace and models only a transport-neutral,
authenticated replication core:

- authority-checked grant and revoke records
- active and revoked publish capabilities
- evidence records keyed by source, locator, and digest
- memory version histories with evidence references
- replay detection and equivocation checks

The proofs are in `MemoryFederation/Proofs.lean`. The executable and tests read
the shared JSON traces under `../../fixtures/federation/traces/`.

The nine traces now include distinct original author/authenticated publisher and
forged authority authorship. Rust consumes the publisher field (previously its
spike harness ignored that field), matching the model's existing distinction.
The Rust `AuthenticatedRecord` input assumes transport authentication has already
established that identity. Constructing the value is not authentication, and the
Lean proofs do not verify credentials or signatures. The Rust-only defense against
reattributing an accepted source/locator is not an additional Lean theorem.

## Origin-owned publication model

`MemoryFederation/Publication.lean` separately models a subset of the new
`mem_federation::publication` domain. Two additional shared JSON traces under
`../../fixtures/federation/publication-traces/` run through both this model and
the real Rust reducer during `lake test` / Rust integration tests. The legacy
nine fixtures and their proofs are unchanged.

The new model proves exact retry idempotence, unchanged non-applied state,
accepted memory ownership/publishing membership and next shared version,
verbatim accepted-command retention, independent content/state projections,
next state revision, revocation denying subsequent memory application, and
recursive replay equality with the reference fold. The version statements are
one-step transition invariants, not a theorem about the complete Rust service.

Natural-number IDs abstract validated non-nil UUIDs. Positive `u32` counters,
canonical encoding, envelope digests, space binding and authenticated identity
are Rust/caller prerequisites, not Lean parser/cryptography proofs. The fixtures
use a fixed space/authority, derive locators from record IDs, and normalize unused
fields; see their [format contract](../../fixtures/federation/publication-traces/README.md).

The new evidence-review, proposal and relation transitions have Rust regression
tests but **are not in this new Lean model**. Local-role mapping, durable
acceptance, outbox/inbox, transport verification and gateway fencing remain
unimplemented. Neither the new nor legacy proofs establish those release gates.

Local commands:

```bash
~/.elan/bin/lake build
~/.elan/bin/lake test
~/.elan/bin/lake exe memory_federation_tests
~/.elan/bin/lake env leanchecker
```

The Lake package name matches the `MemoryFederation` root module. The pinned
lean-action infers that module name for both leanchecker and nanoda. Its nanoda
helper does not recognize Lake's top-level TOML package name, so this project
uses `lakefile.lean`, not a nonstandard `[package]` TOML workaround. The root
library is a default target so the independent checker has an exported module.
See the pinned [leanchecker helper](https://github.com/leanprover/lean-action/blob/38fbc41a8c28c4cbaec22d7f7de508ec2e7c0dd9/scripts/run_leanchecker.sh)
and [nanoda helper](https://github.com/leanprover/lean-action/blob/38fbc41a8c28c4cbaec22d7f7de508ec2e7c0dd9/scripts/run_nanoda.sh).
`lake build`, `lake test` and `leanchecker` can be checked locally with the pinned
toolchain. A local kernel check is not an independent nanoda or remote CI pass.
