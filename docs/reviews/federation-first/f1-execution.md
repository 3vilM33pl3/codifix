# F1 — Private expertise exchange for homelab and Memory

## Summary

Implement capability discovery and explicit remote questions for Olivier’s internal company use.

Start with **one Memory instance and database per project**: `homelab` and `memory`, connected through one private company gateway/PDS. Design enrollment so the game project can join later without code changes.

Each instance answers from an explicitly approved knowledge collection. Ordinary queries remain local. Web, CLI, TUI and dedicated local MCP tools provide explicit remote discovery and questions.

This task delivers **software and a disposable rehearsal**, not activation on existing homelab services. Record an explicit roadmap exception allowing internal F1 development before paid pilots; customer-release gates and unfinished R3 remain open.

## 1. Approved collections and capability discovery

**Current focus (2026-09-11):** delivery slice 4 is component-implemented for
deterministic answering and recovery; work has entered Section 3's public
interfaces. Browser-session HTTP preparation/submission, capability browsing and
original-requester result reads are implemented and tested at component level.
Collection draft/activate/withdraw and local browsing/detail HTTP are implemented;
The Web requester panel has component-tested discovery, confirmation and result reads;
Web collection management and service-grant controls also have component coverage;
CLI/TUI/MCP remain pending, followed by the full disposable
automatic-worker rehearsal. Open checkboxes are end-to-end completion gates;
the dated evidence below records individual implementation increments.

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
2. Integrate the component-tested fresh origin authorization into the complete
   service rehearsal. Each retained-answer delivery now requires a live,
   single-use origin check; metadata/status and exact submit retries contain no
   answer. The optional service delivery validator is implemented, but automatic
   question execution and the full requester workflow remain outstanding.
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

### Retained-answer delivery increment — 2026-09-10

Commits `24ce30b` and `de05bc9` add a fresh, nonce-bound origin check for every
retained-answer transfer, plus an optional disposable-only service validator.
Migration 0050 adds metadata-only delivery challenges and attempt actions. Each
approval is addressed, lease-fenced, expires within 15 seconds and can release
only the original immutable answer to the live waiting read. Exact submit retries
and metadata reads cannot bypass this check. Gateway writer/key recovery expires
old challenges. Retention cleanup respects holds and does not erase publication
or audit history.

The validator polls every two seconds using a separate internal identity. It
resolves and locks current project permissions, collection grants and reviewed
source eligibility through its approval call. It rederives deterministic excerpts
and exact citations rather than trusting stored text. It does not execute queued
questions or invoke providers. Model answers remain disabled. The gateway release
commit is an authorization point, not a distributed transaction with the origin
or proof that the recipient received bytes. Already-authorized in-flight transfers
cannot be recalled; disconnected origins cannot authorize fresh reads.

Required-database store/runtime regressions pass, including two independent
origin databases, real signed loopback HTTP, denial after source-only updates and
local collection-grant revocation, altered/replayed approvals, expired challenges,
writer fencing, journal-failure rollback and the 32-request bound. All ten
federation service tests pass, including current project permission locking,
revocation, rejecting global worker authority, disposable-only configuration,
separate identity enforcement and worker shutdown. Workspace Clippy with
`-D warnings`, formatting and boundary checks pass. A partial dev Memory
checkpoint was saved at `de05bc9`; no completion verification was invoked.

The required-database workspace all-target test run also passes serially: 827
tests across 49 test binaries, with zero failures or ignored tests
(`MEMORY_LAYER_TEST_REQUIRE_DB=1`, `--locked`, `--test-threads=1`), including the
existing federation C/D/E and durable-job regressions. Parallel workspace tests,
Web, Lean, the lean CLI build and the container-network/PDS isolation rehearsal
were not rerun for this increment; their prior evidence above is not relabeled
as new coverage. Logs are in `/tmp/memory-f1-delivery.U72WlQ/` (temporary, not a
durable artifact). The disposable PostgreSQL fixture was stopped after testing.

The earlier full-rehearsal gaps remain: requester preparation/consent, durable
origin execution/results, synthesis, user interfaces and actual PDS/game service
rehearsal. Existing services and credentials were not changed. This increment
does not mark any end-to-end F1 requirement or customer gate complete.

### Requester consent persistence increment — 2026-09-10

