# F1 — Private expertise exchange for homelab and Memory

## Summary

Implement capability discovery and explicit remote questions for Olivier’s internal company use.

Start with **one Memory instance and database per project**: `homelab` and `memory`, connected through one private company gateway/PDS. Design enrollment so the game project can join later without code changes.

Each instance answers from an explicitly approved knowledge collection. Ordinary queries remain local. Web, CLI, TUI and dedicated local MCP tools provide explicit remote discovery and questions.

This task delivers **software and a disposable rehearsal**, not activation on existing homelab services. Record an explicit roadmap exception allowing internal F1 development before paid pilots; customer-release gates and unfinished R3 remain open.

## 1. Approved collections and capability discovery

- [x] Add transport-neutral expertise contracts in `mem-federation`: capability advertisements, immutable collection revisions, question envelopes, answer envelopes, citations and sanitized outcomes. Runtime, SQL and ATProto types remain outside the neutral core.
- [ ] A capability advertises its owner, reviewed name/description/topics, supported answer modes, collection revision/digest and input/output limits. It contains no endpoint, credentials, source contents or local paths.
- [ ] Collection preparation selects exact curated memory versions and explicitly selected evidence. Review displays every potentially disclosed field and its digest before activation.
- [ ] Activation grants standing permission to answer eligible requests from that frozen collection. Collection contents remain on the answering instance; discovery does not replicate them.
- [ ] New source versions require renewed review. Changed, tombstoned, withdrawn or unauthorized entries become ineligible immediately; never silently substitute newer content.
- [ ] Each collection explicitly allows requesting instance identities and model/output destinations. Joining the space alone does not grant access. Adding the game requires enrollment and deliberate collection access grants.
- [ ] Publish capability activation, revision and withdrawal through the existing ordered publication outbox and ATProto adapter. Reuse existing source-version, evidence-review and authenticated-attribution conventions.

- [ ] Compatibility: introduce an explicit schema-v3 extension for expertise advertisements and independent `AskExpertise` / `AnswerExpertise` capabilities. Preserve schema-v2 decoding, canonical bytes and history unchanged. Reject new semantics in old versions. Require compatible participants before enabling F1 in a space; no silent mixed-version downgrade. Keep Lexicons unpublished and retain current upstream pins.

## 2. Private question/answer transport and execution

### Gateway-mediated delivery

- [ ] Use a **bounded request/reply mailbox** in the gateway’s PostgreSQL database. Participants contact only their configured gateway and poll for addressed requests; no peer URLs, inbound responder ports or direct database access are introduced.
- [ ] Questions and answers are addressed to one requester and one responder. They never enter the PDS, shared publication log, general project timeline or retrieval corpus.
- [ ] The requester records its authenticated principal, exact question, selected capability revision, answer mode and permitted onward destination before transmission. Do not attach conversation history, files or local search results.
- [ ] Gateway authorization requires current memberships, explicit expertise capabilities and the advertised recipient policy.
- [ ] The responder uses a dedicated locally configured service identity with exact-project permissions and an explicit collection grant. It does not inherit the requester’s local identity.
- [ ] Recheck authority, collection eligibility and disclosure policy before model invocation, result acceptance and delivery. Revocation blocks future transfers; already transmitted bytes cannot be recalled.
- [ ] Bind signed gateway responses to request ID/digest, requester, responder, capability/collection revision, policy revision and expiry. Reuse enrolled receipt-key trust with distinct signature purposes. Gateway attestation establishes attribution and integrity, not answer truth.

### Recoverability without F2 expansion

- [ ] Add append-only migrations after the current migration head. Persist request identities, payload digests, delivery state, execution leases and metadata-only attempt history.
- [ ] Expose `queued`, `running`, `answered`, `refused`, `denied`, `expired`, `failed` and `uncertain`.
- [ ] Exact retries reuse the original request ID; changed payloads under that ID fail.
- [ ] Claims and result writes use lease tokens and compare-and-swap fencing. Duplicate delivery cannot start concurrent executions or overwrite a completed answer.
- [ ] A provider call with an ambiguous outcome becomes `uncertain`; do not automatically invoke it again. Explicitly starting another question warns about possible repeat billing.
- [ ] Reuse persisted completed results while retained. Expired requests cannot be restarted through replay.
- [ ] Shutdown and restart preserve recoverable delivery state. This mailbox supports bounded Q&A only—not delegated tasks, tools, recursive calls, approval workflows or F2 work orchestration.
- [ ] Defaults: two-second polling; 120-second question deadline; 30-second execution leases renewed every ten seconds; one active answer per responder; at most 32 outstanding requests per requester. Reject excess work with actionable status.
- [ ] Retain private question/answer payloads for 24 hours and metadata/idempotency receipts for 30 days. Enforce expiry during reads, not just cleanup. Respect active holds; do not claim deletion from backups or recipients’ copies.

## 3. Restricted retrieval, models and user interfaces

### Answer generation

