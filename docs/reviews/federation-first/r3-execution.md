# R3 execution and evidence ledger

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
