# R3 engineering evaluation — implementation in progress

The existing generator produces **smoke tests**, not the official benchmark.
They check the machinery, not Memory's product value. Generated manifests set
`smoke_only = true`; older manifests without this field also default to smoke-only.
Changing labels to reviewed cannot make them official evidence. No provider
credentials are needed to build them.

The replacement benchmark is being designed around
[representative incident and repository workflows](representative-benchmark.md).
All ten development knowledge families are materialized: forty draft questions,
each family with five source documents, a plain-language review sheet and four
canonical private rubrics. They cover supplier handover, staff-turnover recovery,
cache partition, schema transition, capacity pressure, civil-time windows,
artifact origin, retention/holds, usage accounting and DNS cutover. Browse their
review sheets under [representative/development](representative/development/).
The source packs include current and superseded instructions, partial observations
and explicit evidence limits. They are distinct development mechanisms, not
renamed held-out cases. The first held-out knowledge family,
[telemetry units](representative/held-out/telemetry-units/review.md), is materialized
with four draft questions. A second,
[cursor snapshot](representative/held-out/cursor-snapshot/review.md), covers tied
sort values, mutable live ordering and scoped continuation; 23 families remain.
No labels are approved and none of these fixtures measures model quality alone.

The first multi-file coding family is also executable:
[durable importer](representative/development/durable-importer/review.md).
Its three cumulative stages have ten private black-box probes. The reference
passes; the buggy starter and no-op controls fail. Restart uses an explicitly
documented durable-store test double, not a claim of disk recovery. Run it with
the pinned Rust image using the isolation instructions and
`node --test evals/r3/representative.test.mjs`. CI requires this test without skips.

The second coding family is [evidence publication](representative/development/evidence-publication/review.md):
five modules, three cumulative stages and ten private probes for reviewed bytes,
version-scoped correction attribution, revocation and request replay. Its reference
passes while the buggy starter and no-op controls fail. Both families are draft
development benchmarks, not held-out approval or a complete representative suite.

The third coding family is [configuration rollout](representative/development/configuration-rollout/review.md):
five modules, two public tests and ten private probes across overlay precedence,
deletion/revision semantics and atomic in-process reload. Its reference and
cumulative sequence pass; the buggy starter and no-op controls fail each stage.

The fourth development coding family is [dependency build](representative/development/dependency-build/review.md):
five modules, two public tests and ten private probes covering transitive build
order, dependency-aware invalidation, cycles and atomic cache replacement. The
reference and cumulative sequence pass; starter and no-op controls fail. All four
development repositories are now executable drafts; the twelve representative
held-out repositories are being implemented. The first,
[framed stream](representative/held-out/framed-stream/review.md), has passing
private reference/cumulative checks and failing starter/no-op controls. The second,
[interval calendar](representative/held-out/interval-calendar/review.md), covers
canonical half-open sets and bounded free-window queries with the same controls;
ten remain. Run `node --test evals/r3/held-out.test.mjs` with the pinned Rust image.
These are deterministic fixture checks, not held-out model calibration or label
approval. The offline controller integrates
development fixtures using synthetic agents; provider-backed execution remains
unfinished.

Materialize those representative development fixtures for the existing campaign
CLI with `node evals/r3/build-representative-pack.mjs NEW_DIRECTORY`. This creates
a hash-locked, private, **smoke-only development rehearsal**: forty knowledge questions
and four coding sequences, no held-out data. `--one-family-per-track` creates a
smaller four-question/one-sequence rehearsal. Neither changes the required
official suite sizes or can be promoted by changing its label status. The CLI
can plan these packs; the controller below provides synthetic offline dispatch.
Live provider-backed execution remains unfinished. Placeholder suite commands
are contracts, not installed agent/grader programs.

`node evals/r3/build-representative-pack.mjs NEW_DIRECTORY --held-out` explicitly
materializes only the currently available held-out fixtures, separately from
development. The real planner loads them with `--split held-out`; it refuses
development planning on that pack. This is for deterministic fixture verification
and reviewer preparation, not model calibration, approved labels or paid runs.
The integrated controller below remains development-only. Changing flags cannot
promote partial or draft held-out material into official evidence.

