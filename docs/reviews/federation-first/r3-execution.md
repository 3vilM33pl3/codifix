# R3 execution and evidence ledger

## 2026-09-16 diagnostic repair and bounded retry authorization

- Olivier requested diagnosis and another attempt. Added closed-vocabulary
  provider diagnostics: transport/HTTP, receipt status/model/usage/output and
  deadline/cancellation categories. Failed reserved calls write private,
  exclusively created, manifest/payload-bound metadata files. No raw provider
  text, prompts, answers, credentials or stack traces are persisted there.
  Uncertainty and replay fences are unchanged; diagnostics never refund usage.
- Validation: 22 focused transport/receipt/mediator tests, 14 provider/profile/
  call tests (one separate integration skipped in that focused invocation),
  three runtime/approval packaging tests and the real disposable-database fake
  HTTP-failure test passed. Full 75-task fake-provider launcher rehearsal passed
  with valid execution evidence and 60 pending human reviews; fake output is
  not quality evidence. Formatting and boundary checks passed. Logs:
  `/tmp/r3-diagnostic-focused.log`, `/tmp/r3-diagnostic-packaging-tests.log`,
  `/tmp/r3-diagnostic-http-e2e.log`, `/tmp/r3-diagnostic-launcher-tests.log`.
  Full fake evidence: `/data/tmp/r3-paid-XcCGOg/pack/`.
- Replacement manifest at `/data/tmp/r3-diagnostic-retry-20260916/campaign.json`:
  `d424e71ad19473266d7c3305574a9b73491bcf67b55b2fb1fcab719aa53fe8c9`.
  The 29 benchmark assets, model/settings, cases, semantic clock and per-task
  budgets are unchanged. The existing two-family label decision is carried
  forward; the user's explicit retry request is bound to this instrumented
  replacement, not to a new benchmark or model.
- Remaining ceilings: **US$9.68, 423 calls, 9,590,064 tokens**. This deducts both
  attempts' 76 settled calls / 5,840 tokens / 166 micro-USD and reserves the full
  uncertain model-call allowance of one call / 404,096 tokens / 318,432 micro-USD.
  Combined maximum is US$9.998598, not an increase to the original US$10 cap.
  Original receipts and nested outer task reservations remain untouched. Exact
  approval preflight and runtime verification passed; no held-out work or
  further automatic retries are authorized. Run outcome is recorded separately.

## 2026-09-16 corrected paid attempt: unresolved first model operation

- Preparation and the corrected embedding boundary passed. The first coding
  task reserved its Responses operation but produced no settled model receipt;
  the launcher stopped with zero accepted observations. No paid retry followed.
  See the [corrected-run report](../../../evals/r3/development-corrected-20260916.md).
- Corrected settled usage is 38 embedding calls, 2,920 tokens and 83 micro-USD.
  One model-call allowance of US$0.318432 remains unresolved. Combined settled
  usage from both attempts is US$0.000166, plus that uncertainty. Parent task
  reservations remain intact and are not additional charges.
- Existing sanitized errors do not identify HTTP status or failure stage; no
  exact cause can be claimed from retained evidence. Next diagnostics must be
  offline-tested and metadata-only. The frozen campaign cannot be resumed or
  modified; another live attempt needs reconciled usage and separate approval.
- The pinned report rejects the incomplete artifact. Its observation-only
  uncertainty/cost counters do not include the failed task's journal; the
  private per-call journal is authoritative for partial usage and uncertainty.
  The owned temporary database is discarded after process exit; all artifacts
  and fences remain. Existing services and original evidence are untouched.

## 2026-09-16 corrected development run approved and launched

- Olivier explicitly replied “Approve corrected run” for manifest
  `169c57de207ca8ba46cf13443b9fccbfb7da65f51d1cd75c3eac39d6c45c9c9c`.
  Recorded the development receipt at `2026-09-16T13:48:09Z`, with remaining
  ceilings of US$9.99, 462 requests and 9,997,080 tokens. The same two reviewed
  families, models, 75 tasks and 300-second task timeout remain frozen. No
  automatic retry, top-up or held-out execution is authorized.
- Dev Memory checkpoint, pinned runtime verification, image checks and the real
  approval preflight passed. Started the pinned launcher once against a fresh
  loopback-only disposable PostgreSQL instance, using the already approved
  dedicated secret reference. Existing services and original evidence are
  unchanged. Corrected evidence lives at
  `/data/tmp/r3-calibration-corrected-20260916/`; this launch record is not a
  completed campaign or a quality claim.

## 2026-09-16 corrected package prepared, not executed

- Rebuilt and pinned the runtime after the numeric parser fix. The replacement at
  `/data/tmp/r3-calibration-corrected-20260916/campaign.json` has final identity
  `169c57de207ca8ba46cf13443b9fccbfb7da65f51d1cd75c3eac39d6c45c9c9c`.
- Verified every benchmark, label, fixture, suite and family-metadata asset and
  model setting against the original. Only the runtime descriptor and absolute
  driver path changed among hashed assets. Recorded the existing scoped label
  approval with a zero spending allowance and explicit provenance; no new human
  approval is inferred. The real preflight rejects missing Development approval.
- Set conservative replacement ceilings of US$9.99, 462 calls and 9,997,080 tokens,
  accounting for the original 83 micro-USD, 38 calls and 2,920 tokens. The original
  attempt, task reservation and receipts are untouched. The corrected pack has
  no spending receipt, settings or execution state; do not launch without new
  explicit hash-bound authority.
- Post-fix validation: **905 required-database workspace tests** pass across
  54 targets with four test threads; the one existing M2 machine-local-path
  documentation failure remains explicitly excluded. An earlier unrestricted
  concurrency run exhausted the disposable database's default connection limit;
  its failure was retained, then the complete suite rerun with bounded concurrency.
  Logs: `/tmp/r3-float-workspace.log` and `/tmp/r3-float-workspace-bounded.log`.
- All 75 evaluator tests, three cross-language retriever tests, formatting,
  boundaries and evaluator Clippy pass. The lean CLI build passes with eight
  existing warnings. Full-workspace Clippy was interrupted when it started a
  second cold native-library build; it is **not claimed as passed** for this
  increment. Logs: `/tmp/r3-float-clippy.log`, `/tmp/r3-float-workspace-clippy.log`,
  `/tmp/r3-float-lean-build.log`. Both original and corrected runtime pins verify;
  the corrected packaged retriever accepts the retained decimal-vector diagnosis
  without any provider call.

## 2026-09-16 first paid attempt: local numeric-integrity failure

- The authorized calibration completed preparation and stopped at its first
  task, before any answer-model request. Thirty-eight embedding calls settled:
  2,920 input tokens and 83 micro-USD under the frozen rounded-up price accounting.
  Zero task observations were accepted. The report correctly rejects the incomplete
  artifact. See the [development report](../../../evals/r3/development-20260916.md).
- Kept the original `40c6f8dd…f6b9d3c` pack, all journals and its outer task
  reservation unchanged. No paid retry was attempted; no new credential created.
- Offline reproduction isolated `serde_json` binary64 parsing: nine components
  of a retained 1,536-dimensional vector changed without `float_roundtrip`,
  breaking the exact embedding digest. The exact-binary fake control passed.
  Enabled exact parsing in `mem-eval` and added Rust/cross-language regressions;
  the old binary fails the new regression and the corrected retriever passes,
  including retained live vectors and tamper rejection. The complete fake
  rehearsal with varied decimal vectors passed all 75 tasks and 76 controller
  receipts. Sixty knowledge reviews remain pending, as expected for fake output;
  this is not a paid retry or quality evidence. Retained at
  `/data/tmp/r3-paid-hzIGPn/pack/run/`, log `/tmp/r3-decimal-paid-rehearsal.log`.
- No conclusion about model quality is possible. A corrected campaign needs a
  new hash-bound approval, accounting for usage already settled by this attempt.

## 2026-09-16 development spending approval and launch

- Olivier explicitly replied “Approve the $10 development run” to the final
  identity `40c6f8dd554cb2f6a52575a87133a31edce8003c25a99b969e14dcb48f6b9d3c`.
  Recorded a development receipt at `2026-09-16T12:23:59Z` with a US$10 ceiling,
  in addition to the existing separate labels receipt. This authorizes only the
  75-task two-family development calibration, with preparation included, 500
  provider requests and ten million tokens; no retries, top-ups or held-out run.
- Dev Memory checkpoint succeeded. The frozen runtime and both image identities
  were reverified, no prior execution existed, and the pinned CLI approval
  preflight passed for exactly that manifest and limits.
- Launched the pinned runtime against a fresh disposable PostgreSQL instance,
  using the previously approved version-pinned OpenBao evaluation key reference.
  No existing service, data, credential or frozen benchmark asset was changed.
  Private artifacts remain at `/data/tmp/r3-calibration-20260916/`; execution
  outcomes must be read separately from this authorization record.

## 2026-09-16 scoped development-label approval

- Olivier replied “Approved” to the request to approve artifact-origin and
  configuration-rollout labels before a separate exact spending request. Recorded
  that scope through the dev Memory checkpoint before editing; neither other
  benchmark families nor spending are inferred as approved.
- Verified the existing draft identity and all pinned runtime inputs, then changed
  only the calibration manifest's `label_status` to `reviewed`. Final SHA-256:
  `40c6f8dd554cb2f6a52575a87133a31edce8003c25a99b969e14dcb48f6b9d3c` at
  `/data/tmp/r3-calibration-20260916/campaign.json`. The sources, private labels,
  tests, model settings, limits and runtime bytes are unchanged.
- Recorded a mode-0600 `private/labels-approval.json` for Olivier, purpose
  `labels`, timestamp `2026-09-16T11:44:23Z`, maximum spend zero. The pinned CLI
  validates the 75-task plan. Approval preflight fails as intended with
  `missing current Development approval`, proving the labels-only receipt does
  not enable paid dispatch. No secret access or provider calls occurred.
- Next: obtain explicit approval for this identity, the pinned answer/embedding
  models and maximum US$10 / 500 calls / ten million tokens, including preparation.
  Full benchmark label approval and the official campaign remain open.

## 2026-09-16 approved paid launcher

- Resumed and checkpointed the existing approved plan through dev Memory before
  editing. Added an explicit `--approved-runtime` path to the Rust campaign runner;
  ordinary arbitrary-driver paid dispatch and automatic paid resume stay blocked.
- The launcher verifies exact label/spend approvals, locks the module closure,
  executable copies and container identities, then connects the credential owner,
  durable controller, disposable services and existing runner. The driver and
  agents receive neither the key nor private launcher settings. Packs are bound
  to their absolute location; a subsequent Cargo build cannot replace their binaries.
- Complete fake-provider execution passed all **75 tasks** with **76 completed
  controller receipts**, including preparation. All snapshots stayed unchanged;
  **60 knowledge reviews remain pending**, so execution validity is not harness
  or quality approval. Evidence: `/data/tmp/r3-paid-T5omqL/pack/run/`, log
  `/tmp/r3-paid-launcher-tests.log`. The artifact deliberately exercises
  `offline: false` accounting with mocked HTTP and a synthetic key; it is **not
  a real paid campaign or measured product-quality result**.
