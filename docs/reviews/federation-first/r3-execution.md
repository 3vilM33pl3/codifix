# R3 execution and evidence ledger

## 2026-09-15 campaign-wide blinded review

- Added CLI export/apply of evidence-bound review packets and human ballots.
  Applying writes a new artifact without rerunning providers or changing measured
  usage/timing. Amendments require the exact prior ballot and retain linked
  history; dissent and model-only judgments cannot silently pass.
- Valid pending review no longer aborts remaining execution. Ordinary reports
  still fail closed until adjudication is resolved. Review rejects incomplete
  execution, missing usage, isolation failures and altered artifact bindings.
- Verification: 55 evaluator and 232 CLI tests passed; real campaign CLI and
  scoring executable tests passed with synthetic ballots, zero skips. Scoped
  Clippy, formatting and boundary checks passed. CI now requires the campaign
  review workflow test. This is not label approval or paid evaluation evidence.

## 2026-09-15 representative evidence-publication repository

- Added a second five-module development coding repository and three cumulative
  stages with ten private black-box probes. They cover snapshot substitution,
  review ownership, stale proposals, separate author/editor/publisher attribution,
  revocation, pre-send failure and lost-acknowledgement request replay.
- The reference passes all probes and the full isolated sequence; buggy/no-op
  controls fail each stage. All three required representative tests passed with
  zero skips. The starter's public smoke test passed with intentional unused-code
  warnings from the incomplete implementation. Labels remain draft.

## 2026-09-15 reusable dispatch results

- Added private, bounded result receipts separated from metadata-only budgets.
  Completed exact retries reuse results; altered inputs/tasks/campaigns, corrupt
  receipts and uncertain attempts fail closed without invoking providers again.
- A real child-process crash test verifies explicit recovery after durable result
  persistence but before settlement, preserving task/campaign accounting.
- Connected validated offline driver observations to the same persistence path
  and task-scoped reservations. Campaign-wide resume and all-provider mediation
  remain separate; paid execution and the single-call restriction stay disabled.
- Verification: 54 evaluator tests and 232 CLI tests passed, including saved
  observation recovery in the real fake-driver scenarios. Scoped Clippy,
  formatting and boundary checks passed. No paid calls or deployment changes.

## 2026-09-15 corpus fingerprints and combined regression evidence

- Added a bounded fingerprint of public benchmark table contents and column
  shapes from one normalized read-only snapshot. Reinforcement, embeddings,
  activity and newly added tables are included; only the four permitted
  operational journals are excluded. This is not a full schema/security audit.
- A real service-handler test queries two independently sealed PostgreSQL clones
  using fake embeddings and actual full-Memory retrieval. Seeded reinforcement
  remains effective, another project's rows stay out, scores match across clones,
  and complete benchmark fingerprints remain unchanged before/after queries.
- Fingerprint tests cover row-order/timezone independence, changed data/columns,
  prepared-template reset and restricted readers. Test data and PostgreSQL were
  disposable; the running dev service/database was not modified by these tests.
- Combined verification: 874 required-database workspace Rust tests passed with
  two test threads; workspace all-target Clippy, fmt and boundaries passed.
  Eleven required live container/Rust benchmark tests and two executable
  retriever/adjudication tests passed without skips. Web: 90 tests and build
  passed; Lean build/test and lean CLI check passed with existing warnings.
- This verifies components and a real retrieval/isolation integration, not a
  complete campaign. Full campaign orchestration, complete representative suite,
  all-provider mediation/cached results/reconciliation, review collection,
  approvals and official measured evidence remain open. No paid/provider or
  customer deployment operations were performed; R3 remains incomplete.

## 2026-09-15 per-task provider budget accounting

- Durable request journals now optionally enforce task-level ceilings in
  addition to campaign totals. A three-step sequence can retain one task budget;
  unscoped reservations cannot bypass it. Refunds and uncertainty affect both
  ledgers, and restart retains task history and rejects changed limits.
- Shared locked-price calculation rounds charges up and rejects excessive token
  requests/overflow; the existing dispatcher now uses that calculation.
- Forty-nine evaluator tests and 232 CLI tests passed, including task request,
  token and cost caps, reopen, uncertainty and no-dispatch-on-limit checks.
