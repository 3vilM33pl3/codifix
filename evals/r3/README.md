# R3 engineering evaluation — implementation in progress

The existing generator produces **smoke tests**, not the official benchmark.
They check the machinery, not Memory's product value. Generated manifests set
`smoke_only = true`; older manifests without this field also default to smoke-only.
Changing labels to reviewed cannot make them official evidence. No provider
credentials are needed to build them.

The replacement benchmark is being designed around
[representative incident and repository workflows](representative-benchmark.md).
The first multi-document development source pack and draft rubric are now
materialized at [supplier handover](representative/development/supplier-handover/review.md).
The complete replacement suite is not yet implemented or approved; that pack is
not wired into the smoke generator or an official campaign.

The first multi-file coding family is also executable:
[durable importer](representative/development/durable-importer/review.md).
Its three cumulative stages have ten private black-box probes. The reference
passes; the buggy starter and no-op controls fail. Restart uses an explicitly
documented durable-store test double, not a claim of disk recovery. Run it with
the pinned Rust image using the isolation instructions and
`node --test evals/r3/representative.test.mjs`. CI requires this test without skips.

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
  results remain measurable. This does not supply the missing isolation driver.
- `campaign plan` and `campaign report`. `campaign run --offline --allow-shell`
  can dispatch a separately reviewed, hash-locked isolation driver and retain
  reservation/observation artifacts. **No complete driver is shipped yet.**

## Remaining integration, not approval-dependent

The [offline container component](isolation/README.md) now provides bounded,
network-disabled execution of explicitly supplied files, with a real Docker
boundary/reset/timeout test. It is not yet wired to the campaign driver, service
snapshots, retrieval adapters or independent graders. Full R3 isolation remains
an open gate.

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
checker. This is an integration-test primitive, not yet the campaign's snapshot
orchestration, complete corpus fingerprinting or crash-recovery driver.

- Implement and verify disposable snapshot preparation/reset and per-condition
  service/agent/grader containers with denied cross-condition/hidden-test access.
- Wire baseline adapters and the existing suite runner into that driver, including
  vector preparation and the actual full-Memory retrieval path.
- Enforce request/token/spend limits at every provider dispatch in a budget
  mediator, including coding sequences and uncertain calls. The current runner
  reservation is only a conservative single-request foundation; paid dispatch is
  rejected unconditionally before any driver starts.
- Connect the implemented evidence-bound adjudication utility to the complete
  campaign driver and operator review flow. Existing substring scoring is
  diagnostic only; official knowledge observations now require independent
  review evidence, not just a driver's success boolean.
- Run complete offline campaigns and crash/isolation fault tests. Reference
  solution tests alone do not establish these properties.

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

## Baseline adapter contract

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

This utility does not yet collect/reconcile reviews across an interrupted full
campaign; that integration remains open. Test ballots are explicitly synthetic
and confer no benchmark label approval or product-quality evidence.

### Campaign uncertainty

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