- Abort, frozen-launcher SIGTERM and SIGKILL rehearsals preserve uncertain
  preparation and fence relaunch before another credential read. Recovery is
  explicitly stop-and-reconcile, not automatic reconstruction of lost database
  state. Logs: `/tmp/r3-paid-launcher-interrupt.log` and
  `/tmp/r3-paid-launcher-kill.log`.
- `prepare-paid-calibration.mjs` builds a new draft development-only pack from
  artifact-origin and configuration-rollout (75 executions). It pins proposed
  v2 settings and the runtime but creates no approval and reads no key. Its
  no-overwrite/no-approval regression passed in
  `/tmp/r3-paid-calibration-test.log`. The [runbook](../../../evals/r3/paid-launcher.md)
  documents final-hash approval and private settings.
- Prepared the actual draft at `/data/tmp/r3-calibration-20260916/campaign.json`,
  SHA-256 `943ce776cd89a00c4b183cba22ed82b36fda8514242b299690bbf59bd4f6af48`.
  This is a **draft identity, not a spend-approval target**: genuine label approval
  and the resulting reviewed manifest produce a different identity. Runtime
  packaging copied the Bao executable, without invoking it. Root disk pressure
  required regenerating on the data disk; the moved earlier draft under
  `/data/tmp/r3-calibration-superseded-20260916` is location-invalid and not usable.
- Required-database workspace validation passed **904 tests** across 54 targets
  with the one already failing documentation test excluded. Its separate rerun
  still fails on the machine-local path in `m2-model/refresh-guide.md`, outside
  this change. Seven approval/provider-owner regressions passed without skips.
  Workspace Clippy (`-D warnings`), formatting, boundaries, changed-document link
  checks and the lean CLI build pass (eight existing lean-feature warnings).
  Logs: `/tmp/r3-launcher-workspace.log`, `/tmp/r3-launcher-doc-links.log`,
  `/tmp/r3-launcher-owner-regression.log`, `/tmp/r3-launcher-clippy.log`,
  `/tmp/r3-launcher-boundaries.log`, `/tmp/r3-launcher-lean-build.log`.
- **R3 remains open:** actual label approval, exact model/spend approval, real
  provider compatibility/calibration and the separately approved official campaign
  remain outstanding. No real key read, paid request, credential rotation or
  existing deployment change occurred in this increment.

## 2026-09-16 durable controller and cancellation recovery

- Dev Memory is reachable again. Resumed the approved R3 plan and checkpointed
  before editing; no credential changes, paid requests or deployment changes.
- Extracted the shared host IPC controller into `campaign-dispatch.mjs` and wired
  it into the existing rehearsal. Attempts are synced before execution and
  receipts before reply. Exact duplicate requests share work/results; altered,
  concurrent and out-of-order requests are rejected. The exclusive journal
  directory fences a second owner, including after SIGKILL. A failed owner
  cannot be revived by restoring changed inputs. Locked work-list bounds and
  response locations are validated independently of the task executor.
- Added read-only `inspect-dispatch.mjs`: metadata only, no provider invocation,
  answers or automatic repair. Partial/corrupt receipts fail closed. An uncertain
  attempt yields exit 2; successful inspection never enables execution. Keep
  this journal with the Rust outer/per-call budget journals and artifacts.
- Approved provider/session helpers now accept controller cancellation. An
  in-flight fake HTTP request aborts, its maximum charge stays reserved, and
  subsequent calls/bindings are denied. No key or provider error body is exposed.
- Nine controller fault tests and seven approved-owner/session tests pass without
  skips. Tests cover duplicate/lost responses, receipt-write failure, cancellation,
  changed inputs, inspection sanitization and SIGKILL before claim, during effect
  and after receipt. The owner suite includes actual disposable PostgreSQL/service
  preparation and a complete isolated coding sequence, with fake credentials and
  mocked provider responses. Logs: `/tmp/r3-dispatch-faults-final.log` and
  `/tmp/r3-dispatch-owner.log`.
- Focused regressions pass: 53 evaluator campaign tests, 10 CLI campaign tests and
  22 provider/review tests, without skips. Workspace Clippy, formatting and
  boundaries pass. Logs: `/tmp/r3-dispatch-eval-tests.log`,
  `/tmp/r3-dispatch-cli-tests.log`, `/tmp/r3-dispatch-regressions-final.log`,
  `/tmp/r3-dispatch-clippy.log` and `/tmp/r3-dispatch-boundaries.log`.
  The lean CLI build also passes with eight existing dead-code warnings:
  `/tmp/r3-dispatch-lean-build.log`.
- Required-database workspace tests, including federation regressions, pass with
  the known unrelated M2 `markdown_local_links_and_images_resolve` test excluded.
  A separate run confirms that check still fails on its machine-local path;
  no M2 document was edited. Logs: `/tmp/r3-dispatch-workspace.log` and
  `/tmp/r3-dispatch-known-doc-failure.log`. No new Web, live PDS or Lean theorem
  run is claimed for these host-controller changes.
- Both complete rehearsal tests pass without skips in 722 seconds. The frozen
  legacy-v1 HTTP run completes 75 tasks, retaining 76 controller receipts and
  reconciling 34 preparation plus 180 execution calls. All snapshots remain
  unchanged. `execution_valid` is true; harness/official/quality gates remain
  false, with 60 knowledge judgments pending. Every provider response is fake.
  Artifact: `/data/tmp/memory-r3-rehearsal-cli-WmD3Co/pack/run/artifact.json`;
  controller journal: the same pack's `private/dispatch/`; log:
  `/tmp/r3-dispatch-rehearsal.log`. The run executes its frozen runtime copy;
  subsequent work-list/inspection refinements are covered by the final nine
  controller tests. This is not a complete v2 approved-owner campaign.
- Removed only the owned tmpfs database `memory-r3-dispatch-20260916` after all
  database validation. Synthetic contents are regenerable; artifacts remain.
  Existing dev services, credentials and project databases were untouched.
- **R3 is not complete.** These are durable dispatch and cancellation components,
  not the complete approved paid launcher. Restart deliberately stops for
  reconciliation rather than reconstructing a lost prepared service or repeating
  uncertain calls. Locked live runtime packaging, whole-launcher recovery,
  campaign approval/calibration and official evidence remain open.

## 2026-09-16 conservative accounting and approved provider session

- Resumed the approved R3 plan and checkpointed through dev Memory before edits.
  Added an explicit v2 provider profile using full model-capacity input
  reservations, rather than claiming exact local tokenization of Responses
  framing/schema. Actual validated usage settles reservations; unknown usage
  stays reserved. No separate counting endpoint, real key read or paid call was
  used. Existing v1 canonical identities and saved artifacts remain compatible.
- The reviewed v2 profile pins the dated answer model, embedding dimensions,
  rates and capacity bounds. Per-call limits remain separate from whole-task
  budgets: a multi-call task reserves capacity for every permitted call. Tests
  cover insufficient headroom before egress, actual-usage settlement, uncertain
  charges and rejection of unreviewed counters, models, rates and overrides.
  [Accounting contract](../../../evals/r3/context-accounting.md).
- Connected the approved credential/provider owner to the shared disposable
  campaign session. Rust validates the manifest, frozen assets and exact
  approvals before secret access and again before each provider disclosure.
  Exit, approval revocation or uncertain transmission retires the bindings.
  The complete CLI controller, locked runtime packaging and interrupt/recovery
  rehearsal remain engineering work; the paid-dispatch guard stays enabled.
- Five owner/session tests pass without skips. They use synthetic approval
  records in temporary test packs, a fake secret process and mocked OpenAI
  responses, never fabricated user approval. The database-backed case prepares
  1536-dimensional vectors and runs one Full Memory knowledge task and a full
  three-step coding sequence through isolated services/containers. Both preserve
  their sealed snapshots. This verifies wiring, not live provider compatibility
  or real embedding quality/parity.
- Validation passes: 53 evaluator campaign tests, 10 campaign CLI tests, 25
  provider/context/credential tests, the five owner/session tests, workspace
  Clippy with warnings denied, formatting, boundaries and the lean CLI build.
  The latter retains eight existing dead-code warnings. Required-database
  workspace tests, including federation regressions, pass with only the existing
  unrelated M2 `markdown_local_links_and_images_resolve` check excluded. A
  separate run confirms that check still fails; its document was not changed.
  A journal fixture's happy-path subprocess timeout was raised from five to
  thirty seconds after contention exposed a startup flake. Its explicit timeout
  test and production deadlines are unchanged.
- Logs: `/tmp/r3-context-eval-tests.log`, `/tmp/r3-context-cli-tests.log`,
  `/tmp/r3-context-provider-final.log`, `/tmp/r3-approved-session-final.log`,
  `/tmp/r3-context-workspace-final.log`,
  `/tmp/r3-context-known-doc-failure.log`,
  `/tmp/r3-context-workspace-clippy.log`, `/tmp/r3-context-lean-build.log`,
  and `/tmp/r3-context-boundaries.log`. No new live PDS, Web, Lean theorem or
  remote CI run is claimed for this increment.
- The fresh legacy-v1 HTTP rehearsal completes all 75 tasks:
  `/data/tmp/memory-r3-context-Mah0m2/pack/run/artifact.json`.
  Its 34 preparation calls and 180 execution calls reconcile with actual fake
  HTTP requests; all 75 observations retain one unchanged sealed snapshot.
  The report has `execution_valid: true`, with harness/official/quality gates
  false and 60 knowledge judgments pending. This is a compatibility rehearsal,
  not a full v2 owner campaign or measured answer quality. Log:
  `/tmp/r3-context-rehearsal.log`; report: `/tmp/r3-context-final-report.json`.
- Removed only the owned tmpfs database `memory-r3-context-20260916` after
  validation. Synthetic contents can be regenerated; artifacts are retained.
  Existing dev services, credentials and project data were untouched.
- The final `remember-task` attempt could not reach dev Memory at
  `127.0.0.1:4250` (connection refused). No completion memory was stored and no
  service was restarted. This committed ledger preserves the handoff evidence;
  the successful pre-edit plan checkpoint is unaffected.
- **R3 remains incomplete.** The actual locked calibration manifest/launcher,
  label review, exact development model/spend approval, live verification and
  separately approved official campaign remain open. The revised ten-million
  token calibration cap is only a proposal reflecting conservative reservation
  headroom, not permission to spend. See the
  [remaining gates](../../../evals/r3/paid-readiness.md).

## 2026-09-16 approval and secret-read gates

- Resumed/checkpointed the existing R3 plan through dev Memory before edits.
  Dedicated-key provisioning had already succeeded in the approved OpenBao entry;
  no new credential, rotation, `.env`, deployment or existing service change was
  made. A read-only model-list request authenticated successfully. No generation
  or embedding request was made; billing/inference availability is not established.
- Added `memory eval campaign check-approvals`. It uses the existing Rust
  manifest/asset and approval validator, reports exact identity/limits, and never
  enables paid dispatch or invents approval. The rebuilt command rejects the
  retained draft rehearsal for unapproved labels and spending.
- Added the trusted-owner `approved-credential.mjs` primitive. Exact approvals
  are checked before a pinned-version Bao read and again before a host callback
  receives the key. No secret or callback result is returned; subprocess errors
  are sanitized, TLS verification stays enabled, redirects are disabled, and
  provider/proxy environment variables do not propagate. JavaScript string
  zeroization is not claimed. This primitive is not yet a live campaign launcher.