- These are multi-call mediator foundations, not complete provider egress,
  tokenization, result caching or reconciliation. Paid execution and the current
  single-request driver restriction remain unchanged; R3 is not complete.

## 2026-09-15 evidence-bound knowledge adjudication

- Added semantic rubric contracts, bounded blinded review packets, exact-source
  citation validation and per-criterion human ballots. Model diagnoses cannot
  pass a case; absent/conflicting judgments stay unresolved. Invalid citations
  remain measured failures even with passing human judgments.
- Reports now require independent review for official knowledge observations.
  Campaign/case/execution, answer, locked rubric/source and computed-score
  mismatches fail closed. Supplied offline reviews receive the same checks;
  legacy smoke artifacts remain readable without becoming official evidence.
- Added the trusted-host `r3_score` preparation/scoring utility, required CI
  rehearsal and four draft machine-readable supplier-handover rubrics. Human
  identity is an operator responsibility, not authenticated by a JSON field.
- Verification: 46 evaluator tests, 232 CLI tests, the real scoring utility
  rehearsal, scoped all-target Clippy, formatting and boundaries passed.
- Full campaign review collection/reconciliation, complete representative suite,
  provider mediation and isolated campaign integration remain open. No labels
  were approved, providers called, or deployment/database settings changed.

## 2026-09-15 representative importer benchmark

- Added the first five-module development Rust repository, its public contract
  and smoke tests, three cumulative private stages and reference implementation.
- Ten black-box probes cover partial commit, arithmetic overflow, gaps, identity
  conflicts, authority ordering, lost acknowledgements and restart/catch-up.
  Reference execution passes; buggy and no-op controls fail each stage.
- Five required live Rust/container tests passed with zero skips; the standalone
  public repository's two tests passed offline. CI requires the new benchmark.
- Labels are draft. The store is an in-process durability test double, not a
  disk-recovery experiment. This is one development sequence, not the complete
  representative suite or a campaign/product-quality result. R3 remains open.

## 2026-09-15 dispatch journal integration

- Existing offline campaign dispatch now writes `dispatch-budget.jsonl` with
  a durable reservation before driver launch. Settlement requires validated
  usage, locked inputs, isolation metadata and the preparation receipt.
- Failed, timed-out, mutated, missing-usage and unsupported multi-request
  dispatches retain unresolved reservations. Exact reported total tokens are
  accounted for, including cached input outside the provider's input count.
- The legacy ledger retains conservative maximum dispatch allowances. This is
  still one reservation per driver, not mediation of arbitrary internal calls;
  the single-request restriction and unconditional paid-run block remain.
- All 232 CLI tests passed, including eleven real fake-driver scenarios with
  journal inspection/reopening. CLI all-target Clippy, fmt and boundaries passed.
  No providers, database/deployment changes or new benchmark approvals were used.
- Full provider mediation, complete campaign driver and representative suite
  remain unfinished. R3 is not complete.

## 2026-09-15 durable per-request budget boundary

- Added `RequestBudget`: exclusive owner, private append-only metadata journal,
  fsynced pre-dispatch reservations, separately bounded input/output usage,
  request/token/spend caps and conservative handling of uncertain outcomes.
- Confirmed receipts refund unused tokens/cost only. Reused request IDs,
  altered campaign limits, concurrent owners and partial journals fail closed.
  Process termination releases the OS lock but retains the uncertain charge;
  a new dispatcher cannot blindly retry or start another call.
- Thirty-eight evaluator tests passed, including fake-provider budget gates and
  a child-process termination/recovery test. Scoped all-target Clippy, formatting
  and boundaries passed. No providers, databases or deployments were used.
- This is a dispatcher component, not yet integration with `campaign run`.
  Existing single-request and paid-run restrictions remain. Task-level limits,
  price/parameter validation, all-provider mediation, persisted results and
  reconciliation still need driver integration. R3 remains incomplete.

## 2026-09-15 combined verification and first representative source pack

- Materialized the first development knowledge family: supplier handover has
  five public documents and a separate draft four-question review rubric.
  Labels remain draft; this is not the complete suite or a campaign input yet.