- [ ] Search only eligible rows in the approved collection, applying collection/project restrictions **before ranking and limits**. Use existing lexical matching and deterministic citation helpers; do not introduce retrieval tuning, embeddings or reinforcement writeback.
- [ ] Default to deterministic excerpts with citations and an explicit insufficient-evidence outcome. Maximum input question: 4 KiB; selected context: 32 KiB; eight citations; complete response: 64 KiB.
- [ ] Include original author, authenticated answering instance, exact source version, collection revision, reviewed evidence labels and content digests. Do not invent AT-URIs for evidence that was never published.
- [ ] Support optional synthesis through the existing configured model client, using a restricted execution path: tool-free, structured output, bounded responses and metadata-only auditing. Never reuse the ordinary query prompt builder, which includes local source paths.
- [ ] Validate citations against the supplied evidence. Invalid citations, malformed answers or unavailable models produce explicit failures; no silent provider substitution.
- [ ] Model execution requires the intersection of current gateway policy, collection consent, requester consent and responder configuration. Advertisements contain provider IDs, never URLs.
- [ ] Model mode stays disabled until the operator configures allowed destinations and positive call/token budgets. Default enabled-mode limits are one invocation per question, 800 output tokens and 20 invocations per responder per day. Reserve limits transactionally and retain uncertain usage.

### Public interfaces

- [ ] Add shared client contracts for collection review/activation, capability discovery, question submission and question status/result.
- [ ] Under the existing project-and-space federation API, add collection draft, review/activate and withdraw operations; bounded capability listing/detail; question preparation/submission and request-ID status/result reads.
- [ ] Extend `memory federation` with `collections`, `capabilities`, `ask` and `answer` operations. Add equivalent Web and focused TUI workflows showing the exact destination, question, model mode and disclosure policy before confirmation.
- [ ] Add dedicated local MCP discovery, prepare, ask and result tools. Submission is an externally consequential operation, not an automatically approved read-only lookup.
- [ ] For MCP, require an operator-bound caller/output-destination policy: a cloud assistant receiving an answer is onward model disclosure. Unknown destinations fail closed; callers cannot self-authorize by naming an approved provider. Ordinary `memory_query` remains unchanged.
- [ ] Central authorization, project scoping, CSRF protection and read-only restrictions apply consistently. Operational listings omit question bodies, evidence, prompts, outputs and provider error bodies.

## 4. Verification and delivery

Implement in small committed slices:

- [x] Checkpoint this approved internal-use contract and update F1’s conditional roadmap status.
- [ ] Add neutral contracts, schema compatibility, advertisements and Rust/Lean conformance.
- [ ] Add collection review, immutable snapshots and authorization.
- [ ] Add gateway mailbox, restricted answering and recovery.
- [ ] Add Web/CLI/TUI/MCP workflows and operator documentation.
- [ ] Run the complete disposable rehearsal and record evidence.

Required tests:

- [ ] Homelab discovers Memory expertise and obtains a cited answer; repeat in the opposite direction.
- [ ] Unselected memories, raw captures, sessions, paths and another project’s data never enter responses or model inputs.
- [ ] Source updates, tombstones, collection withdrawal and permission revocation invalidate pending disclosure appropriately.
- [ ] Forged attribution, stale advertisements, replayed receipts, altered retries and invalid citations fail closed.
- [ ] Competing workers, restarts, gateway disconnection, expired leases and uncertain provider calls preserve correct state.
- [ ] A third synthetic game instance cannot ask until explicitly enrolled and allowed; afterward it works without code changes.
- [ ] Unauthorized participants cannot read question payloads or results through mailbox, publication, export or operational interfaces.
- [ ] Fake models verify permitted egress, prompt-injection containment, usage limits and sanitized auditing.
- [ ] Extend shared Rust/Lean traces for modeled ownership, revisions, permission checks and replay properties; do not describe these as proofs of deployed authentication or model behavior.
- [ ] Run required-database workspace tests, existing federation/PDS/isolation regressions, Rust formatting and Clippy, boundary checks, Lean build/tests, Web tests/build, the lean CLI build, and real browser/CLI/PTY/MCP rehearsal workflows.

Use synthetic data, temporary credentials and fake providers only. Keep existing services, project data, DNS, secrets and unrelated working-tree changes untouched.

**Completion means:** two disposable project instances discover and answer through the private gateway with verified disclosure controls, citations and recovery; third-instance enrollment is demonstrated; deployment instructions are ready. Real homelab activation, paid model calls, customer readiness, R3 completion and F2 remain separate work.

Implementation begins only after the approved-plan checkpoint succeeds through the working dev Memory connection. The currently failing MCP credential is not permission to rotate credentials or bypass that checkpoint.

## Execution evidence — 2026-09-10

The dev CLI checkpoint succeeded without credential changes: thread
`f1-private-expertise`, plan memory `212111f9-4bdf-4308-821f-acebd5ab8f27`.
The installed MCP connection's earlier failure was not bypassed or repaired.

