# Memory / Codifix: federation-first product and execution roadmap

Approved for implementation on 2026-09-07. This replaces the earlier roadmap:
ATProto federation is required for the first paid pilots, not an optional later
feature. Engineering continuity is the initial commercial use case; the domain
and protocol remain general-purpose. Preserve expert knowledge locally, improve
it through reviewed corrections, and exchange selected knowledge across
organizational boundaries without sharing databases.

Commercial direction refined on 2026-09-15: Codifix is the knowledge and governance
network for the Agentic Enterprise. The first buyer/pilot focus is incident
handover between application and platform/operations business units, each with
independent administration and local agents. The completed
[M1 preparation pack](m1-buyer-pilot-scorecard.md) records the shift from
centralized-memory positioning and engineering/MSP-first buyer selection.
Customer deployments, financial-model rebuilds and full deck revisions remain
separate milestones.

Implementation evidence: [2026-09-07 checkpoint](2026-09-07-implementation.md)
and [2026-09-08 domain checkpoint](2026-09-08-publication-domain.md), followed by
the [B2/B3 permission and conformance increment](2026-09-08-b2-b3.md) and
[Stage C durability foundation](2026-09-08-c-durability.md) and
[completed C integration](2026-09-08-c-completion.md). The
[repository-authenticity increment](2026-09-09-d-repository-verification.md) is now
followed by [completed Stage D implementation](2026-09-09-d-completion.md) and
[Stage E end-to-end integration](2026-09-09-e-completion.md) and
[R1 durable-work completion](2026-09-10-r1-completion.md).
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
not a proof of human approval or cryptography. Stage C now adds schema-version-2
adapter compatibility, durable local grants and source/gateway/receiver runtime
integration. Production security, deployed two-party user workflows and external
namespace publication remain gated below.

## C. Durable publication and synchronization

- [x] C1 Commit source mutations with publication intent; gateway authorization, reducer application, accepted order and outbound intent together; receiver inbox, projections and cursor together. Evidence: migrations 0033–0035, mem-federation-store sharing APIs/deferred source hooks and real three-database C workflow. Explicit reviewed stream opt-in; no default enrollment or automatic evidence expansion.
- [x] C2 Publish to PDS idempotently after gateway acceptance, and expose queued, accepted, published and replicated as distinct states. Evidence: schema-v2 Atrium adapter, mem-federation-runtime immutable create/exact readback and status APIs; real pinned-PDS test loses a successful reply and independently verifies the resulting signed repository. Status UI remains E; runtime signature verification remains D.
- [x] C3 Handle duplicates, gaps, retries, unavailable peers and full resynchronization; preserve local work offline and never infer application order from repository keys or database sequence allocation. Evidence: durable worker scheduling, current local/remote permission checks, leased retry/writer fencing and atomic verified-prefix paginated replay; real PostgreSQL, service-worker and PDS regressions. Local PostgreSQL is required; O(history)/16 MiB resync limits are explicit.

The [C completion evidence](2026-09-08-c-completion.md) closes the foundation's
missing source hooks, schema-v2 live PDS path and authenticated worker scheduling.
Service startup is opt-in; trusted review/enrollment APIs are in-process at this
stage. Separate databases/runtime instances in controlled tests are not completed
customer installations or browser workflows. E/R1 and all release gates remain
open; no customer deployment or production Lexicon publication is authorized.

## D. Trust boundary

The [D completion evidence](2026-09-09-d-completion.md) covers the dedicated gateway,
private topology, signed publication/receiver proofs, explicit key rollover,
scoped untrusted model context and audit/hold semantics. This closes D's implemented
boundary, not E's user journeys or deployment-specific security, full recovery,
legal/customer and release decisions.