- A frozen test template now rejects any remaining preparation connection
  without killing it; its required-database regression passed.
- Initial unbounded workspace testing exhausted the disposable PostgreSQL
  connection limit (one service test failed with `53300`). The rerun with
  `--test-threads=2` passed: 854 tests, zero failures, with
  `MEMORY_LAYER_TEST_REQUIRE_DB=1`. Final test-support verification separately
  passed three unit tests and one migration test after the active-writer guard.
- Workspace all-target Clippy, Rust formatting and boundary checks passed.
  Web: 90 tests and build passed. Lean build/test passed with existing lint
  warnings. The no-default-features CLI check passed with existing warnings.
- Nine live container/sequence tests and the external retriever executable
  rehearsal passed. No paid calls, customer data, deployed-service changes or
  label approval. Live PDS activation/rehearsal was not performed in this run.
- R3 remains incomplete: complete campaign orchestration, source/label/config
  fingerprint integration, per-request provider mediation, complete representative
  datasets, semantic adjudication, approval and official measurement remain open.

## 2026-09-15 external baseline adapter

- Added the existing-protocol `r3_retriever` executable for prepared handover,
  BM25 and explicit-vector context selection with shared limits and citations.
- The adapter denies project/condition mismatches and hidden answer facts,
  ignores host fixture paths, and emits sanitized failures. Vector mode requires
  an explicit query embedding rather than inventing vectors or falling back.
- Thirty-two evaluator tests, scoped all-target Clippy and a real executable
  stdin/stdout rehearsal passed. The rehearsal tests all three comparators and
  scope, label-leak, missing-vector, oversize and budget rejection.
- This is context retrieval only. Common answer generation, provider-mediated
  query embeddings and complete campaign integration remain open. No quality,
  parity or R3-completion claim follows from these tests.

## 2026-09-15 sealed database reset integration

- Evaluation startup now rejects public-schema sequence USAGE/UPDATE and
  replication roles, closing writes through `SELECT nextval(...)`.
- Test-support databases can be frozen with connections disabled and cloned
  without migrations or reseeding. Read-only clones receive distinct logins,
  benchmark SELECT and only the existing permitted operational DML.
- Real PostgreSQL tests verify exact seeded reinforcement restoration, denied
  writes, denied cross-clone connections and acceptance by the service seal.
- Required-database verification: 127 service tests and two test-support unit
  tests passed; the migration integration test also passed separately. Scoped
  all-target Clippy, formatting and boundary checks passed after fixing test
  assertion lint errors. Existing federation service regressions were included.
- Database testing used an isolated pgvector container, never the dev database.
  This is a test-support primitive, not complete campaign snapshot orchestration,
  corpus fingerprinting or crash reconciliation. No R3 completion claim.

## 2026-09-15 real Rust grading integration

- Added a minimal digest-pinned Rust/Node image without checkout or assistant.
- Added bounded executable Rust scratch and independent host-side exact-output
  grading. Hidden expectations never enter submitted programs; a reference
  compile/run distinguishes unavailable tooling from incorrect submissions.
- Connected real Rust grading to three-step source-only sequence execution.
  A failed middle step remains a whole-sequence failure after later repair.
- Nine required live isolation tests passed without skips; boundaries and diff
  checks passed. CI now requires the Rust tests separately. Remote CI and broad
  Rust/database/Web regression suites were not rerun for this Node-only slice.
- This is dependency-free black-box Rust, not arbitrary Cargo tests. Full
  campaign/service snapshot integration, representative benchmarks and approvals
  remain open. R3 is not complete and no paid execution is enabled.

## 2026-09-15 source-only coding handoff

- Container invocations clone reviewed inputs before asynchronous staging.
- Explicit bounded export paths prevent build products and grader files from
  crossing the submission boundary. Sequence agents export declared source files;
  graders export none. Missing sources stay missing for negative grading.
- Six required live container/sequence tests passed without skips, including
  asynchronous input mutation, oversized unselected build output, selected
  directory/symlink/hard-link rejection and empty grader export.
- `git diff --check` and `scripts/check-boundaries.sh` passed. This Node-only
  increment did not rerun Rust/database/Web tests or an official campaign.