Commit `91100f7` adds origin-private question preparation, exact digest
confirmation and transaction-aware retrieval of consented questions. Migration
0051 stores the exact private payload separately from its metadata-only receipt;
neither enters a project timeline, PDS, gateway database or retrieval corpus.
Preparation is read-only. Confirmation and the receipt commit together, and
concurrent exact retries reuse the original request ID without replacing consent.
Any changed question, principal, project or reviewed advertisement fails closed.

Each read rechecks the current local `AskExpertise` grant, active principal,
project read/export permissions, read-only policy, capability consent and the
operator's permitted output destinations. Model mode remains denied. The store
is a trusted in-process boundary: the future service must bind authenticated
identity, lock current project permissions, obtain the signed advertisement and
supply operator-owned destination policy. A client-supplied allowlist is not
authorization. Service submission must commit consent first, then reload it in
a fresh transaction before contacting the gateway. This increment does not yet
wire that service or user workflow, or claim that a remote request was sent.

Payload retention is 24 hours and metadata retention is 30 days. Cleanup respects
holds, while read-time expiry applies even under a hold. Expired questions cannot
be restarted through a retained consent receipt. The new tables are inaccessible
to the deployment gateway role. Disposable database tests cover rollback,
concurrent confirmation, journal failure, changed-input and stale-advertisement
rejection, exact identity/project scope, revoked grants, empty destination policy,
read-only operation, expiry and held-payload cleanup. No existing deployment or
credential was changed. Automatic origin execution/results, HTTP/Web/CLI/TUI/MCP
workflows, optional synthesis and the complete service/PDS/game rehearsal remain
outstanding; F1 is not complete.

Verification for this increment: 829 tests across 50 test binaries pass, with
zero failures or ignored tests. Required-database workspace all-target tests
run serially with `MEMORY_LAYER_TEST_REQUIRE_DB=1` and `--locked`; workspace
all-target Clippy (`-D warnings`), formatting and boundary checks also pass.
Existing Rust federation regressions are included. Web, Lean, lean CLI and the
container/PDS isolation rehearsal were not rerun for this storage-only change.
Temporary logs are under `/tmp/memory-f1-consent.BYrd9m/`. The disposable database
was stopped/removed after verification. A partial dev Memory checkpoint records
`91100f7`; the approved plan remains open.

### Deterministic origin execution increment — 2026-09-11

Commits `a114e75` and `7ffa5ef` add private immutable result persistence (migration
0052) and automatic deterministic answering in the disposable-only service
worker. Answer and delivery loops are independent but owned by one cancellable
task. Each stage is bounded to 12 seconds, polls every two seconds and renews an
active answer lease at ten seconds. No model, ordinary query prompt, external
retriever, reinforcement writeback or detached task is introduced.

Current project service authority and eligible source rows are locked during
generation. A result and its digests commit locally before gateway completion;
a new transaction rechecks authority, source eligibility and exact citations
before transfer. Recovered deterministic work may recompute the same excerpt,
but cannot overwrite the committed answer with different bytes. Gateway lease
fencing and the previously implemented fresh per-read delivery check remain
authoritative. Local persistence is not proof of gateway acceptance; a failed
HTTP acknowledgement does not roll back a previously committed local result.

The store tests cover rollback, duplicate recovery, changed-result and project
rejection, expired leases, held payload expiry and cleanup. The gateway runtime
SQL role cannot access the new result table. The two-origin signed-HTTP component
test now commits local results and invokes the new guarded completion path in
both directions. All ten federation service tests pass, including lifecycle and
permission regressions. These tests do **not** establish the full automatically
scheduled two-service/PDS workflow or every process-crash window. Automatic
retention maintenance, requester HTTP/UI submission wiring and exact operational
failure presentation still require integration. Optional models and all end-user
F1 workflows remain unfinished. The approved plan is not complete.

All 830 tests across 50 test binaries pass, with zero failures or ignored tests.
Required-database workspace tests run serially with `--all-targets --locked`
and `MEMORY_LAYER_TEST_REQUIRE_DB=1`; workspace Clippy (`-D warnings`), formatting
and boundary checks pass. Temporary logs are in
`/tmp/memory-f1-answer.BqxJVy/`. Web, Lean, lean CLI and the container-network/PDS
isolation harness were not rerun for this increment. The disposable database was
stopped/removed after verification; no existing service or credentials changed.
The Memory skill saved a partial checkpoint at `7ffa5ef` without marking the plan
complete.