- [x] D1 Keep each instance database private and independently authorized; only the gateway accesses PDS credentials and endpoints, with no CAR/firehose/repository/cross-space bypass. Evidence: dedicated executable, restricted SQL grants and two-space disposable TLS/container isolation; not customer installation.
- [x] D2 Verify identities safely without SSRF; reject forged publishers, malformed records and invalid signatures. Evidence: pinned DID/PDS verification, receipt chains, fresh signed pages, explicit key windows and recovery revalidation; no dynamic discovery.
- [x] D3 Keep raw captures, sessions, paths and unselected evidence local; ensure reviewed snapshot bytes equal published bytes; treat imported content as untrusted data and gate external-model forwarding with explicit space policy. Evidence: immutable reviews, segregated imports, scoped context and guarded default-deny provider API; E adds explicit user retrieval/citation, not automatic external-provider forwarding.
- [x] D4 Implement and document revocation/offline-copy behavior, audit receipts, retention/deletion/legal-hold boundaries; administrators and admitted recipients can read plaintext, and previously delivered bytes cannot be recalled. Evidence: immutable receipts/history/intents, authority-only hold/erasure requests, tested revocation/local access and decommission limits; legal policy and full deployment DR remain release gates.

## E. End-to-end federated knowledge workflow

The [completed E integration](2026-09-09-e-completion.md) builds on the historical
[first API increment](2026-09-09-e-service.md). Evidence uses two separate Memory
service processes, databases and credentials on a controlled host, an actual
gateway and pinned PDS, and real browser/CLI/PTY clients. It is not a customer
deployment, customer SSO proof, or clearance of the remaining release gates.

- [x] E1 Enroll two independently operated Memory instances with separate databases and administrative boundaries in a private project space. Evidence: two_instance.rs and real-PDS client gate; separate OS processes, non-superuser database credentials and application/gateway identities; cross-instance DB connection and HTTP-token reuse denied. Controlled synthetic deployment only.
- [x] E2 Review/publish selected memory and evidence, retrieve/cite on the second instance with publisher/author/version attribution, propose correction to origin, and approve as a new shared version preserving provenance. Evidence: browser journey, idempotent HTTP actions and correction regression tests. Approval pauses local subscription without laundering imported text into ordinary local memory; fresh review reconciles shared counters.
- [x] E3 Provide Web/CLI/TUI parity for membership, publication review, proposals and sync state; keep origin/sharing restrictions in retrieval results. Evidence: Web federation workspace, memory federation CLI and focused TUI, shared local API, component tests and real browser/CLI/PTY interactions. Credentials/trust remain administrator configuration.
- [x] E4 Verify disconnection, catch-up, duplication and revocation end to end, including denying subsequent access/publication according to policy. Evidence: real gateway disconnect/local queue/reconnect, repeated full resynchronization, revoked writer accepted-sequence check, remote access denial and retained-copy/local-grant behavior; isolation harness rerun.

## F. Conditional expansion after reliable knowledge federation

- [x] F1 Add capability discovery and policy-controlled remote questions/answers with permitted input/output contracts. Approved internal-use exception (2026-09-10), software/rehearsal completed 2026-09-11: synthetic `homelab` and `memory` instances exchange cited answers, with deliberate game enrollment, real Web/CLI/TUI/MCP, pinned-PDS and TLS-isolation evidence. See the [approved F1 contract and execution checklist](f1-execution.md) and [operator runbook](../../user/f1-internal-expertise.md). This does not approve deployment on existing services, paid calls, customer release or R3/F2 completion.
- [ ] F2 Subsequently add durable work request/claim/result/failure/cancellation/approval lifecycles. Keep MCP local; add A2A only for an actual integration, and regional aggregation/views only for measured scale or residency needs.

F1 has the bounded internal-use exception above; F2 remains conditional. Neither
is permission to bypass the customer release gates or claim R3 is complete.
A private aggregation service needs its own authorization and
retention contract; native Spaces is not the public relay topology.

## Core reliability, evaluation, retrieval and expert correction

- [x] R1 Replace in-memory post-curation work with durable jobs, bounded retries, leases and observable failures; close the mutation/event-log transaction gap. Evidence: [R1 completion](2026-09-10-r1-completion.md), [coverage](r1-coverage.md) and [rollout/repair](../../developer/architecture/r1-rollout.md). All 16 scoped engineering items verified; customer DR, deployment readiness, evaluation quality and F remain separately gated.
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