- R3 remains incomplete: real Rust build/grading integration, complete campaign
  driver, sealed/reset service datasets, representative benchmarks and approvals
  remain open. No paid calls, label approval or existing deployment changes.

## Approved benchmark refinement

The small original cases are now smoke-only, not proposed official evidence.
The manifest defaults missing `smoke_only` to true so old draft packs also fail
official authorization. New generator output explicitly sets it true. Merely
approving labels cannot promote a smoke pack to product evidence.

Replacement design: `evals/r3/representative-benchmark.md`. It specifies multi-
document incident reasoning and multi-file repository maintenance, with positive
and negative graders, distributed provenance and three-step regression tests.
These are draft designs; complete replacement fixtures, labels and execution
remain unfinished. Original counts and approval/spend gates are unchanged.
Revised plan checkpoint: `d1934e30-d011-4dac-b5be-9055a832578a`.

Status: implementation in progress; label approval and paid runs not authorized.
Approved plan checkpoint: `r3-isolated-evaluation`, plan memory
`201e0e95-7691-4abc-924a-0cbe55ccc53b` (2026-09-10).

## Delivery gates

- [x] Checkpoint approved contract and inspect existing evaluation machinery.
- [x] Operator-only frozen semantic clock and no-learning service profile.
- [ ] Sealed/reset disposable datasets and restricted agent containers.
- [ ] Equal-information document, lexical, vector and full-Memory baselines.
- [ ] Synthetic development/held-out knowledge and coding review pack.
- [ ] Campaign plan/run/report, provenance, limits and fail-closed gates.
- [ ] Offline fault/isolation/statistical tests and integration evidence.
- [ ] Olivier approves labels; freeze suite only after approval.
- [ ] Approve and run paid development calibration.
- [ ] Separately approve and run official campaign; publish measured outcome.

## Contract

Knowledge: 40 development questions in 10 scenarios; 100 held-out questions in
25 scenarios. Coding: 4 development and 12 held-out three-step Rust maintenance
sequences. Entire scenario/repository families are disjoint. Three official
repeats; sequence children and repeats are not independent samples.

Conditions: maintained handover, simple lexical, simple vector, full Memory,
and diagnostic no-memory. Same facts, model, reasoning and task budgets. No
retrieval tuning or R4/R5/R6/F expansion. Synthetic outcomes are not customer ROI.

Defaults: knowledge >=90%, coding >=80%; >=5 percentage-point improvement over
each informed baseline with positive simultaneous 95% cluster-bootstrap lower
bounds per track. Zero designated critical failures. Cost and median latency
<=2x a development-selected, pre-frozen baseline. Missing evidence invalidates a
campaign. Valid negative/inconclusive results do not pass product quality.

## Initial coverage inventory

| Component | Existing behavior | R3 obligation |
| --- | --- | --- |
| `mem-search` recency | Rust wall clock | One semantic timestamp per query |
| Reinforcement ranking | SQL `now()` decay | Same timestamp as Rust; retain seeded scores |
| Query hooks | Reinforcement access and timeline writes | Suppress measurement-induced learning/context |
| Service workers | Background/scheduler/federation lifecycle | Disable during measurement, including reload |
| Existing Docker eval stack | Shared database, host CLI bootstrap | Dedicated resettable, sealed and restricted harness |
| Existing canary gate | Draft labels, mainly no-memory comparator | Preserve compatibility; not R3 evidence |
| Eval comparison | R2 parent-level costs | Preserve; cluster repeats/scenarios for inference |
| Agent sequence runner | Shared workspace inside sequence | Retain only within sequence; hidden independent grader |
| Artifacts | Optional configuration metadata | Required hashed campaign provenance; fail closed |

## Approval and safety boundaries

The offline harness must not load developer provider credentials or contact paid
providers. Snapshot provisioning uses disposable resources only. Operational
journals may change, but must not enter benchmark context; sealed knowledge and
reinforcement state must not change. Real clocks remain in authentication,
leases and latency measurement. No user query parameter can change semantic time.

Implementation completion, valid experiment completion and product-quality pass
are distinct. No official result may be claimed from fake/offline runs or draft
labels. No existing deployment is changed by this work.