- All five credential tests pass, including real CLI denial before the fake
  secret executable starts. Fake-process cases cover stale/missing approvals,
  mid-read changes, malformed/oversized keys, error-body sanitization and ambient
  environment isolation. The CI offline group now includes these tests; remote
  CI execution is not claimed.
- Validation: 10 campaign CLI tests and 52 evaluator campaign tests pass. The
  provider/review/credential offline group passes all 18 tests without skips.
  Broader CLI verification passes 236 tests, excluding the previously recorded
  unrelated M2 documentation-link check. CLI all-target Clippy, Rust formatting,
  boundaries and lean CLI build pass (existing lean dead-code warnings remain).
  Logs: `/tmp/r3-gates-cli-tests.log`, `/tmp/r3-gates-eval-tests.log`,
  `/tmp/r3-gates-offline-tests.log`, `/tmp/r3-gates-cli-all.log`,
  `/tmp/r3-gates-clippy.log`, `/tmp/r3-gates-boundaries.log`, and
  `/tmp/r3-gates-lean-build.log`. No new full workspace/database, federation/PDS,
  Web, Lean theorem or complete campaign run is claimed for this increment.
- The [paid-readiness checklist](../../../evals/r3/paid-readiness.md) consolidates
  remaining gates and a proposed development-only calibration. Official model
  documentation and account model visibility were checked; the proposal is not
  model/spend approval. Complete Responses token accounting and the live launcher
  remain engineering work, not tasks hidden behind pending human approval.
  Fixture tree `198d202a976298539e51219da0b015b6cb9658cb` is unchanged.
- **R3 remains incomplete.** Label approval, exact locked development spending,
  and the later official campaign remain separate. Paid CLI dispatch is disabled.

## 2026-09-16 shared provider campaign session

- Added a trusted-owner session for mediated preparation and isolated task
  execution. The HTTP rehearsal now uses it. Snapshot embedding identity,
  dimensions and baseline vector provenance come from the locked provider
  profile, not synthetic defaults. The provider-only executor requires both
  model and embedding bindings; failures cannot select a fake fallback.
- Preparation consumes only the selected split. Frozen assets are checked,
  active work is serialized, and a failed preparation/task prevents additional
  execution. Cleanup refuses to race active work. This is an internal lifecycle,
  not an approved live launcher or authority to spend.
- The new disposable-database test executes all five knowledge conditions and
  a three-step Rust sequence with independently named answer/embedding models
  and three-dimensional vectors. Model calls, query embeddings, unchanged
  snapshots and pending private scoring are asserted. Changed source bytes
  prevent another call; completed preparation cannot be repeated.
- The three-test legacy/new-session group and 36-test provider/service group
  pass with no skips. The 52 core campaign tests and eight CLI campaign tests
  pass. The final three-test session suite also verifies rejection of invalid
  bindings before snapshot creation and denial of concurrent execution/cleanup.
  Workspace Clippy, formatting, boundaries and the isolated lean CLI build pass
  (eight existing lean-feature warnings). CI requires the new session test;
  remote CI execution is not claimed.
- The required-database workspace run again fails only at the existing M2
  `markdown_local_links_and_images_resolve` check, with the other 234 CLI tests
  passing. The workspace rerun excluding only that check passes, including
  federation regressions. Logs: `/tmp/r3-provider-session-workspace.log` and
  `/tmp/r3-provider-session-workspace-rest.log`. The unrelated M2 file was not
  changed. Web and Lean theorem builds were not rerun for this Node-only change.
- The fresh shared-session HTTP rehearsal passes all 75 tasks in 603 seconds:
  `/data/tmp/memory-r3-rehearsal-cli-SSbtJN/pack/run/artifact.json`.
  Its 34 preparation calls and 180 execution calls reconcile, and all snapshots
  remain unchanged. `execution_valid` is true; `harness_valid`, official and
  quality gates remain false, with 60 knowledge judgments pending. Every
  provider call was simulated; this is not measured answer quality.
- Removed only the owned tmpfs database `memory-r3-controller-20260915` after
  verification. Synthetic data can be regenerated; artifacts and source fixtures
  remain. Existing dev services and project data were untouched.
- Label approval, verified real tokenizers/prices/egress, live-launcher wiring,
  paid calibration and the official campaign remain open. The paid CLI guard
  is unchanged. **R3 is not complete.**

## 2026-09-15 locked provider and tokenizer configuration

- Added the optional canonical-compatible `provider_profile` contract. It binds
  response and embedding tokenizers to hashed configuration assets, records the
  separate embedding model/dimensions/input price and pins configuration evidence.
  Missing profiles cannot authorize paid execution or qualify as non-offline
  evidence. Unsupported protocols, mismatched models/assets and non-portable
  numeric values fail closed. Hashes do not prove tokenizer or price correctness.
- The shared prepared-call binding now supplies both call types. Focused tests
  demonstrate different response/embedding rates, immutable active bindings,
  phase separation, counter identity checks and retained provider receipts.
  The fake HTTP controller uses this binding with its actual counter source
  pinned and explicitly unapproved synthetic configuration evidence.
- Core campaign tests pass (52), CLI campaign tests pass (8), and the profile/
  compatibility/review group passes all 10 tests without skips. The previous
  75-task artifact still reports `execution_valid: true` with 60 pending judgments
  under the new binary; its canonical identity remains unchanged.
- The fresh profile-backed HTTP rehearsal passes all 75 tasks in 543 seconds:
  `/data/tmp/memory-r3-rehearsal-cli-IjOBFI/pack/run/artifact.json`.
  The counter-source pin matches the copied source; 34 preparation calls and 180
  execution calls reconcile. `execution_valid` is true; official/quality gates
  remain false, with 60 knowledge judgments pending. All calls are simulated.
- The updated required-database CI group passes all 35 tests without skips.
  Workspace Clippy, formatting, boundaries and the isolated lean CLI build pass
  (eight existing lean-feature warnings). The unfiltered workspace run retains
  the unrelated M2 documentation-path failure, with its other 234 CLI tests
  passing. The workspace rerun excluding only that check passes. Logs:
  `/tmp/r3-provider-profile-rehearsal.log`, `/tmp/r3-provider-profile-ci-node.log`,
  `/tmp/r3-provider-profile-workspace.log`,
  `/tmp/r3-provider-profile-workspace-rest.log`.
- No real model was selected or invoked; the paid-driver guard, label approval
  and paid-run gates remain unchanged. This completes configuration binding,
  not the remaining live-controller/tokenizer integration or R3 as a whole.
- Removed only the owned tmpfs database `memory-r3-profile-20260915` after
  verification. Its synthetic contents are regenerable; artifacts and fixtures
  remain. Existing dev services and project data were untouched.

## 2026-09-15 complete HTTP-mediated rehearsal

- The new `--wire` rehearsal passes all 75 tasks in 470 seconds through the
  existing Responses/embedding codecs and loopback HTTP transports. Artifact:
  `/data/tmp/memory-r3-rehearsal-cli-yFbvex/pack/run/artifact.json`.
  There are 34 preparation calls and 180 execution calls; the controller verifies
  all 214 actual HTTP requests against the durable audits. A single unchanged
  sealed snapshot is used throughout.
- `execution_valid` is true; `harness_valid`, `official_evidence` and
  `quality_passed` are false. Sixty knowledge judgments remain pending, and the
  15 no-op coding failures are expected controls. The symbolic token counter,
  model answers and vectors remain synthetic. No paid provider was contacted.
- The updated required-database CI command passes all 33 adapter/service/agent
  tests without skips. The separate wire/receipt/transport suite passes 20 tests.
  Workspace Clippy, formatting, boundaries and the isolated no-default-features
  CLI build pass; that build retains eight existing dead-code warnings and
  reports `memory 2.0.0`. The unrelated M2 workspace-test exception remains as
  documented below. Logs: `/tmp/r3-wire-rehearsal.log`,
  `/tmp/r3-wire-ci-tests.log`, `/tmp/r3-wire-adapters.log`,
  `/tmp/r3-preparation-final-clippy.log`, `/tmp/r3-preparation-lean-build.log`.
- Removed only the owned tmpfs PostgreSQL container
  `memory-r3-preparation-20260915` after all tests completed. Synthetic database
  contents can be regenerated; source fixtures and private run artifacts remain.
  Existing services and project data were not changed.
- The paid-run guard remains in place. The remaining live-driver integration,
  verified real model/tokenizer/pricing configuration and label/paid approvals
  are not claimed complete by this rehearsal.

## 2026-09-15 integrated preparation-budget rehearsal completed

- The corrected 75-task rehearsal passes in 483 seconds. Artifact:
  `/data/tmp/memory-r3-rehearsal-cli-ujsK8K/pack/run/artifact.json`.
  Its preparation phase records 34 simulated embedding calls and 34 symbolic
  input tokens; execution records 180 simulated calls, including 30 query
  embeddings. All 214 calls consume the same campaign request allowance.
- One sealed snapshot is preserved throughout. Preparation and execution spend
  are zero. Sixty knowledge judgments remain pending and the 15 no-op coding
  sequences fail as expected; this is execution evidence, not measured quality.
- Required-database workspace tests pass when excluding only the existing M2
  `markdown_local_links_and_images_resolve` failure. The unfiltered run fails
  because `m2-model/refresh-guide.md` contains a machine-local path; those
  unrelated files were left untouched. Logs are
  `/tmp/r3-preparation-workspace-tests.log` and
  `/tmp/r3-preparation-workspace-rest.log`.
- Preparation-budget integration is implemented. The complete approval-gated
  provider driver and verified real provider/tokenizer/pricing profile remain
  unfinished. No paid calls or label approvals were performed.

## 2026-09-15 preparation identity and failure regression checks

- The first integrated preparation run stopped safely after one task: using a
  reduced-budget manifest for validation changed its identity and failed the
  knowledge-review binding. Retained failed evidence is at
  `/data/tmp/memory-r3-rehearsal-cli-WAkQkk`; it is not a completed campaign.
- Fixed execution to carry remaining limits separately from the immutable
  manifest. Added an explicit identity assertion at the task boundary.
- All eight CLI campaign tests pass, including process death immediately after
  a preparation reservation, changed receipt rejection, no automatic preparation
  restart and exhaustion of the shared request allowance. Core campaign tests
  pass (50), including combined preparation/execution request, token and spend
  limits. A fresh integrated rehearsal is running against the corrected driver.

## 2026-09-15 global preparation allocation and receipt phase

- Added an optional canonical-compatible `preparation_budget` manifest contract
  and a separately audited preparation receipt. Requests, tokens and spend consume
  the global campaign allowance; only confirmed usage is subtracted before task
  limits are issued. Official reports reject missing preparation accounting.
- The CLI dispatches preparation before tasks, verifies its actual private budget
  journal/results and persists an acceptance receipt. Resume rechecks it without
  invoking preparation again; missing acceptance, changed evidence and uncertainty
  stop execution. Task snapshots must match the accepted prepared snapshot.
- Core campaign tests pass (49). The real subprocess preparation test proves
  allocation before tasks, exact remaining request limits, release of unused token
  reservations and no repeated preparation during resume. The paid-run guard is
  unchanged. The integrated synthetic rehearsal is being extended separately.

## 2026-09-15 verification of scoped embedding and budget increments