### Browser requester HTTP increment — 2026-09-11

Commit `0e3edae` adds project-and-space-scoped `questions/prepare` and
`questions/submit` routes connecting authenticated browser review to the private
gateway. Opt-in `expertise_browser: true` is accepted only for disposable debug
configuration; it does not enable the production gateway or create grants. This
path is deterministic and `local_human` only: service tokens, legacy API tokens
and MCP callers are denied until operator-bound destination policies exist.

Preparation uses verified retained advertisements and returns all exact question
fields and a digest without transmission. Submission binds the current session
principal and configured project/instance, commits consent, opens a fresh
transaction, rechecks session/project authority and local AskExpertise permission,
then transmits. The gateway rechecks current membership/revision. Network failure
cannot undo local consent; retries preserve the review and request ID. Responses
are metadata-only and no-store. Central route policies, CSRF, body limits and
sanitized errors apply; OpenAPI inventory is updated.

The HTTP test's separate gateway connection observes the exact committed consent
and receipt before submission handling. Tests cover preparation without sending,
duplicates, forged identity, altered review, destination tampering, cross-project
access even with both project grants, read-only mode, CSRF, token-caller rejection,
membership removal and session revocation. This synthetic gateway fixture is not
the final two-instance automatic-worker rehearsal. No frontend, CLI/TUI/MCP tool
or result-read endpoint is claimed complete.

Verification: 831 tests across 50 binaries pass with zero failures/ignored tests
in the required-database serial workspace run (`--all-targets --locked`,
`MEMORY_LAYER_TEST_REQUIRE_DB=1`). Workspace Clippy (`-D warnings`), formatting,
boundary checks and OpenAPI inventory pass. Web/Lean/lean-CLI and the full
container/PDS isolation rehearsal were not rerun for this increment. Temporary
logs are at `/tmp/memory-f1-requester.JiEjn1/`. The disposable database was removed
after verification; existing services and credentials were untouched. The Memory
skill saved a partial checkpoint at `0e3edae` without completing the plan.

### Browser discovery and retained result increment — 2026-09-11

Commit `85892fe` adds bounded capability browsing and original-requester private
status/result HTTP reads under the same disposable browser-only opt-in. Discovery
uses signed gateway replies and filters for deterministic `local_human` support;
pagination follows the underlying bounded page even when filtering removes rows.
Retained reads require exact committed consent, current requester authority and
unchanged advertisement consent. Runtime verification binds the entire returned
question identity and digest. Completed answer delivery still requires fresh
origin approval; no local cached-answer bypass was introduced.

Execution expiry and retention are separate: completed results can be read after
the execution deadline, but never after payload expiry, even under a hold.
Submission remains deadline-bound. Session authority and consent retention are
checked again after waiting for the origin. Another administrator cannot read the
original requester's private result. Responses remain no-store; bearer/MCP callers
and read-only mode remain denied by this conservative browser path.

Tests cover bounded pagination, malformed cursors, token denial, queued HTTP
status, original-principal ownership, membership/session revocation and retained
consent expiry. The separate two-origin runtime fixture now exercises completed
answer delivery through the new result wrapper and fresh origin approval. This
is not a completed-answer browser/automatic-worker end-to-end rehearsal.
Collection management HTTP, actual Web/CLI/TUI/MCP workflows, optional models and
the full disposable rehearsal remain pending. No completion gate was checked off.

Verification: required-database serial workspace tests pass (831 tests across
50 binaries, zero failed or ignored), as do workspace Clippy (`-D warnings`),
formatting, boundary checks and OpenAPI inventory. Logs are at
`/tmp/memory-f1-reads.Od2eC0/`. Web, Lean, lean CLI and full container/PDS isolation
rehearsals were not rerun in this increment. The disposable database was removed;
existing services, project data and credentials were untouched.

### Browser collection lifecycle increment — 2026-09-11

