# Memory / Codifix: federation-first product and execution roadmap

Approved for implementation on 2026-09-07. This replaces the earlier roadmap:
ATProto federation is required for the first paid pilots, not an optional later
feature. Engineering continuity is the initial commercial use case; the domain
and protocol remain general-purpose. Preserve expert knowledge locally, improve
it through reviewed corrections, and exchange selected knowledge across
organizational boundaries without sharing databases.

Implementation evidence: [2026-09-07 checkpoint](2026-09-07-implementation.md)
and [2026-09-08 domain checkpoint](2026-09-08-publication-domain.md), followed by
the [B2/B3 permission and conformance increment](2026-09-08-b2-b3.md).
The full roadmap remains incomplete; checked items have deliberately bounded scope.

## Source contracts and existing evidence

- Notion architecture: https://app.notion.com/p/robotmotel/Why-AT-Protocol-Should-Be-the-Communication-Fabric-for-a-Distributed-Memory-Layer-3be8990715f48190b7bac810bad21aab
- Meta expert-correction context: https://engineering.fb.com/2026/09/02/ml-applications/organizational-second-brain-ai-learns-from-experts/
- Existing spike: docs/future-development/federation-protocol-spike.md.
- Existing production contract: docs/developer/adr/0009-private-federation-v1.md.
- Upstream alpha: https://atproto.com/blog/atproto-spaces-alpha
- Spaces proposal: https://github.com/bluesky-social/proposals/tree/main/0016-permissioned-data

The offline Rust/Atrium/Lean spike and partial private-PDS harness are already
implemented, not production federation. Continue from the clean existing
codex/federation-v1 checkout; do not restart from df67156. The initial production
candidate remains one authoritative gateway per private space, explicit
publication, origin-owned records, reviewed evidence, offline copies and
Web/CLI/TUI parity. Native Spaces is a separate disposable-test-data experiment;
its alpha is not approved for production or sensitive data. Do not silently
substitute native Spaces, a forked PDS, security bypasses or a bespoke protocol.

## A. Protocol contract and prerequisites

- [x] A1 Pin upstream versions and specifications under test; document private-profile versus native-Spaces identity, addressing, authentication, repository verification, synchronization, revocation, deletion and audit differences. Evidence: fixtures/federation/upstream-lock.json and docs/developer/architecture/federation-profile.md; this is a pinned specification comparison, not a native Spaces runtime test.
- [x] A2 Re-run the existing PDS prerequisite assertions and complete missing recovery evidence before further product integration; stop for design review if a PDS fork or broad security bypass is required. Evidence: tests/federation-pds/README.md, including encrypted cold recovery into a fresh volume after source-volume removal (2026-09-07); full Memory/gateway recovery remains a product gate.
- [x] A3 Test pinned Atrium coverage for production operations and keep network/authentication dependencies outside the neutral core. Evidence: crates/mem-atproto/src/sdk_coverage.rs tests generated binary import/export, credential selection and fail-closed authentication handling; the profile documents missing network/authentication implementation and native Spaces support.
- [x] A4 Define schema compatibility, namespace ownership, evolution and publication policy before publishing production Lexicons. Evidence: docs/developer/architecture/federation-profile.md. Production namespace ownership is not verified and no production Lexicon was published.

Native Spaces compatibility tests must be bounded, disposable and non-sensitive.
The signed-MST experiment does not prove native Spaces compatibility. Its
repository commitments, synchronization, signature properties, read granularity
and deletion behavior require separate evidence and a future explicit decision.

## B. Production federation domain

- [x] B1 Extend the neutral domain with stable instance/space identities, authenticated publisher distinct from author, shared versions independent of local versions, mutable-state revisions, immutable reviewed evidence, origin-owned streams, proposals, membership capabilities, relation retraction, causation and deterministic authority ordering. Evidence: crates/mem-federation/src/publication and docs/developer/architecture/federation-publications.md. Complete at the pure reference-domain scope, not durable persistence, credential verification, production wire adaptation or end-to-end product integration.
- [x] B2 Define explicit local-permission/gateway-capability mappings without allowing local roles to grant remote authority automatically; retain all AT-specific types behind the adapter. Evidence: publication/policy.rs and docs/developer/architecture/federation-permissions.md; exact-project permissions intersect an explicit local grant and current gateway membership. All 2048 current local permission combinations are tested. This is a pure policy API, not enabled routes, authentication or durable local grant storage.
- [x] B3 Expand executable Rust/Lean conformance together, including the new production semantics; do not claim proofs of the complete service, runtime, cryptography or deployment. Evidence: five publication traces plus nine legacy traces run through Rust and Lean; PublicationInvariants.lean adds evidence, causation, proposal, relation and authority proofs. UUID/wire/hash/review validation and runtime guarantees remain outside the model; B2 local policy is exhaustively tested in Rust, not proved in Lean.