## 2026-09-10 implementation increment

The service/search clock and no-learning changes are implemented and tested with
disposable PostgreSQL. Measurement startup also refuses privileged/unsealed DB
roles. Source/reinforcement DML is denied; allowed operational journals remain
outside benchmark content. No existing deployment was restarted or migrated.

Pure baseline comparators, campaign manifests/approval contracts, conservative
reservations, clustered reports and CLI plan/report interfaces are implemented.
An offline driver interface is present but **the full driver is not implemented**;
paid dispatch fails before execution. These are not complete R3 end-to-end results.

The draft review generator prepares all 156 tasks. All 48 coding reference
graders pass. The draft knowledge scorer still requires semantic/citation
adjudication; coding difficulty is explicitly limited to small functions.
See `evals/r3/README.md` for the remaining integration list. R3 stays unchecked.

### Local verification

- Required-database `cargo test --workspace --all-targets --locked -- --test-threads=8`:
  passed against an owned, disposable PostgreSQL/pgvector container. This includes
  existing federation regressions; no live PDS or customer dataset was used.
- Workspace Clippy (`--all-targets --locked -- -D warnings`): passed.
- `cargo fmt --check` and `scripts/check-boundaries.sh`: passed.
- `cargo build -p mem-cli --no-default-features --locked`: passed with existing
  lean-feature dead-code warnings; full-feature development binary rebuilt after.
- `mem-eval`: 29 tests passed, including bootstrap, accounting, malformed evidence,
  approval hashes, comparator bounds and schedule reproducibility.
- CLI campaign tests: shell-consent and paid-dispatch denial happen before I/O;
  receipt overwrite protection passed. Public command-help metadata is covered.
- Draft reference graders: 48/48 passed. Campaign plan validates 660 development
  and 1680 held-out work items; labels/model/budget remain unresolved.
- Completion check (`finish-execution --dry-run`): correctly reports unfinished
  plan items; no completion memory or plan archival occurred.

Draft material is reproducible from `evals/r3/build-review-pack.mjs`;
`target/memory-evals/r3-review-draft-2/REVIEW.md` is the local review copy.
Manifest hash: `c5a35ffbe2cf4f40700c0bc9cdf74ca7e01e63c80f004ba53c8a2496cc914a3a`.
The first draft was superseded after fixing the legacy suite tag to `eval_type`;
use draft 2, not draft 1. CI now builds and checks fresh draft material without
providers; that workflow edit has not yet run on remote CI.

Next execution slice is the actual resettable service/agent/grader driver and
per-request provider budget mediation. These are engineering work still to do,
not work blocked on spending approval. No complete offline campaign or official
benchmark has been executed, and no quality or ROI claim is supported yet.

Progress capture (without curation/enrichment):
`8e4311a9-6144-4e0a-b70c-53a565ab0ff4`. The R3 plan remains active; this is not
a completion record. The owned disposable PostgreSQL container was removed
after verification; its synthetic test data can be recreated, not recovered.
The user's running dev service and generated draft review directories were left
untouched. Final workspace Clippy and boundary checks passed after moving CLI
tests into a dedicated module.

## 2026-09-12 runner-boundary increment

Resumed the existing approved R3 plan through the dev Memory workflow. No labels,
models, spending approvals, service configuration or existing data were changed.

The CLI runner now reloads and validates the locked manifest and its assets both
before and after each successful driver process, including the final work item.
Changing an asset or rewriting the manifest cannot be hidden by completing the
last task. The original expected manifest identity remains authoritative.

Per-task evidence is checked before another task starts: model identity, result
identity, non-skipped status, recorded duration, known/consistent input and output
usage within individual bounds, resolved rubric, no uncertain charge, valid
snapshot/clock controls and unchanged configuration/driver/build. A valid wrong
answer remains measurable rather than being treated as invalid execution.
Reservations and the driver's raw observation/log files remain available when
execution stops. This is not a container isolation mechanism or a provider proxy.