### Integrated offline rehearsal

`node evals/r3/rehearse.mjs NEW_DIRECTORY --one-family-per-track` runs that small
representative pack through the existing campaign CLI. Supply these explicit
test-only environment settings (never a customer or existing dev database):

- `MEMORY_LAYER_TEST_REQUIRE_DB=1` and `MEMORY_LAYER_TEST_DATABASE_URL` for a
  disposable PostgreSQL administrator capable of creating temporary databases.
- Absolute `MEMORY_R3_CLI_BIN`, `MEMORY_R3_SNAPSHOT_BIN`, `MEMORY_R3_SCORE_BIN`
  and `MEMORY_R3_RETRIEVER_BIN` paths to the built Memory and example executables.
- Reviewed immutable local `MEMORY_R3_TEST_IMAGE` and `MEMORY_R3_RUST_IMAGE`
  IDs from the isolation instructions. There are no automatic registry pulls.

The bootstrap creates a new private pack, copies/hash-locks the actual controller
and driver modules, privately copies/pins executable bytes, prepares one snapshot and dispatches
75 tasks using fresh service clones and isolated agents/graders. Dropping the
selection flag includes all currently materialized development families. The
controller's Unix socket is private to the host operator and is never mounted
in an agent. Replayed identical requests reuse an in-process receipt; altered
requests or out-of-order work fail. The existing CLI keeps durable dispatch
reservations/results and refuses ambiguous automatic repeat execution.

The agents are deterministic excerpts/no-op controls; vectors are fake. No
provider credentials are inherited and no paid calls occur. A complete run keeps
`run/artifact.json`, the CLI's report, and blank blinded packets in `run/review/`.
Knowledge judgments remain pending, so the public report is deliberately **not
valid quality evidence**. The controller may finish successfully while recording
the CLI's nonzero pending-review status. It separately validates that no other
evidence defect is being hidden by pending review. No approval is synthesized.

Keep failed private artifacts and `private/run-diagnostic.txt`. Starting a new
rehearsal is not resuming an uncertain old one. This controller does not recover
an interrupted prepared snapshot after its process dies. Normal shutdown removes
owned services, clones and IPC; forced termination can require exact-resource
reconciliation. Existing services are not restarted or configured. Timing includes
setup and hidden grading and is not customer/model latency. This remains an
offline integration harness, not the complete approved/paid R3 experiment.

```sh
node evals/r3/build-review-pack.mjs target/memory-evals/r3-review
node evals/r3/verify-review-pack.mjs target/memory-evals/r3-review
memory eval campaign plan --manifest target/memory-evals/r3-review/campaign.json
```

Use a new output directory each time; generation refuses to overwrite material.
The output separates `public/` source/fixture content from `private/` rubrics,
reference solutions, seed input and suite/hidden-test definitions. Never mount
`private/` into an execution agent. The generator deliberately leaves labels
`draft`, model identifiers `pending`, and paid budget zero.

## What is implemented

- 40 development / 100 held-out knowledge questions, grouped into 10 / 25
  distinct fictional scenarios. Each includes one unsupported-impact question.
- 4 development / 12 held-out three-step coding sequences. These are small,
  single-function Rust maintenance exercises, not large-repository tasks.
- Forty-eight executable reference graders; source and label hashes; existing
  mem-eval suite compatibility; a review document and explicit label caveats.
- Pure indexed-document, fixed BM25 and cosine-vector comparator functions in
  `mem-eval::campaign::baselines`. Embeddings are supplied by the caller; no
  provider or Memory ranking is silently substituted for a baseline.
- `r3_retriever` example executable connects those comparators to the existing
  external-retriever JSON protocol. See the adapter contract below; this is
  context retrieval, not a common answer model or complete campaign driver.
- Campaign identity/order/approval contracts and fail-closed statistical reports.
- Dispatch-boundary checks reject changed manifests/assets after the final task
  as well as between tasks. Invalid model, usage, timing, rubric and isolation
  evidence stops the runner before it starts another task; valid negative quality
  results remain measurable. Live provider-backed isolation remains unfinished.
