# R3 engineering evaluation — implementation in progress

The existing generator produces **smoke tests**, not the official benchmark.
They check the machinery, not Memory's product value. Generated manifests set
`smoke_only = true`; older manifests without this field also default to smoke-only.
Changing labels to reviewed cannot make them official evidence. No provider
credentials are needed to build them.

The replacement benchmark is being designed around
[representative incident and repository workflows](representative-benchmark.md).
Those designs are drafts, not implemented or approved fixtures.

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

- Implement and verify disposable snapshot preparation/reset and per-condition
  service/agent/grader containers with denied cross-condition/hidden-test access.
- Wire baseline adapters and the existing suite runner into that driver, including
  vector preparation and the actual full-Memory retrieval path.
- Enforce request/token/spend limits at every provider dispatch in a budget
  mediator, including coding sequences and uncertain calls. The current runner
  reservation is only a conservative single-request foundation; paid dispatch is
  rejected unconditionally before any driver starts.
- Connect semantic/citation/refusal rubrics to independent grading and blinded
  adjudication. Existing substring scoring is diagnostic only.
- Run complete offline campaigns and crash/isolation fault tests. Reference
  solution tests alone do not establish these properties.

## Statistical interpretation

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
attributes. Only `external_effects`, `external_effect_outcomes`,
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
