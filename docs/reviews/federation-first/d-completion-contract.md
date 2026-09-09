# Stage D completion contract

Approved continuation of D1–D4 from `790a100`, not a replacement roadmap. All
implementation and tests use synthetic data; deployment/customer approvals,
E user interfaces and production release/recovery gates remain separate.

- [x] D1: provide a gateway-only executable/configuration and private deployment topology; test member access through the authenticated gateway and rejection of raw PDS/CAR/firehose, cross-space and database bypasses. No PDS fork, public exposure or disabled upstream SSRF protections.
- [x] D2: require independent signed gateway publication receipts and fresh authenticated pages; atomically retain/verify receipts with publication and receiver cursors. Explicitly enroll keys with bounded rollover windows, reject old writers/credentials and forged identity/signatures, and never resolve record-derived URLs.
- [x] D3: keep reviewed evidence exact and imports segregated as untrusted data. Enforce default-deny external-model use with current gateway policy, immutable publication restrictions and explicit local authority; test denial before provider access and retain provenance in permitted context.
- [x] D4: persist append-only independent audit receipts and retention/hold decisions; preserve accepted history and offline copies, distinguish tombstones from physical deletion, and block ordinary retention/erasure of pending or accepted federation state. Document administrative plaintext access, in-flight operations, inability to recall bytes, and explicit decommission/legal review boundaries.
- [x] Validate adversarial input, key rollover, revocation, rollback/tamper, restore/retained receipts, cross-space isolation and provider-policy behavior, plus workspace Rust, Clippy, boundaries, unchanged Rust/Lean traces and real pinned PDS integration. Update docs/checkpoints and verify all D items against evidence.

Decisions within the approved profile: identity and key enrollment remain explicit
administrator configuration, not dynamic DID discovery. Gateway audit keys are
separate from PDS signing keys; recipients receive scoped signed attestations,
not raw repositories. Key changes require explicit trust distribution and
sequence cutovers. Initial retention is fail-closed: no automatic physical purge
of accepted federation history; tombstones affect current visibility only.
Actual legal retention periods and whole-space decommission require the space
owners' documented policy/approval, not a software-generated legal decision.