- `campaign plan` and `campaign report`. `campaign run --offline --allow-shell`
  can dispatch a separately reviewed, hash-locked isolation driver and retain
  reservation/observation artifacts. The representative offline controller above
  is implemented; **the official provider-backed driver remains unfinished**.

## Remaining integration, not approval-dependent

The [knowledge bridge](isolation/knowledge.mjs) now joins actual baseline adapter
results or prepared Memory version bindings to isolated offline answering and
Rust blinded adjudication. A disposable-service test verifies that path with an
unchanged sealed database. It uses synthetic agents and fake vector inputs, not
real model/provider execution, complete campaign dispatch or quality evidence.

The [offline container component](isolation/README.md) now provides bounded,
network-disabled execution of explicitly supplied files, with a real Docker
boundary/reset/timeout test. The representative offline controller now connects
it to service snapshots, retrieval adapters and independent graders. Full
provider-backed R3 isolation remains an open gate.

The same component now executes three-step agent/private-grader handoffs with
explicit source-only carryover and no grader feedback returned to agents.
Positive, negative, deleted-file recovery and fresh-reset tests run in real
containers using inert JSON submissions. It also supports dependency-free,
multi-file Rust binaries with host-held black-box expectations in a pinned
Rust/Node image. Existing suite-runner integration and official campaign results
remain unfinished.

`mem-test-support::IsolatedDatabase::freeze` and `FrozenDatabase::fork_read_only`
provide tested prepared-database clones for service integration tests. The
template refuses new connections; each clone uses a distinct restricted login.
Tests verify unchanged seeded reinforcement state after reset, denied benchmark
writes, denied cross-clone connections and acceptance by the service's seal
checker. The offline controller now uses that preparation/reset mechanism and
checks corpus fingerprints. It does not provide interrupted-controller snapshot
recovery or the complete provider-backed driver.

`mem_test_support::fingerprint::benchmark_fingerprint` now hashes all public
benchmark tables and their column shapes from a single normalized, read-only
snapshot. Only the four permitted operational journals are excluded; new tables,
reinforcement, embeddings and query activity are included automatically. A real
service-query test uses fake embeddings with the actual full-Memory retrieval
path, verifies seeded reinforcement ranking, and checks identical fingerprints
before/after queries and across independently sealed clones. It is a service
handler integration test, not HTTP authentication or a complete campaign.
The fingerprint utility is bounded for small disposable fixtures, and does not
prove permissions, indexes, functions, external evidence files or backup recovery.

- Complete the official isolation profile beyond the current trusted-host
  offline services and network-disabled agent/grader containers.
- Complete real vector preparation and full-Memory configuration parity; the
  integrated rehearsal deliberately uses fake vectors and a lexical snapshot.
- Enforce request/token/spend limits at every provider dispatch in a budget
  mediator, including coding sequences and uncertain calls. The current runner
  reservation is only a conservative single-request foundation; paid dispatch is
  rejected unconditionally before any driver starts.
- Complete provider-backed campaign dispatch and genuine operator adjudication.
  The offline controller already exports bound pending review packets; it does
  not decide semantic correctness or approve labels.
- Finish representative datasets and broader provider-mediated crash/isolation
  fault tests. Interrupted campaigns must remain fail-closed with retained
  evidence; automatic restoration of a killed controller's snapshot is not an
  additional R3 completion requirement. A completed partial offline rehearsal is
  not the full R3 benchmark or official evidence.

## Per-request reservation journal

`mem_eval::campaign::budget::RequestBudget` is a tested boundary for a trusted
campaign dispatcher. Create its journal in a private campaign directory, outside
all agent mounts. Each reservation records a bounded request ID, payload digest,
input/output token allowances and micro-USD allowance. The manifest identity and
limits are bound to the journal header. The file is exclusively locked for the
owner's lifetime and fsynced before `reserve` or `dispatch` permits a call.

Confirmed usage releases only unused token/cost allowances; request count is
never refunded. An error, invalid usage receipt or process termination retains
the full allowance and blocks further requests until explicit reconciliation.
Used IDs cannot dispatch again, even with identical inputs. Torn journals,
changed limits/campaigns and competing owners fail closed. Metadata journals do
not store questions, model outputs, credentials or provider error bodies.