Commit `1eaf800` connects existing collection storage to project/space-scoped
browser draft, activate and withdraw routes. Drafting uses a caller-owned
transaction so current authentication locks cover source inspection. It neither
saves standing consent nor transmits a record. Activation requires the exact
review digest, authenticated reviewer, unchanged source versions and explicit
local/observed gateway Publish and AnswerExpertise capabilities. Snapshot,
sanitized audit and advertisement intent commit together. Withdrawal disables
local consent and queues the inactive advertisement atomically. Exact retries
reuse the stored intent; queued is not accepted/published/replicated.

The disposable browser path permits deterministic `local_human` collections only;
it rejects model destinations and token/MCP callers. It does not create enrollment
or answering-service collection grants. Current read/curate/export permissions,
CSRF, read-only enforcement, bounded bodies and no-store responses apply. The
OpenAPI and operator documentation describe the experimental contracts.

The HTTP fixture covers preview without persistence, exact retry, altered review,
source freshness, metadata-only publication, CSRF, token/destination denial,
read-only mode and revoked local grants. This is not the full two-origin
automatic-worker rehearsal or a completed frontend workflow. Collection browsing,
Web/CLI/TUI/MCP, optional models and remaining rehearsal gates are still open.

Verification: 832 tests across 50 binaries pass with required disposable
PostgreSQL, serial workspace execution and `--all-targets --locked`; zero tests
failed or were ignored. Two initial existing CLI fixture tests inherited the
operator client-token environment variable; the passing run unsets that variable
only in the test process. The initial assertion log was redacted. Workspace
Clippy (`-D warnings`), formatting, boundaries and OpenAPI inventory pass.
Logs are `/tmp/f1-collections-workspace-clean.log` and
`/tmp/f1-collections-{clippy-final,openapi,boundaries}.log`. Web, Lean, lean CLI and
the full container/PDS isolation rehearsal were not rerun. The disposable
database was removed, with existing services and credentials unchanged. A partial
Memory checkpoint records `1eaf800`; the approved plan remains incomplete.

### Local collection inspection increment — 2026-09-11

Commit `6ecf89a` adds bounded origin-local collection listing and head-review
detail HTTP. Keyset listing uses collection UUIDs with an explicit next cursor,
not publication sequence order, and returns metadata only. Detail validates and
returns the exact stored snapshot, advertisement, reviewer and request ID. These
reads make no gateway request and neither change consent nor authorize answering.
Withdrawn collections remain inspectable; newer source versions are not silently
substituted. Stored `active` is not evidence of current eligibility or gateway
acceptance. Multi-page browsing is not a frozen snapshot.

The conservative disposable browser policy retains current read/curate/export,
explicit local-grant, token rejection, read-only denial and no-store behavior.
The HTTP fixture verifies bounded pagination, invalid cursor/limit rejection,
metadata-only listing, unknown collection handling, unchanged reviewed contents
after a source update, withdrawal visibility and denial after local grant removal.
This completes another component of Section 3, not its full user workflow.
Web/CLI/TUI/MCP, service-grant UI, optional model execution and full disposable
automatic-worker rehearsal remain open.

Verification for collection inspection: required-database serial workspace run
passes 832 tests across 50 binaries with zero failures/ignored tests. The inherited
client-token variable was unset only for the test process. Workspace Clippy
(`-D warnings`), formatting, boundaries and OpenAPI inventory pass. Logs are
`/tmp/f1-browse-{workspace,service,clippy,openapi,boundaries}.log`. Web, Lean, lean
CLI and the full container/PDS isolation rehearsal were not rerun. The disposable
database was removed; existing services and credentials were untouched. The Memory
plan-execution workflow saved a partial checkpoint at `6ecf89a`, not completion.

### Web requester increment — 2026-09-11

Commit `bd97633` adds the explicit expertise requester panel to the existing
Federation workspace. It makes no automatic expertise requests: discovery,
preparation, exact disclosure confirmation, submission and result reads are user
actions. Displayed consent includes destination, collection revision, question,
principal, digest, deterministic mode and local-human output policy. An uncertain
retry retains the complete review and request ID. Closing a review cannot cancel
delivery. Result text is untrusted plain text; citations and request binding are
inspectable. A refresh clears previously displayed answer bytes before fetching.