- Wired the new embedding receipt tests into the existing required-database CI
  group. Its exact local command passes all 31 tests, with no skipped tests.
  Log: `/tmp/r3-scoped-ci-node-tests.log`.
- Workspace Clippy, formatting, boundaries and the real isolated lean CLI build
  pass. The lean build keeps eight existing dead-code warnings and its executable
  reports `memory 2.0.0`.
- The unfiltered required-database workspace run fails one unrelated, in-progress
  M2 edit: `markdown_local_links_and_images_resolve` rejects an absolute local path
  in `m2-model/refresh-guide.md`. All 232 other CLI tests pass. The complete
  workspace rerun excluding only that test passes, including database/federation
  regressions. This is not an unqualified green workspace run; the M2 files were
  left untouched. Logs: `/tmp/r3-scoped-workspace-tests.log` and
  `/tmp/r3-scoped-workspace-rest.log`.
- Removed only the owned tmpfs container `memory-r3-scoped-20260915`, after all
  database tests completed. Its synthetic contents were disposable; the 75-task
  artifact, logs and source fixtures remain. Existing services were unchanged.
- R3 remains incomplete. Remaining engineering includes global preparation-budget
  integration and the complete approval-gated live driver with locked model,
  tokenizer, pricing and embedding configuration. Label and paid-run approvals
  are separate gates; they do not block continuing that offline implementation.

## 2026-09-15 integrated query-embedding accounting rehearsal

- The existing mediated executor now routes vector-baseline and Full Memory query
  embeddings through each task's durable budget. Handover, lexical and no-memory
  conditions make no query-embedding calls. Parent totals come from the complete
  task audit; coding step diagnostics retain agent-only usage.
- The 75-task rehearsal passes in 599 seconds at
  `/data/tmp/memory-r3-rehearsal-cli-Ihwq1D/pack/run/artifact.json`: 180 simulated
  calls, including 30 query embeddings. Independent audit confirms all call/token
  totals, parent-versus-step attribution and unchanged snapshot fingerprints.
- `execution_valid` is true; `harness_valid`, `official_evidence` and
  `quality_passed` remain false. The 60 knowledge judgments are pending and 15
  no-op coding sequences are expected failures. No paid calls occurred.
- This run freezes the synthetic task-embedding path and predates the later
  prepared-embedding receipt adapter and outer-ledger fix. Their focused tests
  provide separate evidence; the old artifact is not retroactively changed.

## 2026-09-15 prepared embedding receipt adapter

- Added a single-input embedding request/receipt adapter based on the official
  OpenAI schema, with explicit model, dimensions, local counter identity and rates.
  The existing transport now selects Responses or embeddings explicitly; no
  credentials are discovered, persisted or selected by this change.
- Checked pricing uses the existing Rust mediator. Exact retries reuse saved
  float32 vectors; changed payloads, undercounted tokens, missing usage and invalid
  vectors fail closed. Preparation and query calls use separate task identities.
- The 26-test combined adapter/mediator/proxy suite passes. A real disposable
  Memory service test seeds prepared vectors and queries through this adapter,
  with separate preparation/query audit totals and unchanged sealed contents.
- These are fake-provider integration tests only. They do not supply a verified
  real tokenizer, current rates, approval, global preparation-budget integration
  or a complete paid campaign driver.

## 2026-09-15 confirmed campaign reservation settlement

- The CLI reporting/preflight ledger now follows the locked durable dispatch
  ledger. Confirmed tasks release unused token/spend reservations; recovered tasks
  are not reserved twice. Interrupted attempts retain their reserved allowance.
- All six campaign command tests pass. The extended crash/recovery regression
  allows 240 tokens: enough for a completed 10/30-token task and the next 200-token
  reservation, but not two obsolete 200-token reservations. Resume completes the
  next task once and reports actual 20/60-token totals.
- This corrects execution accounting, not the still-pending integration of
  pre-dispatch corpus-preparation costs into the global campaign allowance.

## 2026-09-15 task-scoped query embedding and cancellation

- Prepared service clones accept a trusted task-specific query embedding callback;
  document preparation retains its separate callback. Repeated questions in distinct
  tasks cannot silently share execution allowances. Failed callbacks never fall back.
- Query permits cancel their callbacks on release; late results cannot enter a new
  permit. The request mediator accepts cancellation, denies already-cancelled calls
  before reservation, and retains uncertain reservations for interrupted in-flight
  calls. It does not infer free retries or log cancellation reasons.
- Four proxy tests, nine mediator tests and all three real disposable-service tests
  pass, including independent task budgets, exhaustion and unchanged snapshots.
- This is offline integration evidence, not a paid-provider or quality result.
  Full live-driver/preparation-budget integration and approval gates remain open.

## 2026-09-15 full mediated development rehearsal completed

- Completed and independently checked all 660 development tasks: 600 knowledge
  observations, 60 three-step coding sequences and 960 simulated model calls.
  Artifact: `/data/tmp/memory-r3-full-mediated-QVhHrk/pack/run/artifact.json`.
  Every call count matches its private audit; one sealed snapshot fingerprint is
  preserved throughout. Provider spend and preparation spend are both zero.
- `execution_valid` is true. `harness_valid`, `official_evidence` and
  `quality_passed` remain false: 600 semantic judgments are pending, and the 60
  no-op coding failures are expected negative controls. Do not ask a human to
  grade these fake outputs as if they were paid model outcomes.
- This run froze at the shared-synthetic-embedding increment; the later query
  timing correction is covered by the passing 75-task run at
  `/data/tmp/memory-r3-rehearsal-cli-7yDI2p/pack/run`. Prepared Responses calls are
  covered separately by fake-server and isolated model workflow tests.
- Refreshed required-database workspace tests, workspace Clippy, formatting,
  boundaries and 29 provider/packet tests pass. A real no-default-features CLI
  build and executable version check pass at
  `/data/tmp/memory-r3-lean-target-EzlJOc/debug/memory`; the build retains eight
  existing dead-code warnings. No shared development binary was replaced by it.
- Removed only the owned tmpfs PostgreSQL container `memory-r3-http-20260915`.
  Its synthetic database was disposable; source fixtures and run evidence remain.
  Existing services and unrelated M2 financial-model work were left untouched.
- R3 is not complete. The live driver still needs the selected, verified real
  provider/tokenizer/pricing and embedding profile. Label approval is requested
  separately, and paid development/official runs remain unauthorized and disabled.

## 2026-09-15 human label-review entrypoint

- Added `evals/r3/label-review.md` with the exact fixture-tree identity, every
  family review sheet and a plain-language explanation of label-only approval.
  It does not approve labels, select a provider or authorize spending.
- Its test verifies all 51 sheets, the committed fixture identity and absence
  of uncommitted/untracked fixture changes. Unrelated project work is out of scope.

## 2026-09-15 prepared-counter failures and current short rehearsal

- Local tokenizer exceptions are sanitized before any reservation or model call.
  Async counters are rejected and their rejected promises are consumed, preventing
  accidental raw-input error output. Five prepared-call tests pass.
- The current short mediated rehearsal passes all 75 tasks in 475 seconds at
  `/data/tmp/memory-r3-rehearsal-cli-7yDI2p/pack/run`, with shared synthetic semantic
  embeddings and corrected query-vector timing. Its 150 calls are simulated and
  its semantic judgments remain pending. It does not exercise live tokenization
  or prices; the full 660-task run remains in progress.

## 2026-09-15 bounded development calibration allocation

- Diagnostic development packs can now be authorized for the plan's small
  calibration, rather than requiring all 660 development dispatches up front.
  Complete families, both tracks, all conditions/repeats and separate matching
  label/spend approvals are still required. Held-out diagnostic packs remain
  ineligible for official evidence; the paid CLI guard is unchanged.
- Added a 75-dispatch calibration contract test covering absent approvals,
  missing spend approval, held-out denial and partial-family rejection.
- Evaluator tests and the four representative-pack tests pass with the freshly
  rebuilt CLI. Draft labels, missing budget and manifest/source tampering still
  fail closed; no paid execution is enabled by this change.

## 2026-09-15 remove circular development baseline gate

- Development readiness no longer demands the reference baseline that calibration
  is intended to select. It still requires matching label/development approvals;
  held-out authorization continues to require the frozen baseline and its own
  approval. Changing the manifest invalidates earlier approvals as before.
- Evaluator tests pass, including a new test of this distinction. The paid CLI
  guard is unchanged, so this is not authorization to make any provider call.

## 2026-09-15 prepared provider calls through existing isolated workflows

- The private budget bridge exposes checked price arithmetic without reserving
  or authorizing a call. The existing knowledge/agent adapters accept preparation
  of the complete wire request before reservation, preserving their task identities
  and accounting preparation time inside the deadline.
- Provider preparation binds model, tokenizer identity, quoted input count and
  exact request bytes. Saved private results retain cache/reasoning billing
  metadata; agents receive only their answer/action. Token undercounts and unknown
  cache prices leave an uncertain attempt and block further calls.
- Eight mediator tests, four prepared-call tests and five model workflow tests
  pass, including all knowledge conditions and the actual isolated three-step
  driver. Evaluator Clippy passes after the Rust price-bridge addition.
- These tests use synthetic local counters and fake responses, not a verified
  production tokenizer or paid provider. Provider/model/tokenizer/price approvals,
  real embedding parity and the paid CLI guard remain unresolved. The full
  development rehearsal is still running against its earlier frozen runtime.

## 2026-09-15 common provider request codec and query timing correction

- Added a pure structured-request codec shared across knowledge conditions and
  isolated coding actions. Model/reasoning/output settings are fixed; hosted tools,
  storage, background execution and automatic truncation are disabled. Tests cover
  payload isolation, common instructions and malformed action rejection.
- Query-vector generation now belongs inside retrieval timing; document and
  lexical conditions no longer calculate unnecessary synthetic vectors. The real
  required-database five-condition/hidden-Rust task test passes after this change.
- The ongoing full rehearsal uses its earlier immutable copy, so it excludes the
  small synthetic query-vector calculation from baseline timing. Do not claim
  that artifact verifies this newer timing fix or the provider request codec.

## 2026-09-15 explicit cache-price accounting

- Added an optional approved cached-input rate while preserving legacy model
  encodings when it is absent. Reservations remain undiscounted; settlement keeps
  cached tokens inside input totals and requires a known rate before discounting.
- Checked integer arithmetic rounds once, rejects impossible counts, unknown
  cache pricing and rates above the reservation price. Evaluator tests, six CLI
  campaign tests and scoped Clippy pass. This is not provider billing validation.
- Refreshed required-database workspace tests and workspace Clippy passed before
  this small price extension; the full mediated rehearsal is still running with
  its previously frozen executable copies and no cache-price extension in its pack.

## 2026-09-15 bounded provider transport and refreshed timing rehearsal

- Added a trusted HTTP primitive with fixed destinations, explicit credentials,
  no redirects/retries, deadline cancellation and bounded UTF-8 JSON responses.
  The offline default accepts only synthetic loopback credentials. It does not
  enable paid campaign dispatch or replace input-token and price verification.
- Four transport tests and the combined 14-test transport/receipt/mediator suite
  pass. The real journal is reserved before fake HTTP transmission; an exact retry
  is cached, missing usage becomes uncertain and prevents a further request.