The existing single-request `campaign run` dispatcher now writes
`dispatch-budget.jsonl` before launching each driver, and settles it only after
validating usage, isolation, locked inputs and the preparation receipt. Failed or
invalid dispatches remain unresolved. Its legacy `ledger.json` remains a
conservative sum of maximum dispatch allowances; the journal records confirmed
usage separately. Neither artifact is authority to replay an interrupted run.
This does not remove the single-request restriction or unconditional paid-run
block, nor mediate calls inside an external driver. The complete driver must calculate
conservative allowances from locked model settings/prices, enforce task-level
limits as well as campaign limits, mediate every provider path, persist results,
and reconcile confirmed receipts. The journal does not independently verify
provider billing or provide an approval mechanism. Asynchronous cancellation
between reservation and settlement must remain uncertain, never automatically
refund or retry. The trusted host must protect the journal from replacement.

`RequestBudget::create_scoped` / `resume_scoped` additionally enforce the same
per-task request/token/spend ceilings across all steps in a sequence. Every call
must name its task; an unscoped call cannot bypass the limit. Confirmed refunds
update both ledgers, while uncertain calls block the entire dispatcher. Restart
reconstructs task totals and rejects changed limits. `Allowance::for_model`
calculates rounded-up charges from locked token prices and rejects excess token
bounds/overflow. These APIs are tested foundations for multi-call mediation;
the current campaign driver restriction is not lifted by their availability.
They do not impose wall-clock deadlines, verify tokenization or account for
non-token provider fees. Those remain responsibilities of the complete mediator.

## Reusable dispatch results

`ProviderResults` stores bounded outputs in a private directory, separately from
the budget journal. Completed receipts bind campaign, task, request, exact input
digest, reservation and usage; the journal records their digest, not their body.
Exact completed retries reuse output. Changed retries, missing/corrupt receipts
and uncertain calls cannot trigger automatic provider execution. Explicit
`recover` can settle a complete result persisted before a dispatcher crash,
without making a call; it cannot refund a missing result or infer provider usage.

The existing offline campaign dispatcher now persists validated observations in
`dispatch-results/` before settling its scoped reservation. This is durable
driver-result recovery. Explicit `campaign run --resume` reopens the same `--out`
and holds its budget journal lock throughout recovery and new dispatches. It
validates the entire saved prefix first, reuses complete receipts (including a
crash before settlement), and starts only previously unreserved tasks. Changed
inputs, gaps, missing results or ambiguous attempts stop before new dispatch.
The original start/plan and earlier reports remain intact; a new `resume-UUID/`
directory holds the reconstructed artifact, ledger and report. Keep the output
at its original absolute path because request identities bind that path.
An orphan pre-reservation work directory currently requires operator inspection;
it is never overwritten. This does not mediate every internal provider call.
The single-request restriction and paid-run block remain.
Keep result files outside agent mounts and protect the trusted host from other
writers. Local receipts establish persistence, not provider authentication or
billing truth. New result receipts are bounded to 2 MiB.

## Baseline adapter contract

### Executable snapshot owner

`cargo build -p mem-test-support --example r3_snapshot --locked` builds the
trusted-host JSONL fixture owner. Use **only a disposable PostgreSQL cluster**
through `MEMORY_LAYER_TEST_DATABASE_URL` and `MEMORY_LAYER_TEST_REQUIRE_DB=1`.
It runs migrations and creates/drops owned databases and temporary login roles;
it is not for the dev database, customer instances, or production recovery.

One process accepts bounded requests on stdin:

- `prepare`: a `semantic_time` and `documents` array with `project`, relative
  `source`, and `text`. The caller must verify these against locked source assets
  first. Preparation seeds reference records and scoped tokens transactionally,
  then freezes the database and returns source/version bindings and a fingerprint.
- `fork`: creates a sealed clone with an independent database login and returns
  its opaque `id`, `database_url`, project API tokens and fingerprint.
