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

Local commands:

```bash
~/.elan/bin/lake build
~/.elan/bin/lake test
~/.elan/bin/lake exe memory_federation_tests
```