Domain increment: neutral collection/question/answer contracts, explicit v3
expertise permissions and origin-owned advertisement revisions, v2 semantic
rejection and unchanged v2 projections, adapter unknown-field retention and
default-deny gateway v3 ingress. Six expertise tests (including repeated-delivery
property testing), the extended adapter tests and all prior focused tests pass.
The six publication traces plus nine legacy traces pass in Rust and Lean;
scoped Lean expertise ownership/revision/revocation proofs build without `sorry`.
Focused federation/adapter/store/runtime Clippy passes. This is not a working
remote question service: collection persistence, mailbox, client workflows and
disposable end-to-end evidence are still pending.

Persistence increment: migrations 0048–0049 add origin-only collection reviews,
explicit automation grants and metadata-only review journals, plus a separate
gateway mailbox, attempt journal and transactionally reserved model budgets.
Collection review/outbox writes are atomic. Exact retries, changed-source
exclusion, tombstones, explicit third-instance permission, concurrent claims,
expired-worker fencing, ambiguous provider reservations, holds and read-time
payload expiry are covered by required-database tests using disposable pgvector
PostgreSQL and fake receipt signatures. These tests establish storage behavior,
not network authentication or deployed isolation. Gateway-role grants exclude
origin collections and allow deletion only of expiring Q&A payloads/receipts.
The complete federation/store/runtime regression suite and workspace Clippy
passed for the collection increment; mailbox-specific store tests pass. There
are still no enabled F1 service routes, workers or end-user workflows, and the
two-service/PDS rehearsal remains pending. No existing service was reconfigured.

Transport and excerpt increment: loopback HTTP tests use real gateway receipt
signatures, a disposable gateway PostgreSQL database and two additional isolated
origin databases. Homelab and Memory discover the permitted capability and obtain
reviewed excerpts with exact source/author/evidence attribution in both
directions. An independently granted internal principal answers at each origin;
the requester identity is not reused. The tests verify exclusion of unselected
and cross-project rows, local root metadata, changed unreviewed versions and
fabricated citations. Separate transport tests cover exact/altered retries,
addressed reads, competing claims, restart, explicit game enrollment/allowlisting
and rejecting mixed schema inventories. Receipt tests reject wrong purposes,
nonces, audiences, keys, scopes, altered responses and invalid expiry windows.

**These are component tests, not the completion rehearsal.** Publication receipt
tests simulate the PDS outcome; the existing C/D suite separately exercises the
fake signed-PDS path. No Web/CLI/TUI/MCP F1 workflow exists yet. Optional models
remain rejected and release-mode gateways cannot enable the rehearsal RPC.

### Remaining implementation boundaries

1. Add durable requester preparation/consent and origin result persistence,
   automatic service-identity workers and lifecycle/reconnection handling.
2. Implement fresh origin eligibility authorization for **each delivery of a
   retained answer**, not just generation. Current mailbox reads recheck gateway
   membership, advertisement revision and policy, but cannot observe a local
   source update on their own. The component harness checks origin eligibility
   while generating; it does not close this later-delivery race. Production
   enablement must remain unavailable until this boundary is implemented/tested.
3. Integrate restricted models, configured call/token budgets, uncertain outcomes
   and metadata-only provider auditing. No paid invocation is authorized.
4. Add project-scoped review/consent/result endpoints and matching Web/CLI/TUI/MCP
   workflows, including operator-bound MCP onward destinations and fresh local
   permission resolution. Existing ordinary queries stay unchanged.
5. Complete the actual independently running service/PDS/game rehearsal, protocol
   operational documentation and the outstanding release-gate verification.

The first full workspace verification attempt failed because the filesystem was
full, not a failed assertion. Cargo cleanup removed 28.1 GiB of rebuildable dev
artifacts for five affected library crates only; source, project data and running
service executables were preserved. A lower-concurrency retry follows. The
focused six-crate required-DB suite passed before cleanup; Lean build and all 15
shared traces pass (pre-existing Lean linter warnings remain).

### Verification result for this increment

- Required-database workspace tests pass **serially**: 824 tests in 49 test
  binaries, zero failures/ignored tests, using the disposable PostgreSQL fixture.
  The normal parallel retry failed in `dev_init_writes_service_overlay` when its
  expected temporary XDG config resolved to the host config directory, then
  poisoned the shared test lock (nine follow-on failures). The successful serial
  retry is consistent with shared test-environment interference;
  the parallel run is not recorded as passed and no unrelated test was changed.
- Workspace all-target Clippy with `-D warnings`, formatting and boundary checks
  pass. The gateway still has no dependency on local search or model execution.
- Web: 76 tests across 22 files and production build pass. Existing bundle-size
  warnings remain. These are regression checks, not evidence of F1 UI coverage.
- Lean build/test: all 15 shared traces pass with the scoped reducer proofs.
- Lean CLI: `cargo build -p mem-cli --bin memory --no-default-features --features
  tui --locked` passes with debug information disabled in a separate temporary
  target directory; the resulting binary reports `memory 2.0.0`. This does not
  add or demonstrate the still-missing F1 commands.
- Actual pinned-PDS F1 rehearsal, browser/PTY/MCP user workflows, model execution
  and production activation remain **not implemented/not verified**.

The disposable PostgreSQL container was stopped/removed after verification;
its synthetic fixture data is regenerable from the tests. A resumable partial
checkpoint was recorded at `398cd01`. The approved plan was not marked complete.