- The refreshed 75-task mediated timing rehearsal passes in 435 seconds at
  `/data/tmp/memory-r3-rehearsal-cli-6YFPFh/pack/run`. All 150 calls are simulated;
  labels remain pending and no quality/paid evidence is claimed.
- The newer full 660-task mediated run includes shared synthetic embeddings and
  is in progress, as are refreshed required-database workspace tests.

## 2026-09-15 shared synthetic embedding profile

- New rehearsal packs bind the same synthetic embedding function to the simple
  vector baseline and Full Memory's active semantic service. Values survive f32
  storage exactly; the service rejects silent lexical fallback.
- Preserve existing Memory chunking and the baseline's fixed whole-source chunks;
  record that distinction in the hashed configuration. An initial test wrongly
  demanded identical chunk strings and failed during preparation; it was corrected
  without changing retrieval algorithms or treating that failed run as evidence.
- The required-database test passes all five knowledge conditions and real hidden
  Rust grading, alongside two fixture checks. These are synthetic model-plumbing
  checks, not real embeddings or quality evidence. The already-running timing
  campaign froze earlier code and does not verify this newer change.

## 2026-09-15 strict provider receipt decoding

- Added a pure decoder for completed, tool-free Responses text receipts. It
  rejects changed models, inconsistent or novel usage, tool calls, truncation,
  refusal and malformed JSON without exposing raw provider errors. Cached input
  and reasoning usage remain explicit; reasoning is not counted twice.
- Three synthetic tests pass and are included in CI. This component performs no
  network calls and does not yet supply authenticated transport or verified prices.
- A fresh 75-task mediated rehearsal with the new timing/official gates is now
  running. Earlier passing artifacts are not retroactively timing evidence.

## 2026-09-15 official provider evidence gate

- Official reports now require the bounded per-call profile, coherent task receipt
  audits and at least one accounted model request per task. Legacy unmediated
  artifacts remain readable offline, but cannot establish official evidence.
- Portable audit validation permits conservative reservation ceilings within the
  locked model price; CLI acceptance still verifies the exact issued contract and
  actual private journal. Synthetic report fixtures do not authenticate providers.
- Evaluator tests and scoped Clippy pass. Paid dispatch remains disabled; real
  tokenization, pricing and embedding parity still need implementation and the
  separate label/provider/budget approvals remain open.

## 2026-09-15 mediated rehearsal completed

- The fresh mediated run passed all 75 tasks and verified 150 simulated calls
  against actual journals and saved receipts in 446 seconds. Artifact directory:
  `/data/tmp/memory-r3-rehearsal-cli-SZaLCz/pack/run`.
- All task counters match their audits; database fingerprints stay unchanged.
  Sixty knowledge judgments remain pending and fifteen no-op coding sequences
  correctly fail hidden tests. Zero actual provider calls/spend; no product-quality
  or label-approval claim. The earlier interrupted run remains retained separately.

## 2026-09-15 execution-only timing

- New task timing separates retrieval, answering/agent execution, private grading
  and the complete executor span. Parent duration is retrieval plus answer/agent
  time; child coding durations exclude hidden grading. Snapshot provisioning and
  private adjudication no longer feed execution latency comparisons.
- Official reports reject missing timing metadata; legacy offline artifacts remain
  readable. Supplied timing must add up without overflow or overlapping totals.
  Mediated CLI dispatch requires this evidence before accepting a task.
- The required-database task test exercises all five knowledge conditions and a
  real hidden Rust sequence with these assertions. Six CLI tests pass. This timing
  change was made after the full mediated run froze its executables; that run is
  evidence for mediation, not for the new timing fields.

## 2026-09-15 mediated rehearsal deadline and retained evidence

- The first mediated 75-task run reached its outer test deadline: the controller
  finished task 75 after the CLI had been interrupted, so no completed campaign
  artifact was accepted. Its partial journals/diagnostics remain at
  `/data/tmp/memory-r3-rehearsal-cli-mxOzFg`. It is not passing evidence.
- Increased the mediated test's overall allowance from ten to fifteen minutes,
  leaving per-task deadlines, call/token/spend limits, labels and scoring unchanged.
  A fresh zero-spend synthetic rerun is in progress, not yet claimed passed.
- Workspace Clippy and the lean CLI check passed (the latter retains existing
  dead-code warnings). Required-database workspace tests passed before this
  test-only timeout adjustment.

## 2026-09-15 campaign multi-call integration

- Added an explicit, bounded `provider_budget` manifest extension. Absent fields
  preserve legacy canonical bytes and its single-call behavior. In mediated mode
  each task receives a private journal contract clamped to remaining request quota.
- The CLI verifies actual journal/result files before accepting the portable
  audit, on both fresh execution and recovery. Task counters must match; a live
  writer, self-consistent invented audit, altered receipt or exhausted quota fails.
  Report validation checks the portable audit against the locked task/model bounds.
- Six CLI campaign tests, 61 evaluator tests and the required-database workspace
  suite pass. Formatting and boundaries pass. Workspace Clippy and the 75-task
  mediated rehearsal are running; they are not yet claimed passed.
- The preceding legacy 75-task regression passed in 415 seconds, retained at
  `/data/tmp/memory-r3-rehearsal-cli-w2teEy`. The first attempt used a mistyped image
  ID and failed before dispatch; it was not accepted as evidence.
- `--mediated` uses simulated excerpt/no-op callbacks, symbolic token receipts and
  zero spend. It does not enable paid execution, settle labels, provide real
  embedding parity or establish task latency/quality.

## 2026-09-15 provider receipt audit

- The mediator can produce a metadata-only audit by verifying every actual result
  file against the locked journal. It reports unique calls, task aggregates,
  limits, payload/result digests and usage, never prompts or outputs.
- Unknown or tampered receipts invalidate the audit; cached deliveries and restart
  do not change it. Task aggregates must exactly equal the journal totals.
- Sixty evaluator tests and six bridge tests pass, including new audit checks;
  evaluator Clippy passes. This does not authenticate a provider or remove the
  CLI's current single-call/paid-dispatch guard. Connecting verified multi-call
  audits to campaign execution is the next integration step.

## 2026-09-15 common mediated answers and recovered task accounting

- All five conditions can use the same tool-free answer adapter after the existing
  exact-source/context checks. Only selected context and the question reach the
  model; full sources and rubrics stay in the existing private Rust adjudicator.
  The result does not pretend the external model ran inside our container image.
- Durable call receipts supply token/cost counts. Recovered tasks retain their
  original attributed cost, with new invocations reported separately; a cached
  answer is neither a second charge nor a zero-cost replacement for prior usage.
  Coding sequence parents sum their per-step receipts once; steps stay diagnostic.
- The combined three model-adapter tests, six mediator fault/restart tests and
  four existing knowledge/retriever tests pass. Fake outputs with invalid citations
  fail; valid citations still await human rubric judgment. No labels or paid runs
  were approved by these checks. Integrated CLI regression is being re-run.

## 2026-09-15 isolated multi-call coding driver

- The existing three-step sequence runner accepts a trusted agent adapter with
  only the current public workspace. It never passes the private grader object,
  future tasks or grading feedback. Adapter submissions use the same validation
  and private grading path as the existing container runner.
- A bounded structured model loop routes every callback through the durable
  request mediator. Commands run only in network-none containers; only selected
  submission files persist. The model gets no host command facility or secrets.
  Per-sequence call/time limits cover the answering loop, excluding hidden grading.
- Two required container tests pass: six budgeted fake model calls complete three
  steps, while malformed commands cannot execute on the host or pass scoring.
  Existing sequence isolation/reset tests also pass. This verifies a restricted
  driver protocol, not model competence or a paid-provider integration.

## 2026-09-15 per-call execution bridge

- The existing Rust request-budget/result store now exposes a reservation-or-cache
  decision for asynchronous dispatch. A private JSONL helper and trusted-host
  adapter reserve before each callback, save confirmed outputs, and reuse exact
  completed requests without invoking again. No agent receives budget authority.
- Six required bridge tests pass, including an actual killed budget process,
  restart, per-sequence and campaign request caps, changed retries, missing or
  excessive usage, and timed-out callbacks. Uncertain attempts retain the whole
  allowance and block further calls until explicit reconciliation.
- Three required-database service tests pass. One connects this bridge to actual
  foreground-service embedding callbacks: one preparation call and two query
  calls settle; the next query hits its task cap before the callback executes.
  Synthetic microdollar counters test accounting only; no paid calls occurred.
- This is not yet an approved provider adapter, tokenizer/price contract, coding
  agent loop or official campaign integration. Whole-task wall-clock enforcement
  remains the controller's responsibility; this adapter bounds individual calls.
  Do not use these test callbacks as proof of paid execution safety or quality.

## 2026-09-15 configured semantic retrieval in sealed service clones

- The trusted snapshot helper can plan chunks using Memory's actual chunker and
  seed explicitly supplied vectors with a loopback-only embedding profile.
  Supplied preparation cost remains unknown, not fabricated as zero.
- A private, temporary embedding adapter binds each query to its exact text and
  model, permits one callback, validates float32 vectors and rejects silent
  lexical fallback. It is not a paid-provider or budget mediator. No credentials
  from the host are inherited and the helper does not contact a provider.
- Two required-database foreground-service tests pass: semantic candidates are
  present, foreign-project contents stay excluded, independent clones preserve
  their fingerprints, and a failed embedding callback invalidates the query.
  Two adapter unit tests also pass. These are synthetic plumbing checks, not
  production corpus/configuration parity or official evidence.
- The preceding 75-dispatch CLI rehearsal passed in 395 seconds; diagnostics are
  retained at `/data/tmp/memory-r3-rehearsal-cli-DzLP4J`. Human knowledge review
  remains pending and no paid calls occurred.

## 2026-09-15 embedding evidence bindings

- Vector comparator execution now requires exact document/query text and vector
  hashes plus equal declared profiles, including model, dimensions, normalization,
  purposes and metric. Same-dimensional incompatible models, swapped rows and
  changed inputs fail before ranking. Older unbound configs decode but cannot
  execute vector mode; lexical/handover behavior is unchanged.
- Rust/JavaScript use binary64 little-endian vector hashing for byte agreement.
  All 58 evaluator tests, two required cross-language retriever tests, four
  isolated knowledge tests, Clippy, formatting and boundaries pass. A fresh
  75-dispatch integrated rehearsal subsequently passed (see entry above).
- These hashes establish consistency, not provider origin, real embedding
  preparation or quality. Fake profiles remain explicit and paid dispatch remains
  disabled. Full Memory preparation parity and provider mediation are still open.

## 2026-09-15 combined held-out fixture verification

- All twelve held-out repositories passed together, including private references,
  cumulative stages and deliberately failing starter/no-op controls: twelve test
  groups, zero failures/skips, 284 seconds. Log:
  `/tmp/r3-all-held-out-repositories.log`. Every public repository's two ordinary
  tests also passed during its implementation.
- This verifies the draft fixtures and isolation machinery, not held-out model
  performance. Full draft counts are 140 questions and sixteen three-step
  repositories across the two splits; labels and official evidence remain pending.

## 2026-09-15 complete draft coding allocation

- Structured-redaction completes twelve held-out repositories, each with five
  public modules, three cumulative stages and ten hidden probes. The final family
  tests schema allowlists, ambiguous structures, exact destination consent and
  invalidated grants after source replacement. No real secrets or model calls.