Project/space/principal/read-only/permission changes remount the panel, clear local
state and abort requests. No private question or answer is persisted to browser
storage. UTF-8 byte limits, disabled read-only/export-denied controls, synchronous
duplicate-click exclusion and filtered-page cursors have component coverage.
The React review checklist guided lifecycle cancellation, accessibility and safe
text rendering. Authorization remains server-owned, not inferred from UI controls.

Verification: all 81 Web tests across 23 files pass, including five new requester
tests; the production Web build passes with its existing large-chunk warning.
Logs: `/tmp/f1-web-tests.log`, `/tmp/f1-web-build.log`. These use mocked service
responses, not a real-browser/two-origin rehearsal. Rust/database, Lean, lean CLI
and container/PDS regressions were not rerun for this frontend-only increment.
No existing service or credentials changed. Collection management screens,
CLI/TUI/MCP, optional models and full rehearsal remain open; F1 is not complete.

### Web collection management increment — 2026-09-11

Commit `6c9fb99` adds origin-local collection browsing/detail, new/revised
collection preparation, full-byte activation confirmation and revision-bound
withdrawal confirmation. No browsing or publication happens automatically.
Preparation selects canonical IDs and an explicit requester allowlist, fixes the
mode to deterministic local-human output, and attaches no evidence by default.
An advanced JSON evidence selector accepts arrays only for selected canonical
IDs; backend validation and complete review still apply. It is not a graphical
source picker. Revisions deliberately do not carry forward omitted evidence.

Uncertain actions retain their exact request and confirmation payload for retry.
Success is labelled queued, not accepted or published. Closing review does not
undo committed work. Project/identity/permission changes clear state and abort
requests. The React checklist guided labelled controls, action serialization,
lifecycle cleanup and escaped text rendering. No private review is persisted in
browser storage; authorization remains server-owned.

Verification: all 85 Web tests across 24 files pass, including four new collection
workflow tests. The production build passes with its existing large-chunk warning.
Logs are `/tmp/f1-collections-ui-tests.log` and
`/tmp/f1-collections-ui-build.log`. These are mocked component tests, not the
real-service browser rehearsal. Rust/database, Lean, lean CLI and container/PDS
regressions were not rerun for this frontend-only increment. No existing service
or credentials changed. Service-grant controls, CLI/TUI/MCP, optional models and
the full disposable rehearsal remain open; F1 is not complete.

### Service identity collection grants — 2026-09-11

Commit `5f852d4` adds browser GET/POST collection grant inspection and reviewed
policy changes, plus Web controls after collection inspection. Current actor
read/AuthManage and local space grant checks run under transaction locks. Enabling
reuses the worker's active internal identity check, rejecting global/group
authority and missing current project read/export access. It does not create an
identity, credentials, worker configuration, enrollment or AnswerExpertise space
grants. The target is rechecked again by the worker at execution time.

Revocation now works for disabled target identities. State changes and sanitized
audit commit together. GET exposes only the selected target's stored boolean or
null, not execution eligibility or identity details. An uncertain grant response
clears the Web confirmation; inspection and a new deliberate review are required,
unlike exact-retry publication intents. The React checklist guided confirmation,
stale-policy handling and labelled inspection controls.

Tests cover non-admin denial, missing confirmation, unsuitable/missing-scope
targets, enablement, disabled-target denial and revocation, and scoped grant reads.
The new GET/POST pair initially exposed a unique-path policy registration conflict;
registering both methods together fixed it, with the focused HTTP test passing.
Web tests cover explicit confirmation and inspection after an uncertain write
without replaying old policy. Real-browser verification, CLI/TUI/MCP, optional
models and the full disposable rehearsal remain open. F1 is not complete.

Final grant verification: corrected required-database serial workspace run passes
832 tests across 50 binaries (zero failed/ignored). Web passes 86 tests across
24 files and the production build (existing chunk warning). Workspace Clippy
(`-D warnings`), formatting, boundaries and OpenAPI inventory pass. Logs:
`/tmp/f1-grants-workspace-final.log`, `/tmp/f1-grants-fixed-focused.log`,
`/tmp/f1-grants-{web,web-build,clippy-final,openapi,boundaries}.log`.
Lean/lean-CLI and full container/PDS/browser rehearsal were not rerun. The
disposable database was removed and a partial Memory checkpoint saved at
`5f852d4`. Existing services and credentials were unchanged.