Verification: all 232 CLI tests pass serially with the inherited developer token
unset; all 30 evaluator tests pass. The runner tests include actual local shell
subprocesses changing a source, corrupting the manifest and changing its seed
while retaining valid JSON, plus invalid evidence and paid-dispatch denial.
The fixture uses new temporary directories and synthetic files only. CLI/evaluator
all-target Clippy with warnings denied, formatting and boundary checks pass.
Logs: `/tmp/r3-runner-cli-all.log`, `/tmp/r3-runner-eval-tests.log`,
`/tmp/r3-runner-clippy-final.log`, `/tmp/r3-runner-boundaries-final.log`.

No database, Web, federation/PDS or full workspace regression rerun is claimed for
this CLI-only change. No complete offline campaign was run. The remaining
container/service/grader driver, representative fixtures, per-request provider
mediation and official approval gates remain open. **R3 is still incomplete.**

## 2026-09-12 offline container-boundary increment

Added `evals/r3/isolation/container.mjs` and its read-only guest supervisor as a
component for the existing campaign driver. It accepts explicit text files and
an immutable reviewed local image ID, not a host workspace or ambient credentials.
Each invocation uses non-root execution, no network, read-only inputs/root,
bounded tmpfs workspaces, process/memory/output limits and exact-resource cleanup.
Returned submissions are untrusted and bounded; symlinks, hard links, special
files, unsafe names and invalid encodings are rejected. Explicit file transfer
can preserve one sequence's state without retaining it for independent cases.

All three Node tests pass with the container gate explicitly required (no skips).
The live case runs the cached immutable PDS fixture image's Node executable only,
never a PDS service or model. It checks a real hidden host file remains invisible,
minimal command environment, absent Docker socket, read-only inputs/root,
loopback-only network, selected input access, sequence transfer, independent reset,
symlink rejection, output flooding and deadline cleanup. Every owned test container
and staging directory was removed. No existing service or database changed.

The offline-eval CI job now explicitly runs the same required container gate;
remote CI execution is not claimed. Local evidence:
`/tmp/r3-container-final-tests2.log` and `/tmp/r3-container-boundaries.log`.
Node syntax, diff whitespace and boundary checks pass. No Rust, database, Web or
federation regression rerun is claimed for this standalone Node component.

Limitations: same-kernel Docker isolation trusts host administrators and the
reviewed image. Guest results are not grading verdicts or security attestations.
Host/daemon crash reconciliation, real Rust build scratch/export handling,
PostgreSQL snapshot reset, full retrieval adapters, hidden grader execution and
campaign-driver integration remain unfinished. No delivery checkbox for complete
isolation is closed. See [component contract](../../../evals/r3/isolation/README.md).
**R3 remains incomplete; labels and paid execution remain unapproved.**

## 2026-09-12 independent sequence-grading increment

Added `evals/r3/isolation/sequence.mjs`: exactly three agent/private-grader pairs,
with complete contract validation before dispatch and a frozen copy of that
contract throughout execution. The agent sees only public inputs and allowlisted
prior submission files. The separate grader receives selected submissions and
hidden checks in disjoint namespaces. Neither grader output nor grader-modified
files can contaminate the next step. Deleted source files remain deleted until
explicitly recreated; later repair cannot erase an earlier failed step.

All five Node component tests pass with real containers explicitly required and
zero skips. The sequence fixture exercises successful work, a confident no-op
that fails hidden checks, injected fake grader/feedback files, changed public
context, caller mutation after dispatch, private feedback exclusion, sequence
reset and deletion/recovery. These are trusted Node graders over inert synthetic
JSON, not representative Rust tasks or proof against malicious compiled-code
attacks on graders. Whole-sequence timing and per-step diagnostics stay distinct;
all results explicitly remain offline smoke evidence.

Verification log: `/tmp/r3-sequence-final-tests.log`. Node syntax, whitespace and
boundary checks pass (`/tmp/r3-sequence-boundaries.log`). CI now requires all
container and sequence tests; remote CI is not claimed. All owned containers and
staging data were removed. No existing service, database or provider was used by
the fixture. No Rust/database/Web/full-workspace regression rerun is claimed.

The actual campaign driver integration, sealed service snapshots, retrieval
adapters, Rust build/grader execution, crash reconciliation and provider mediation
remain open. Representative fixtures and label/spend approvals remain pending.
**R3 is not complete.**