- `fingerprint` / `release`: act only on an `id` owned by that process.
- `shutdown` or EOF: cleans up owned readers and the template. Stop owned services
  first. If the owner is killed or cleanup fails, reconcile its disposable cluster.

Keep stdout private: it contains temporary credentials. Never mount the owner,
its admin connection or control channel into an execution agent. At most sixteen
clones can be live. Input is bounded to 1,024 documents and 8 MiB of text. No
arbitrary SQL, database names or external source paths are accepted by the protocol.

The executable regression launches the actual `memory service run` with a sealed
clone, isolated configuration/home and no provider credentials; verifies source
bindings and denied cross-project access; checks unchanged fingerprints; then
releases and resets clones. It does **not** provide embeddings, representative
labels, model mediation, container/network isolation or an official campaign.
The preparation reply explicitly marks this as an offline lexical fixture.

Build `cargo build -p mem-eval --example r3_retriever --locked`. Invoke
`target/debug/examples/r3_retriever PREPARED_CONFIG.json` with the existing
external-retriever request JSON on stdin. Lock the executable/configuration in
the campaign before use. Configuration fields are `project`, `condition`,
`mode` (`handover`, `lexical`, `vector`), prepared `documents` (`id`, `text`),
`embeddings` for vector mode, `top_k` and the common context `bytes` limit.
Project and condition must match exactly; handover refuses truncation.

The adapter never follows the request's `context.fixture_path`, rejects nonempty
`context.hidden_facts`, and returns selected texts with source-ID citations.
Errors omit query/configuration contents. It makes no provider calls. Vector
mode requires `query_embedding` on the request and fails if missing; the legacy
external runner does not supply this extension. Future campaign mediation must
generate/account for that embedding using the locked provider. Tests use explicit
fake vectors only, never evidence of vector retrieval quality. The common answer
model, equal-information corpus preparation and campaign integration remain open.

## Statistical interpretation

### Knowledge adjudication

`mem_eval::campaign::scoring` provides private, bounded review packets with
factual, required-evidence and appropriate-refusal criteria. Citations identify
an exact source asset and a verbatim quote. The scorer checks those bytes against
the frozen source; semantic entailment and sufficient evidence still require
human judgment. A valid quote alone never establishes a correct answer.

`r3_score prepare INPUT.json NEW_DIRECTORY` creates `private/evidence.json` and
`review/{packet,ballot}.json`. Build with
`cargo build -p mem-eval --example r3_score --locked`. The input contains
`manifest_sha256`, `work`, `rubric`, `sources` and a structured `answer`; these
contracts are in `campaign/scoring.rs`. Use a trusted host, never an agent mount.
Give the reviewer only `review/`, without condition/model/repeat or automated
scores. The private salted execution binding prevents transferring a ballot
between observations while hiding condition identity. Answer wording can still
compromise blinding; this is not a claim of perfect anonymity.

Each human reviewer replaces `pending-reviewer` with their operator-recorded ID,
addresses every criterion with `pass`, `fail` or `unresolved`, and gives a reason.
Collect ballots as a JSON array, then run:

```sh
target/debug/examples/r3_score score \
  NEW_DIRECTORY/private/evidence.json BALLOTS.json
```

Output contains the assessment and adjudicated evidence for the observation's
`knowledge_review` field. No ballots, model-only diagnoses, incomplete judgments
and reviewer conflicts cannot pass. Conflicts remain unresolved regardless of
ballot order; retain the originals and explicitly resolve the disagreement with
the reviewers, rather than discarding dissent. The utility refuses to overwrite
an existing preparation directory or replace ballots in adjudicated evidence.
It does not authenticate humans: the operator must collect genuine decisions
and retain their provenance. A model must never label itself a human reviewer.

Official knowledge reports reject missing evidence and mismatched campaign,
execution, answer, rubric, source or score bindings. Supplied reviews are also
validated offline. Old smoke artifacts remain readable without a review but are
not official evidence. Label files use compact JSON in `Rubric` field order and
one trailing LF (`rubric_bytes`); their raw SHA-256 is locked in manifest assets.
The first supplier-handover family's four machine-readable rubrics are drafted
under its `private/` directory. They remain unapproved, not frozen labels.