- Its two public tests, required reference/cumulative/starter/no-op container
  group and four pack/CLI tests pass without skips. Every held-out repository
  passed those controls individually; a combined twelve-repository rerun is in
  progress and is not yet claimed passed here.
- The full draft allocation now matches R3: 40/100 knowledge questions and 4/12
  coding sequences for development/held-out. The held-out planner produces 1,680
  dispatches across five conditions and three repeats. These are fixtures, not
  approved labels, real-model quality or an official completed campaign.

## 2026-09-15 held-out bounded-decoder repository

- Added five modules, two public tests and ten private probes for split run
  pairs, exact/cumulative expansion limits, failed-stream fencing and atomic
  publication of the last completed result. No filesystem or real decompressor.
- Public tests, required container reference/cumulative/negative controls and
  four pack/CLI tests pass without skips. Eleven of twelve held-out repositories
  exist; only structured-redaction remains before draft fixture allocation is full.

## 2026-09-15 held-out route-precedence repository

- Added five public modules, two tests and ten hidden probes for lexicographic
  route specificity, HEAD fallback, capture isolation, normalized conflicts,
  exact retry and atomic replacement/removal, including the full-table boundary.
- Public tests, required container reference/cumulative/negative controls and
  four pack/CLI tests pass. Re-ran container checks after extending capacity
  coverage; all pass without skips. Ten of twelve held-out repositories exist.
  This is an in-memory router fixture, not an HTTP or access-control release.

## 2026-09-15 held-out clocked circuit repository

- Added five modules, two public tests and ten hidden probes for consecutive
  failures, single half-open probes, stale completions, permit expiry and
  backwards-time rejection. Explicit logical time replaces any real clock.
- Public tests, required container reference/cumulative/negative controls and
  four pack/CLI tests pass without skips. Nine of twelve held-out repositories
  exist; all 140 knowledge questions remain draft. No external calls occurred.

## 2026-09-15 held-out sparse-sensor repository

- Added five public modules, two public tests and ten hidden probes across
  canonical sparse ingestion, rectangular transposition and checked dot-product
  composition. The reference preserves aliases, source shape and rejected state.
- Public tests and the required container reference/cumulative/starter/no-op
  group pass without skips, as do four pack/CLI tests. Eight of twelve held-out
  repositories exist. The knowledge allocation remains complete but unapproved.
- Current evaluator Clippy, formatting and boundary checks also pass. These
  synthetic fixture tests do not establish actual sensor or model quality.

## 2026-09-15 complete draft knowledge allocation

- Color-profile and distribution-notice families complete all 100 held-out
  questions across 25 independently allocated families. Each has five synthetic
  source documents, four canonical private rubrics and a review sheet. All 40
  development questions across ten separate families also exist. No labels are
  frozen or approved and no held-out model calibration has occurred.
- All 56 evaluator tests and four required pack/CLI tests pass without skips.
  The explicit draft held-out pack plans 1,605 dispatches for 100 questions plus
  seven coding repositories across five conditions/three repeats. This is a
  planning check, not a completed held-out run or permission for paid calls.
- Five held-out coding repositories remain, alongside real embedding parity,
  provider-call mediation and approval/evidence gates. These new fixtures do not
  change production retrieval, filesystem, identity or distribution behavior.

## 2026-09-15 held-out field-matching and trace-link families

- Added ten sources and eight draft questions on field/token boundaries,
  authorization-before-limits, immutable async carriers and sampled telemetry
  versus durable business receipts. No retrieval tuning or telemetry changed.
- Canonical rubric and four pack/CLI checks pass without skips. Ninety-two held-out
  questions and seven coding repositories exist; two knowledge families and five
  coding repositories remain. Labels remain draft and outside calibration.

## 2026-09-15 held-out persistence and null-filter families

- Added ten sources and eight draft questions on rename/flush confirmations,
  uncertain external effects, SQL UNKNOWN and project-correlated anti-joins.
  Filesystem and query semantics are explicit synthetic contracts.
- Canonical rubric and four pack/CLI checks pass without skips. Eighty-four
  held-out questions and seven coding repositories exist; four knowledge families
  and five coding repositories remain. No live storage or database writes.

## 2026-09-15 held-out lock and aggregate-size families

- Added ten sources and eight draft questions on opposite lock ordering,
  callback reentry, source revalidation and checked aggregate sizing. All models
  are explicit and synthetic; no production synchronization changed.
- Canonical rubric and four pack/CLI checks pass without skips. Seventy-six
  held-out questions and seven coding repositories exist; six knowledge families
  and five coding repositories remain. Labels and paid approvals remain open.

## 2026-09-15 held-out memory-budget and reduction-order families

- Added ten synthetic sources and eight draft questions on disjoint group charges,
  byte-based admission, finite-precision reduction order and invalid duplicate
  contributions. No production memory or scoring algorithm changed.
- Canonical rubric and four pack/CLI checks pass without skips. Sixty-eight
  held-out questions and seven coding repositories are materialized; eight
  knowledge families and five coding repositories remain. Labels remain draft.

## 2026-09-15 held-out stream and drain knowledge families

- Added ten sources and eight draft questions for byte framing/UTF-8 boundaries,
  explicit failed-feed semantics, local admission gates, active streams and
  uncertain provider outcomes. All observations are synthetic.
- Canonical rubric and four pack/CLI checks pass without skips. Sixty held-out
  questions and seven coding repositories are materialized; ten knowledge families
  and five coding repositories remain. Held-out labels remain unapproved.

## 2026-09-15 held-out trust-boundary knowledge families

- Added certificate-chain and token-audience families: ten synthetic sources and
  eight draft questions on missing intermediates, clean clients, exact resource
  audiences, independent scope/membership checks and unsafe workaround refusal.
  No live certificates, credentials, issuer or network connection is used.
- Canonical rubric and four pack/CLI checks pass without skips. Fifty-two held-out
  questions and seven coding repositories are materialized; twelve knowledge
  families and five coding repositories remain. Approval gates remain open.

## 2026-09-15 held-out vector-compatibility knowledge family

- Added five sources and four draft questions on model aliases, complete profile
  equality, exact index coverage and unavailable compatibility. Toy vectors are
  explicit diagnostic data, not an embedding-quality or provider claim.
- Canonical rubric and four pack/CLI checks pass without skips. Forty-four held-out
  questions and seven coding repositories exist; fourteen knowledge families and
  five coding repositories remain. No change to retrieval algorithms or egress.

## 2026-09-15 held-out watcher-replacement knowledge family

- Added five sources and four draft questions for stale file-object watches,
  same-metadata content changes, invalid candidates, event loss and explicit
  adoption/durability limits. No actual host configuration is read or changed.
- Canonical rubric and four pack/CLI checks pass without skips. Forty held-out
  questions and seven coding repositories are materialized; fifteen knowledge
  families and five coding repositories remain. All labels remain draft.

## 2026-09-15 held-out quantization-error knowledge family

- Added five source documents and four draft questions distinguishing per-reading
  rounding from final batch conversion, checked signed accumulation, reviewed
  historical repair and the limits of recovering raw data from rounded values.
- Canonical rubric validation and four pack/CLI tests pass without skips. There
  are 36 held-out questions and seven coding repositories; sixteen knowledge
  families and five coding repositories remain. No paid calls or label approval.

## 2026-09-15 held-out Unicode-identity knowledge family

- Added five sources and four draft rubrics for explicit normalization rules,
  original display bytes, stable IDs, conflicting lookup ownership and permission
  preservation. The fixture does not claim a complete Unicode/security standard.
- Canonical rubric and four pack/CLI tests pass without skips. Thirty-two held-out
  questions and seven coding repositories are materialized; seventeen knowledge
  families and five coding repositories remain. No labels/model runs approved.

## 2026-09-15 held-out cohort-assignment knowledge family

- Added five sources and four draft rubrics for account-keyed assignment,
  deliberate rollout expansion, retained actual exposures and account-level
  measurement. Same-seed/session changes are not treated as valid account identity.
- Canonical rubric and four pack/CLI tests pass without skips. Twenty-eight
  held-out questions and seven coding repositories exist. The explicit held-out
  fixture pack plans 525 executions; none is approved model-calibration evidence.

## 2026-09-15 complete development offline execution

- The real controller/CLI completed all 660 representative development executions
  (40 knowledge questions and four three-step repositories, five conditions,
  three repeats) and the independent review export validated their evidence.
  The controller exited successfully. The inner campaign exit1 correctly reflects
  pending semantic review, not an execution failure or fabricated approval.
- Artifact: `/data/tmp/memory-r3-full-development-0D0PLW/pack/run/artifact.json`;
  progress/result log: `/tmp/r3-full-development-rehearsal.log`. There are 600
  pending knowledge packets and 60 failed no-op coding sequences, zero provider
  calls/spend, one prepared snapshot fingerprint and zero before/after changes.
  Report execution validity is true; harness/official/quality gates remain false.
- Private pinned executables and cloned service/agent/grader isolation were used.
  Normal controller cleanup completed. Synthetic agents/fake vectors do not
  establish real embedding parity, task latency, model quality or customer ROI.
  Labels, remaining held-out fixtures, provider mediation and paid gates remain
  open. No existing deployment or customer data was used.

## 2026-09-15 held-out notice-bundle repository

- Added five modules, two public tests and ten hidden probes for selected scope,
  retained attribution, text deduplication and complete metadata before replacing
  a generated bundle. Fictional notice classes are not actual license-law claims.
- Public tests, the targeted required reference/cumulative/broken-control group
  and four pack/CLI tests pass without skips. Seven of twelve held-out repositories
  exist. Offline fixture dispatch ceilings now match the exact planned work count;
  this does not authorize paid requests or replace per-provider-call mediation.
- Current evaluator Clippy, formatting and boundaries also pass. Remote CI is
  not claimed. All held-out labels remain draft and outside calibration packs.

## 2026-09-15 held-out alert-denominator knowledge family

- Added five sources and four draft rubrics for biased trace samples, aligned
  completed-request counters, weighted window aggregation and unsupported user/
  causal/revenue claims. Counts and rates are synthetic; no production metrics.
- Canonical rubric and four pack/CLI tests pass without skips. Twenty-four
  held-out questions and six coding repositories exist. Labels remain draft;
  the held-out fixture pack is separate and non-promotable.

## 2026-09-15 held-out snapshot-pages repository

- Added five modules and ten probes for tied ordering, project-before-limit
  filtering, immutable reads under live mutations, cursor binding and expiry.
  This is an in-memory snapshot model, not deployed database or auth evidence.
- Two public tests, the required targeted reference/cumulative/broken-control
  group and four pack/CLI tests pass without skips. Six of twelve held-out
  repositories exist. The development rehearsal has not yet completed.

## 2026-09-15 held-out circuit-probes knowledge family

- Added five synthetic sources and four draft rubrics for shared half-open
  admission, generation/claim-bound completion, separate liveness and unresolved
  dependency recovery. These are stated fixture policies, not production changes.
- Canonical rubric and four pack/CLI tests pass without skips. Twenty held-out
  questions and five coding repositories are materialized. The explicit fixture
  pack plans 375 executions; labels and official evidence remain pending.

## 2026-09-15 held-out identifier-catalog repository

- Added a five-module catalog with an explicit small normalization table, distinct
  display/key identity, collision-safe insertion/rename and released ownership.
  Ten hidden probes do not assume full Unicode normalization or authentication.
