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

Stable instance IDs, shared-versus-local versions, mutable-state revisions,
origin ownership, proposals and durable gateway ordering are still forthcoming;
these proofs must not be presented as covering that unimplemented contract.

Local commands:

```bash
~/.elan/bin/lake build
~/.elan/bin/lake test
~/.elan/bin/lake exe memory_federation_tests
```