- [x] M1 Define the boundary-problem buyer/pilot scorecard. Completed preparation pack (2026-09-15): [buyer, discovery and pilot dossier](m1-buyer-pilot-scorecard.md), [financial inputs and pitch brief](m1-finance-pitch.md), and [tabletop/publication evidence](m1-execution.md). First focus: application/platform business units with two independently administered instances; measure time, expert interruptions, correction reuse, onboarding, cost and quality. [Notion business summary](https://app.notion.com/p/3dc8990715f4815a812bf2a9262ef282). This completes preparation, not customer discovery, validated ROI, pricing, deployments or M2–M5.
- [x] M2 Rebuild separate London and New York native models as alternative bases for the same Agentic Enterprise product/workload. Original M2 and the employee AI revision completed 2026-09-15; the current [legal counsel and assurance leadership revision](m2-legal-assurance-revision.md) includes nine employees plus fractional counsel, with [current and historical evidence](m2-model/README.md#evidence-index), [verified refresh](m2-model/refresh-guide.md), and [Notion summary](https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a). Year-end staff are 8/9/9; first-24-month employee AI spending and retained headroom are each £420,000. Verified Base gross funding is £3.23m/$4.91m. The combined delay/support/full-AI stress ends month 36 with £120,768/$356,370 shortfalls. Shared commercial cohorts and 137.75% derived retention are unchanged. The historical external-assurance allowance does not establish complete initial or recurring ISO/SOC coverage; M3 must obtain scoped quotes. Financial preparation is complete; staffing, prices, capacity and revenue remain hypotheses.
- [ ] M3 Validate the provisional federation identity/key, gateway/PDS, replica storage, transfer, recovery, security-review and multi-party support costs now exposed by M2. Measure actual employee AI spending, engineering throughput, accepted output, review/rework, defects and delivery capacity before treating the reduced team as proven. Validate annual platform/included-deployment/usage packaging without billing duplicate sync copies as new knowledge, provider/workload rates, support capacity and payment terms. Replace M2 estimates with evidence, stress delayed pilots/support costs and recalculate both funding cases before hiring commitments; revenue remains conditional on federation/customer readiness gates. Validate fractional legal scope, specialist work and the accounting/payroll split; assurance-lead compensation, availability and programme capacity; and scoped initial/recurring ISO 27001, ISO 42001 and SOC 2 Type II fees, independent audit, testing and evidence tools. Agree organisational/system boundaries and evidence periods before any assurance claim.
- [x] M4 Revise the [native investor deck](https://docs.google.com/presentation/d/1RtPlKvSxf3Z3dmLRHoIaiaKN3n8xtr6WGWJAzeWXCIY/edit) and narrative. Completed 2026-09-17: fourteen core slides plus four appendix slides, with the [M4 dossier](m4-deck/README.md), [claim register](m4-claim-register.md), [live financial traceability](m4-deck/financial-evidence.md) and [Notion summary](https://app.notion.com/p/3de8990715f481448e6dc392c3f853c4). Native backup, original URL/sharing, editability, notes, sources and all eighteen rendered slides verified. The Agentic Enterprise story starts with application/platform incident handover; ATProto/Memory trust boundaries and unvalidated R3 results are explicit. No fixed fundraising ask; London £3.23m and New York $4.91m are alternative appendix scenarios. M3 costs/capacity/assurance validation and M5 customer evidence remain open.
  - Compact infrastructure v2 completed 2026-09-17: [ten-slide native deck](https://docs.google.com/presentation/d/1dMosQKiMjTx1k9yztYfiURaxQTj_HtOSgrmDYW9Ot_g/edit), [v2 dossier and PDF](m4-deck-v2/README.md), [claim register](m4-deck-v2/claim-register.md), and [Notion summary](https://app.notion.com/p/3de8990715f48198a8f3f9a4ac34092d). Codifix is the Agentic Enterprise infrastructure; external platforms retain native interfaces and Codifix builds adapters. Databricks is proposed, Neuralese is research. All ten slides and source/model boundaries verified; original eighteen-slide deck preserved. M3 must separately scope and cost broader connectors and any Neuralese programme; existing payroll/AI funding does not establish coverage. M3/M5 remain open.
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