- Two public tests, the targeted required reference/cumulative/broken-control
  group and four pack/CLI tests pass without skips. All four earlier held-out
  repository groups also pass together (`/tmp/r3-held-out-four-repositories.log`).
  Five of twelve held-out repositories exist; labels remain draft.

## 2026-09-15 held-out archive-expansion knowledge family

- Added five synthetic sources and four draft rubrics on observed expansion,
  simultaneous per-entry/total/ratio limits, private staging and incomplete
  exposure evidence. No archive was generated, extracted or delivered.
- Canonical rubric and four pack/CLI tests pass without skips. Sixteen held-out
  questions and four coding repositories are materialized. The fixture pack
  remains draft/smoke-only and excluded from model calibration.

## 2026-09-15 held-out decimal-register repository

- Added five modules and three private reference replacements for exact fixed-
  point parsing, checked accumulation and rational half-away rounding. Ten probes
  include both i64 extremes, invalid grammar, wide products and state preservation.
  Units are fictional measurements, not provider prices or money.
- Two public tests, the required targeted container group (reference/cumulative
  pass; starter/no-op fail each stage), and four pack/CLI tests pass with no skips.
  Four of twelve held-out repositories are materialized. No labels are approved.

## 2026-09-15 held-out cancellation-scope knowledge family

- Added five sources and four draft rubrics distinguishing disconnected callers,
  explicit remote stop requests, operation-bound terminal receipts and unresolved
  external work. Cancellation does not imply rollback, free usage or compensation
  authority. All names and operations are synthetic.
- Canonical rubric and four pack/CLI tests pass without skips. Twelve held-out
  questions and three coding repositories are materialized; the explicit fixture
  pack plans 225 executions. No model calibration, label approval or paid run.

## 2026-09-15 held-out path-policy repository

- Added the allocated five-module path model with two public tests and ten
  private probes: normalized component boundaries, independent operations,
  exact revocation, aliases, cycles and bounded rewrite depth. No host paths are
  opened and this is not evidence of a deployed filesystem sandbox.
- Two public tests, the targeted required container reference/cumulative/broken
  controls (one group, no skips), and all four pack/CLI tests pass. Three of
  twelve held-out repositories are materialized. Held-out labels remain draft.

## 2026-09-15 held-out cursor-snapshot knowledge family

- Added five sources and four draft rubrics for tied sort fields, immutable page
  snapshots, changed live ordering and cursor/project scope. The source journal
  describes observed rows; private rubrics derive the required continuation.
- Removed precomputed Celsius conversions from the draft telemetry journal so
  answers must join raw samples with the supplied unit contract. No model outcome
  motivated this source cleanup; neither family is approved or calibrated.
- Canonical rubrics and all four pack/CLI tests pass, no skips. Eight held-out
  questions and two held-out coding repositories are materialized. The separate
  held-out pack plans 150 fixture executions but is not official evidence.

## 2026-09-15 held-out interval-calendar repository

- Added the allocated half-open interval repository with five modules, two public
  tests and ten private probes over canonical unions, subtraction and clipped
  free windows including signed-integer extremes. Abstract ticks have no civil
  time or dispatch semantics, keeping this distinct from development scheduling.
- Both public tests pass. Both held-out repository container groups pass without
  skips: references/cumulative execution succeed, starter/no-op controls fail
  each stage. Four pack/CLI tests pass and plan the isolated held-out allocation.
  Two of twelve held-out repositories exist; labels remain draft.

## 2026-09-15 first held-out knowledge family and separate packs

- Added the allocated telemetry-units scenario: five synthetic sources and four
  draft rubrics joining mixed firmware, conversion provenance, genuine warnings
  and absent physical/population measurements. No equipment or provider was used.
- Added explicit held-out-only materialization. Default development packs remain
  separate; the real CLI rejects calibration planning on a held-out pack and
  refuses promotion of partial fixtures. The offline controller remains
  development-only. All four pack/CLI tests and canonical rubric tests pass.
- One of 25 held-out knowledge families and one of twelve coding families exist.
  Labels remain draft; deterministic fixture verification is not model calibration
  or official held-out execution. The development rehearsal is still running.

## 2026-09-15 first held-out coding repository

- Added the allocated framed-stream repository: five modules, two public starter
  tests, three cumulative stages and ten distinct private probes. Covers chunked
  headers/payloads, complete UTF-8 validation, bounded frames, poisoned input and
  explicit close/reset. This models an in-memory receiver, not live networking.
- Both public tests pass. The required container test passes in 25 seconds with
  no skips: reference and cumulative execution pass, starter/no-op controls fail
  every stage. CI requires the held-out fixture verifier. No model execution,
  calibration or label approval uses these held-out references.
- One of twelve held-out repositories is materialized. The larger development
  rehearsal remains running; its completion is not yet claimed.

## 2026-09-15 complete development benchmark allocation

- Added the tenth knowledge family, DNS cutover: five synthetic sources and four
  draft rubrics on split views, previous TTLs, connection reuse and incomplete
  rollout evidence. All forty development questions and four three-step coding
  repositories are now materialized. The hundred held-out questions and twelve
  held-out repositories remain unimplemented; no label approval is implied.
- The real CLI plans all 660 unique development executions (44 tasks, five
  conditions, three repeats). Tests verify every materialized family participates,
  no sources/labels silently disappear, and changing approval flags cannot promote
  the development-only pack to official evidence. All three pack/CLI tests and
  all 56 evaluator tests pass without skips. This is planning/fixture evidence,
  not a claim that all 660 representative tasks have executed successfully.

## 2026-09-15 quota-accounting knowledge family

- Added five synthetic metering sources and four draft rubrics for reservations,
  uncertain effects, parent/child aggregation and incomplete-campaign reporting.
  Uses fictional microcredits, not provider pricing or a new provider adapter.
- Canonical rubric and both pack/CLI tests pass without skips. Thirty-six
  development knowledge questions are materialized. Unknown usage remains a
  reason not to claim valid official results, rather than a fabricated zero.

## 2026-09-15 retention-hold knowledge family

- Added five fictional policy/inventory/journal sources and four draft rubrics
  separating read expiry, hold authority, uncertain external deletion and copies
  in backups/exports. No jurisdictional legal conclusions are benchmark facts.
- Canonical rubric and both pack/CLI tests pass, no skips. Thirty-two development
  knowledge questions are materialized. These are offline fixture checks, not
  tested customer retention compliance or approved evaluation labels.

## 2026-09-15 artifact-origin knowledge family

- Added five fictional release sources and four draft rubrics distinguishing
  byte/channel identity, current signing authority, installation verification
  and unsupported reproducibility or fleet-wide exposure claims.
- Canonical rubric and two pack/CLI tests pass without skips. Twenty-eight
  development questions are materialized; synthetic digest labels do not claim
  real binary verification. Human labels and official evidence remain pending.

## 2026-09-15 civil-time knowledge family

- Added five sources defining a synthetic zone table, civil-date identity and
  explicit fold/gap policy, with four draft questions and a review sheet.
  Sources distinguish one rehearsal from unknown historical customer impact.
- Canonical rubric and both pack/CLI tests pass, no skips. Twenty-four development
  knowledge questions are materialized; no live timezone or provider lookup was
  used to supply benchmark facts. Label and evaluation approval remain pending.

## 2026-09-15 capacity-pressure knowledge family

- Added five synthetic sources and four draft rubrics on pool acquisition,
  contention, process-wide connection budgets and confounded recovery evidence.
  The arithmetic ceiling is explicitly not a throughput recommendation.
- Canonical rubric and two pack/CLI tests pass without skips. Twenty development
  knowledge questions and four coding sequences are materialized. Labels and
  official evaluation remain unapproved; these are fixture validations only.

## 2026-09-15 schema-transition knowledge family

- Allocated the remaining seven development knowledge mechanisms before writing
  their sources. Added the fourth family: five operational documents and four
  draft rubrics covering mixed-version compatibility, exception handling,
  rollback write shapes and unsupported performance guarantees.
- Canonical rubric validation and both representative pack/real CLI tests pass;
  no skips. The default pack now has sixteen knowledge questions and four coding
  sequences. Source and review material remain separated. No label approval or
  paid execution is implied; full benchmark and provider integration remain open.

## 2026-09-15 complete representative offline controller rehearsal

- Added an offline bootstrap and host-only Unix-socket driver/controller for the
  existing campaign CLI. Actual modules and private executable copies are pinned;
  one prepared snapshot supplies independently reset service instances for every
  condition/task. Exact request replay reuses its receipt, altered/out-of-order
  requests fail, and no paid execution or synthetic human approval is enabled.
- The real 75-dispatch rehearsal completed: four knowledge questions and one
  three-step repository, five conditions, three repeats. All fifteen no-op coding
  executions fail real hidden checks. Sixty knowledge packets remain genuinely
  pending. Execution validity is true; full harness/official/quality gates remain
  false. Review export validates all bindings and accounting independently.
- Integration found and fixed the child diagnostic's `id`/`item_id` mismatch.
  A concurrent build then correctly triggered binary-change fencing. Bootstrap
  now copies executable bytes privately before dispatch, retaining strict hashes.
  The final required test deliberately replaces a test-owned upstream executable
  after the first task; all 75 executions still complete using the frozen copy.
- Final test: two passed, zero skips, 428 seconds. Evidence is retained at
  `/data/tmp/memory-r3-rehearsal-cli-en49ld/pack/run/`; log:
  `/tmp/r3-rehearse-e2e-build-replacement.log`. Earlier schema-failure artifacts
  remain at `/data/tmp/memory-r3-integrated-f7Gdk2/pack/run/`. CI now requires the
  complete rehearsal; remote CI execution is not claimed.
- Required-database workspace tests and workspace Clippy passed during this
  increment (`/tmp/r3-current-workspace.log`, `/tmp/r3-current-clippy.log`); later
  report diagnostics have scoped evaluator regression evidence above. Formatting
  and boundaries pass. No existing deployment, customer data or paid provider
  was used. Timing includes setup/grading, fake vectors are not embedding parity,
  and these synthetic agents cannot establish model quality or customer ROI.

## 2026-09-15 cache-partition knowledge family

- Added a third development scenario: five source documents and four canonical
  draft rubrics covering wrong-space answer reuse, scope versus signed origin,
  obsolete instructions, containment and refusal of unsupported exposure totals.
  This fictional incident does not describe Memory's deployed implementation.
- Canonical rubric validation and both representative materialization/CLI tests
  pass. Default rehearsal packs now contain twelve knowledge questions and four
  coding sequences. Seven development and all twenty-five held-out knowledge
  families, plus twelve held-out coding families, remain to be materialized.

## 2026-09-15 execution validity versus pending review

- Reports now expose execution validity separately from the complete harness
  gate and count pending knowledge reviews. The existing review validator checks
  this distinction; no error-string filtering or automatic approval is used.
- The 56 evaluator tests passed. Targeted regressions also verify that missing
  usage, modified answer bindings and changed snapshots remain execution failures
  even when genuine semantic judgments are pending. The quality gate is unchanged.

## 2026-09-15 representative per-task execution

- Added composition of the real prepared-service/baseline/agent/grader/scorer
  components into campaign-shaped observations. The required executable test
  passes all five knowledge conditions and a three-stage Rust no-op control,
  with unchanged cloned snapshots, pending semantic judgments and rejected
  source tampering. No made-up observations or passing human ballots are used.