B1 implements the reference domain without changing the old six-record spike
schema or adapter. B2/B3 now complete the bounded domain/policy/model stage, not
the product. Fourteen shared traces cover all publication variants, independent
capabilities and causal links. Review-digest validity remains a Rust prerequisite,
not a proof of human approval or cryptography. Production schema-version-2 adapter
round-trips/unknown-field compatibility, local grant persistence and all
durable/runtime/two-instance integration gates remain open before publication.

## C. Durable publication and synchronization

- [ ] C1 Commit source mutations with publication intent; gateway authorization, reducer application, accepted order and outbound intent together; receiver inbox, projections and cursor together.
- [ ] C2 Publish to PDS idempotently after gateway acceptance, and expose queued, accepted, published and replicated as distinct states.
- [ ] C3 Handle duplicates, gaps, retries, unavailable peers and full resynchronization; preserve local work offline and never infer application order from repository keys or database sequence allocation.

## D. Trust boundary

- [ ] D1 Keep each instance database private and independently authorized; only the gateway accesses PDS credentials and endpoints, with no CAR/firehose/repository/cross-space bypass.
- [ ] D2 Verify identities safely without SSRF; reject forged publishers, malformed records and invalid signatures.
- [ ] D3 Keep raw captures, sessions, paths and unselected evidence local; ensure reviewed snapshot bytes equal published bytes; treat imported content as untrusted data and gate external-model forwarding with explicit space policy.
- [ ] D4 Implement and document revocation/offline-copy behavior, audit receipts, retention/deletion/legal-hold boundaries; administrators and admitted recipients can read plaintext, and previously delivered bytes cannot be recalled.

## E. End-to-end federated knowledge workflow

- [ ] E1 Enroll two independently operated Memory instances with separate databases and administrative boundaries in a private project space.
- [ ] E2 Review/publish selected memory and evidence, retrieve/cite on the second instance with publisher/author/version attribution, propose correction to origin, and approve as a new shared version preserving provenance.
- [ ] E3 Provide Web/CLI/TUI parity for membership, publication review, proposals and sync state; keep origin/sharing restrictions in retrieval results.
- [ ] E4 Verify disconnection, catch-up, duplication and revocation end to end, including denying subsequent access/publication according to policy.

## F. Conditional expansion after reliable knowledge federation

- [ ] F1 After reliable paid knowledge federation, add capability discovery and policy-controlled remote questions/answers with permitted input/output contracts.
- [ ] F2 Subsequently add durable work request/claim/result/failure/cancellation/approval lifecycles. Keep MCP local; add A2A only for an actual integration, and regional aggregation/views only for measured scale or residency needs.

F1/F2 are conditional later roadmap stages, not permission to bypass E or the
release gates. A private aggregation service needs its own authorization and
retention contract; native Spaces is not the public relay topology.

## Core reliability, evaluation, retrieval and expert correction

- [ ] R1 Replace in-memory post-curation work with durable jobs, bounded retries, leases and observable failures; close the mutation/event-log transaction gap.
- [x] R2 Repair parent/child evaluation token double-counting; separate physical execution costs, whole-task success and step diagnostics, and add regression tests. Evidence: crates/mem-eval/src/accounting_tests.rs. Overall statistics count paired tasks once; step-prefixed groups are non-additive diagnostics. Legacy mixed comparison artifacts require recomparison before release gating. This does not establish held-out product quality or complete provider billing coverage.
- [ ] R3 Run real isolated quality gates with fixed evaluation time and no reinforcement writeback; include handover-document and simpler-retrieval baselines at comparable information/budget, not only no-access baselines.
- [ ] R4 Apply authorized-project restrictions before candidate limits; batch source reads, bound concurrent retrieval, calibrate ranking by ablation, and exclude obsolete/tombstoned consolidation inputs.
- [ ] R5 Test changed numbers, negations, contradictions, stale facts, weak-evidence refusal, provenance collisions, malicious imported claims and cross-space retrieval; signatures must not imply truth or boost confidence automatically.
- [ ] R6 Link questions/citations/corrections to minimal proposed edits, targeted replay, regressions, human approval and rollback; recipients cannot silently rewrite an origin's authoritative stream.

