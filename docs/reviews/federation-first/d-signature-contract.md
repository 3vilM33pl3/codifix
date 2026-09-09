# Stage D first increment: runtime repository authenticity

Continuation of approved D2, starting from `c718524`. This is not completion of
all of D. No live deployment, DNS, production namespace, customer data or key
rotation is authorized by this increment.

- [x] Add a separate `mem-atproto-verify` security adapter, pinned to Atrium crypto 0.1.3 and existing repository/API versions. Keep crypto, AT identifiers and parsing outside the neutral domain/store and the original offline adapter.
- [x] Bound CAR framing before allocation; check SHA-256 CIDs, canonical commit fields, configured DID and low-S signature under an explicitly enrolled key. Validate full MST structure by bounded parsing and deterministic reconstruction, and require the exact expected record at its scoped key.
- [x] Require signed repository verification in the actual PDS publisher before any success acknowledgement. Require a configured signing key; no implicit trust-on-first-use or unsigned compatibility fallback. Fetch only the pinned PDS endpoint, with no DID-derived URLs.
- [x] Exercise wrong keys, forged DID/publisher, missing/changed records, malformed framing/tree/signatures, limits and readback spoofing, plus the real pinned PDS harness and existing durable workflow.
- [x] Update service configuration, boundaries, CI and operating guidance; run focused and workspace validation, checkpoint partial D progress, and preserve all remaining release gates.

Evidence: [first D increment](2026-09-09-d-repository-verification.md). The pinned
Atrium struct-order incompatibility was resolved in the isolated adapter and
checked against an independent upstream golden and real PDS. The master roadmap
remains 11/39; this five-item bounded increment does not replace or close it.

The explicit DID/key binding is an enrollment prerequisite, not proof of key
ownership supplied by this constructor. Automated safe DID/key discovery and
rotation, receiver verification/audit receipts, private gateway/PDS deployment
isolation, external-model policy, retention/legal holds and full D security review
remain subsequent increments. Native Spaces is not substituted. Repository proof
establishes integrity and origin under the configured key, not truth or human review.

Reference: [AT repository specification](https://atproto.com/specs/repository),
checked 2026-09-09; pinned Atrium source is inspected directly. The existing CAR
reader allocates from length prefixes, so it is not the untrusted-input entrypoint.