For a complete campaign execution, export and apply reviews with the CLI:

```sh
memory eval campaign review --manifest CAMPAIGN.json --artifact RUN/artifact.json --out REVIEW
memory eval campaign review --manifest CAMPAIGN.json --artifact RUN/artifact.json --ballots BATCH.json --out ADJUDICATED
```

Give reviewers only `REVIEW/packets/` and `REVIEW/ballots/`. The trusted operator
collects ballots in `REVIEW/batch-template.json`'s `updates` array; each entry has
`ballot` and `replaces_sha256` (null for a new reviewer). Keep the batch's original
`artifact_sha256`. Applying creates `ADJUDICATED/artifact.json` and `report.json`
without touching the source or invoking a provider. Outputs must be new; private
directories/files are created with modes 0700/0600 on Unix.

For amendments, export from the latest adjudicated artifact and supply the exact
canonical SHA-256 of the reviewer's previous ballot in `replaces_sha256`. This is
the compact serialized `Ballot` contract, including sorted judgment keys. The
old ballot is retained in `knowledge_review.history`, bound to its replacement;
another reviewer's dissent cannot be removed. Retain all source artifacts and
operator provenance. Model diagnoses must never be relabeled as human decisions.

Pending human judgments no longer abort otherwise valid task execution. The run
retains pending evidence and completes remaining work; its final report still
fails closed until judgments are resolved. Campaign review validates all other
execution evidence first and cannot repair an interrupted campaign, missing
usage or failed isolation. Use explicit execution resume first if it is safe;
review cannot resolve an uncertain provider attempt.
Test ballots are explicitly synthetic and confer no benchmark label approval
or product-quality evidence.

### Campaign uncertainty

Each track includes raw diagnostics for all five conditions: independent task
count, successful/total executions, critical failures, execution spend, token
total and median latency. Repeats remain executions, not extra independent tasks;
sequence children are not added again. The no-memory diagnostic is visible even
though it is not an informed-baseline superiority comparison. This reporting
does not change the current critical-failure gate.

Reports average repeats within tasks and resample complete scenario/repository
clusters. Ten thousand deterministic paired bootstrap draws give a 95%
simultaneous band using the maximum absolute centered deviation over the three
informed-baseline comparisons, separately per track. This is approximate
bootstrap inference, not an exact finite-sample guarantee. A small coding set can
produce inconclusive results; do not enlarge its apparent sample size by counting
steps or repeats independently.

Report flags distinguish harness validity, official evidence and product-quality
pass. Offline reports never pass product quality. Unknown costs, missing or
duplicate tasks, altered snapshots, configuration/model mismatches, unresolved
rubrics and uncertain charges invalidate the campaign. Old EvalRun files remain
readable through existing commands but lack sufficient R3 evidence on their own.

## Measurement service

For a separately provisioned disposable database only:

```toml
[evaluation]
semantic_time = "2026-09-10T00:00:00Z"
[service]
read_only = true
[offline]
enabled = false
[cluster]
enabled = false
```

Keep reinforcement ranking enabled if it is part of the locked configuration.
Measurement mode disables learning independently. It does not run migrations or
bootstrap identities; prepare these before sealing. Use a non-owner login with
SELECT on benchmark tables and no role memberships, schema CREATE or privileged
attributes (including replication). No public-schema sequence USAGE/UPDATE is
allowed: `SELECT nextval(...)` would otherwise change the frozen state.
Only `external_effects`, `external_effect_outcomes`,
`foreground_executions`, and `auth_audit_events` may receive operational DML.
Security-definer functions in the public schema are rejected. A privileged
preparation process must not continue writing after the snapshot is sealed.

The semantic clock affects ranking/decay only. Authentication, lease expiry and
duration measurement continue using real clocks. This config does not by itself
prove network isolation, snapshot reproducibility or service-wide suitability.

## Human gates

Olivier must review factual labels, alternative wording, citation/refusal policy
and coding difficulty before any held-out freeze. Subsequent development and
official spending approvals are separate and hash-bound. Do not alter held-out
cases or gates in response to an unfavorable official result. Synthetic results
cannot establish customer ROI.