## Application and infrastructure

- [ ] P1 Distinguish signed-out, forbidden, empty and offline UI states; prioritize evidence inspection, correction and publication review.
- [ ] P2 Separate dev/evaluation/customer databases and watchers; package independently deployable instances and gateway/PDS boundary rather than running customer workloads on the founder desktop.
- [ ] P3 Establish encrypted offsite backups and demonstrate recovery of records, evidence, identities, credentials, jobs and cursors without accepting stale writers.
- [ ] P4 Add security tests for HTTP, MCP, sync, exports and background processing; pass Rust/Lean conformance, real database integration, browser workflow and held-out engineering evaluation gates.

## Commercial, financial and pitch work

- [ ] M1 Define the boundary-problem buyer/pilot scorecard (engineering/MSP, business units, acquired entities, customer/supplier incidents); require two independently administered instances and measure time, expert interruptions, correction reuse, onboarding and operating cost.
- [ ] M2 Rebuild separate London (1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0) and New York (1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es) models for the same product scope: UK usage 2940 pounds versus independent 80000-pound ACV, US usage 25x UK, about 138% modeled retention versus 140% renewal, coherent cohort/cash sensitivities, real margins and correct use-of-funds basis.
- [ ] M3 Add federation identity/key, gateway/PDS, replica storage, transfer, recovery, security-review and multi-party support costs. Define annual platform/included-deployment/usage packaging without billing duplicate sync copies as new knowledge. Stress delayed pilots/support costs; revenue starts after federation gates. Recalculate both funding cases before hiring commitments.
- [ ] M4 Revise deck (1RtPlKvSxf3Z3dmLRHoIaiaKN3n8xtr6WGWJAzeWXCIY) and startup narrative for the two-organization demo, honest competitive comparisons, measured versus planned evidence, ATProto versus Memory responsibilities, privacy/availability limits and milestone-linked funding. Do not invent pilot outcomes or partner commitments.
- [ ] M5 Founder-led customer discovery, pilot agreements, two-party paid deployments and measured outcomes; standardize onboarding, annual conversions and support economics before later expansion.

Live deployment, DNS, new access, secret changes, customer agreements, hiring,
spending and external sensitive communications retain their explicit target and
authorization requirements. Approval to implement software is not authority to
make commitments on the founder's behalf. Prepare artifacts where external
evidence/authority is missing; record the remaining gate honestly.

## Sequence, ownership and mandatory release gates

Planning windows assume the required team is available: weeks 1-2 contract and
compatibility; 3-6 domain/durability; 7-10 secure two-instance integration; 11-14
pilot readiness; 15-20 paid pilots; later expansion after evidence. These are
estimates, not deadline-driven permission to weaken gates. Engineering owners:
federation/backend, retrieval/evaluation, platform/security and product/frontend.
Founder owns customer discovery and agreements.

- [ ] G1 Two separate Memory databases/admin boundaries complete the workflow with no unauthorized access through any supported transport; reviewed/published bytes and attribution match.
- [ ] G2 Invalid records/signatures/publishers rejected; duplicate/reordered/interrupted delivery safe; revocation/offline limits tested and visible; recovery consistent with stale writers fenced.
- [ ] G3 Rust/Lean conformance, database/browser/security suites and held-out useful engineering outcomes pass; native Spaces remains disposable-test-data only.
- [ ] G4 Verify the full approved checkbox plan before claiming completion, retain evidence and report incomplete/conditional/external gates honestly; use small incremental commits and remember verified implementation work.

Rust, the neutral core and existing evidence remain the baseline. Neuralese,
Aeneas, public Bluesky integration, peer consensus and automatic multi-region
failover remain deferred. This execution follows the user-approved roadmap;
source Notion architecture is not modified merely to record progress.