- Added per-stage public input files to sequence isolation. They cannot replace
  prepared fixture/submission files; current prompts are not disclosed early or
  carried into subsequent steps. Both required sequence tests pass.
- These are synthetic excerpt/no-op agents and fake vectors. Rehearsal timing
  includes setup/hidden grading and is not product latency. Complete controller/
  CLI integration, real preparation parity and paid mediation remain open.

## 2026-09-15 reusable prepared-service sessions

- Added a trusted-host snapshot session that prepares once and runs each callback
  against a fresh scoped foreground service/clone. Callbacks receive source
  bindings and a fixed-project query function, not credentials. Query access
  closes at callback completion; instances stop before their databases release.
- The required executable test passes: two independent isolated answers receive
  identical context from different clones of one unchanged snapshot; concurrent
  use, unknown projects and stale query closures are rejected. A failed callback
  is followed by a successful fresh instance. CI now requires this path.
- Normal owner shutdown cleans its disposable resources. Ambiguous exits retain
  staging and require reconciliation. This is a driver component, not automatic
  campaign/controller crash recovery or production deployment isolation.

## 2026-09-15 representative campaign materialization

- The existing campaign contract now permits smaller whole-family smoke-only
  rehearsals, with both tracks in each populated split and all three repeats.
  Empty/mixed/oversized families are rejected. Non-smoke dataset requirements
  remain exactly 40/100 knowledge and 4/12 coding; smoke evidence cannot be
  promoted through approvals. A new report regression proves that distinction.
- Added deterministic private materialization of all currently implemented
  representative development families, or one family per track for fast driver
  checks. Public contexts, source code, private rubrics and hidden graders remain
  separate; every campaign input is hash-locked. Existing paths are never merged
  or overwritten. The real CLI plans both existing suites and rejects tampering,
  absent held-out execution and attempts to relabel partial counts as official.
- The two executable materialization/CLI tests passed without skips. Complete
  driver dispatch and the full representative benchmark remain unfinished.

## 2026-09-15 isolated knowledge-answer integration

- Added an offline bridge from actual baseline results or exact prepared Memory
  version/project/hash bindings to the isolated answer process and Rust blinded
  adjudicator. Only selected source bytes and the question enter the process;
  private rubrics, unselected context and service metadata do not.
- Four required bridge tests pass across handover/lexical/fake-vector retrieval,
  no-memory refusal, invalid citations, malformed output and tampered bindings.
  Three required snapshot/service tests pass, including real sealed HTTP
  retrieval followed by isolated answering and an unchanged database fingerprint.
  CI now requires both paths without skips; remote CI execution is not claimed.
- No human ballots are synthesized. Valid quotations remain pending semantic
  review. Timings distinguish answer execution from adjudication preparation;
  neither claims complete task latency. Full campaign dispatch, production
  embedding parity, provider mediation and representative suite remain open.

## 2026-09-15 dependency-build coding family and grader correction

- Materialized the fourth development repository: five modules, two public
  tests and ten private probes across transitive ordering, dependency-map cache
  invalidation, cycles and atomic failed builds. Artifact generations represent
  a deterministic compiler test double. Labels remain draft.
- No-op controls exposed intermittent stdin EPIPE being classified as unavailable
  grading infrastructure. Early input closure now produces a failed submission;
  a 64-KiB unread-input regression passes. Other infrastructure failures remain
  fail-closed. All nine Rust/representative container test groups pass, zero skips;
  the fixture's two public Cargo tests also pass.
- Four development coding families are executable; twelve representative held-out
  families, full driver integration and approval gates remain open.

## 2026-09-15 configuration rollout coding family

- Assigned and materialized the third development repository: five modules,
  two ordinary public tests and ten hidden black-box probes across three stages.
  Covers overlay precedence, sticky deletion, optimistic revisions, validation
  and an atomic in-process reload test double. It is not infrastructure rollback.
- All four representative test groups passed with required Rust containers and
  zero skips: reference/cumulative execution passes, buggy/no-op controls fail.
  The two public Cargo tests passed. Labels remain draft; no held-out claim.

## 2026-09-15 complete condition diagnostics

- Reports now expose raw task/execution counts, successes, critical failures,
  spend, tokens and median latency for all five conditions, including no-memory.
  Repeats and sequence diagnostics do not inflate independent observations or
  task costs. Older serialized reports remain readable.
- All 55 evaluator tests and scoped all-target Clippy passed. The current gate
  remains unchanged. Olivier was asked whether baseline semantic failures should
  be reported separately from the Full Memory product gate; no approval has been
  assumed. Isolation failures remain fail-closed in every condition.

## 2026-09-15 staff-turnover recovery benchmark

- Materialized the planned second knowledge family: five public documents and
  four private rubrics plus a plain-language review sheet. The questions join
  restored data, retired generation 8/current generation 9 authority, receipt
  reconciliation, offline knowledge and unmeasured recovery/loss claims.
- The canonical rubric/required-dimension test now checks both development
  knowledge families and passed. No human decisions, held-out freeze or paid
  evaluation are implied. Complete representative allocation remains open.

## 2026-09-15 executable snapshot owner and foreground service

- Added a bounded trusted-host JSONL owner for synthetic source preparation,
  sealed clones, fingerprint verification, release and shutdown. It returns
  exact source/version bindings and temporary scoped credentials, never accepts
  arbitrary SQL/database names, and requires the explicit test database profile.
- Real executable tests prepare two projects, launch `memory service run` with
  a sealed clone and isolated configuration/home, verify bindings and denied
  cross-project queries, preserve fingerprints, reset readers and clean up.
  Invalid operations and source paths fail before database access. Both pass.
- The multithreaded executable exposed a close/return-to-pool race not seen in
  earlier single-threaded unit tests. Freeze now re-drains only its own pool for
  a bounded interval; the independent-preparer test still blocks freezing and
  leaves that connection alive. Five required-database support tests and scoped
  all-target Clippy passed. CI now requires the snapshot/service executable test.
- This helper seeds an offline lexical fixture only: no embeddings, complete
  campaign driver, model mediation or official quality evidence is implied.

## 2026-09-15 sealed HTTP authentication and current regression evidence

- A new real HTTP regression reproduced a 500 on the initial scoped-token
  request: authentication tried to update a sealed credential row. Evaluation
  now retains the protected audit receipt without mutating token/session
  timestamps. Normal authentication is unchanged; expiry still uses real time.
- The test passes with scoped tokens and browser sessions, cross-project/write
  denials, expired/unknown token denials, unchanged benchmark fingerprints and
  retained operational audits. Uses disposable PostgreSQL and fake credentials.
- Current required-database workspace tests (including federation regressions),
  workspace all-target Clippy, formatting and boundaries passed. All 90 Web tests
  and build passed. Lean build/tests passed with existing linter warnings; the
  lean CLI check passed with existing dead-code warnings. Web build retains its
  existing large-chunk warning. No paid providers or existing deployment changes.

## 2026-09-15 explicit interrupted campaign recovery

- `campaign run --resume` holds the original budget lock, validates the complete
  reserved prefix, reuses durable results and dispatches only unreserved tasks.
  It preserves earlier artifacts and creates a new resume report directory.
  Missing receipts, altered requests, unknown prefixes and uncertain outcomes
  stop before new work. Complete receipts can repair a missing acknowledgement.
- The real CLI completed all 660 synthetic development dispatches. Repeated
  resume, a missing final settlement and a missing final result were exercised;
  the driver invocation count stayed at 660. This is protocol/recovery evidence
  with synthetic observations, not a complete isolated quality campaign.
- 232 CLI and 55 evaluator tests passed serially. The first parallel run had an
  unrelated cache-home environment-sensitive CLI test failure; it passed on the
  serial rerun. Scoped Clippy, formatting and boundary checks passed. CI requires
  the real runner test. Paid execution remains disabled.

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
  - [x] Prepare once, fork read-only PostgreSQL/service instances, verify snapshots
    before/after work, and deny cross-project reads and stale query closures.
  - [x] No-network synthetic answer/coding containers and separate hidden Rust
    grading; real 75-dispatch representative offline rehearsal.
  - [ ] Complete the provider-backed isolation/egress profile.
    - [x] Fixed-destination bounded HTTP, request/receipt codecs and private
      prepared model calls, including isolated coding and private scoring tests.
    - [x] Canonical provider/tokenizer/embedding configuration contract and shared
      prepared-call bindings with distinct response/embedding pricing.
    - [x] Explicit v2 full-context reservations and multi-call task headroom;
      fake-provider settlement, uncertainty and pre-egress denial tests.
    - [x] Approval-gated secret/provider owner connected to disposable services
      and isolated task execution, verified with synthetic credentials only.
    - [ ] Select and verify the real model/tokenizer/price/egress configuration;
      synthetic counters and fake HTTP receipts are not live-provider evidence.
- [ ] Equal-information document, lexical, vector and full-Memory baselines.
  - [x] Existing baseline executables and real Memory retrieval integrated offline.
  - [x] Supplied embedding vectors through sealed Memory services and a shared
    synthetic embedding function, with chunking differences recorded explicitly.
  - [ ] Real embedding preparation and frozen full-Memory configuration parity.
- [x] Synthetic development/held-out knowledge and coding draft review pack.
  - [x] Four representative development coding families, executable controls.
  - [x] Complete knowledge families (10/10 development, 25/25 held-out) and
    held-out coding families (12/12), with source-first contracts and private rubrics.
  - [x] Full held-out fixture/reference/cumulative/negative-control verification.
    Label approval remains a separate unchecked gate below.
- [ ] Campaign plan/run/report, provenance, limits and fail-closed gates.
  - [x] Multi-call journal/result verification, exact private billing metadata,
    cache-price arithmetic, timing separation and separate calibration approvals.
  - [x] Global preparation/execution request, token and spend accounting;
    durable preparation receipt, snapshot binding and fail-closed resume.
  - [x] Shared provider-only preparation/task lifecycle, profile-derived snapshot
    and baseline embeddings, and isolated knowledge/coding execution without
    synthetic fallback; verified with fake providers only.
  - [x] Shared controller persists attempts/receipts, fences competing/crashed
    owners and provides metadata-only inspection. Provider cancellation retains
    uncertain charges. Complete launcher interruption tests now also cover abort,
    SIGTERM and SIGKILL; automatic paid-session resume remains unsupported.
  - [x] Complete the approved live-provider driver path, locked runtime packaging
    and fake-provider full execution/interruption rehearsal. Ordinary arbitrary
    paid drivers stay blocked. The v2 profile is implemented, not approved for
    an actual campaign or verified against billable provider responses.
- [ ] Offline fault/isolation/statistical tests and integration evidence.
  - [x] Real partial CLI rehearsal, pending review export, source/hash fencing,
    independent snapshots and negative coding controls.
  - [x] Full 660-dispatch development rehearsal with zero provider calls,
    unchanged sealed snapshots and 600 pending semantic review packets.
  - [ ] Complete provider-mediated fault/limit tests and the full benchmark run.
    - [x] Full 660-task mediated development rehearsal with 960 simulated calls;
      prepared-call faults, counter failures and isolated workflows tested separately.
    - [ ] Verified real-provider profile and official approved campaign evidence.
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
