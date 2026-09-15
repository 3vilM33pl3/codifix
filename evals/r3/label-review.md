# R3 benchmark label review — not a paid-run approval

No commands or spending are required for this review. You are deciding **what
counts as a correct answer**, not whether Memory has passed an evaluation.

The draft contains 140 knowledge questions and 16 three-step coding sequences
across 51 distinct families. All data is fictional. Synthetic harness passes do
not establish model quality or customer ROI.

## What to check

1. Are these useful engineering tasks rather than arbitrary puzzles?
2. Do the expected answers follow from the public sources, including which
   instructions are current and what the evidence does **not** establish?
3. Are reasonable alternative answers/implementations accepted?
4. Are citations, refusals and hidden coding checks fair for all five conditions?

Each family link below opens a plain-language review sheet. Its adjacent
`public/` material supplies the facts; `private/` contains the scoring contracts
and, for coding, hidden checks. These review/private files must never enter an
answering agent's context. Development and held-out families remain separate.

You can report changes by family and question/step number. Do not approve merely
because the offline tests pass: those tests verify mechanics and reference
implementations, not the usefulness of every label.

## Scope of an approval

The reviewed fixture version is Git tree
`198d202a976298539e51219da0b015b6cb9658cb`, under
`evals/r3/representative/` (last fixture commit `370eba7`).

After reviewing, an explicit response can be:
“Approve the benchmark labels for that fixture tree only; no paid runs.”

That is separate from choosing provider/model/tokenizer settings, approving a
maximum development spend, and approving the final locked held-out campaign.
Nothing in this file records approval or changes draft labels. Final campaign
approval records must match the exact locked manifest; changing it requires
matching approvals.

## Development knowledge

- [artifact origin](representative/development/artifact-origin/review.md)
- [cache partition](representative/development/cache-partition/review.md)
- [capacity pressure](representative/development/capacity-pressure/review.md)
- [dns cutover](representative/development/dns-cutover/review.md)
- [quota accounting](representative/development/quota-accounting/review.md)
- [retention hold](representative/development/retention-hold/review.md)
- [schema transition](representative/development/schema-transition/review.md)
- [supplier handover](representative/development/supplier-handover/review.md)
- [timezone window](representative/development/timezone-window/review.md)
- [turnover recovery](representative/development/turnover-recovery/review.md)

## Development coding

- [configuration rollout](representative/development/configuration-rollout/review.md)
- [dependency build](representative/development/dependency-build/review.md)
- [durable importer](representative/development/durable-importer/review.md)
- [evidence publication](representative/development/evidence-publication/review.md)

## Held-out knowledge

- [alert denominator](representative/held-out/alert-denominator/review.md)
- [archive expansion](representative/held-out/archive-expansion/review.md)
- [cancellation scope](representative/held-out/cancellation-scope/review.md)
- [certificate chain](representative/held-out/certificate-chain/review.md)
- [cgroup pressure](representative/held-out/cgroup-pressure/review.md)
- [circuit probes](representative/held-out/circuit-probes/review.md)
- [cohort assignment](representative/held-out/cohort-assignment/review.md)
- [color profile](representative/held-out/color-profile/review.md)
- [cursor snapshot](representative/held-out/cursor-snapshot/review.md)
- [distribution notice](representative/held-out/distribution-notice/review.md)
- [lock inversion](representative/held-out/lock-inversion/review.md)
- [null filter](representative/held-out/null-filter/review.md)
- [numeric overflow](representative/held-out/numeric-overflow/review.md)
- [quantization error](representative/held-out/quantization-error/review.md)
- [rename durability](representative/held-out/rename-durability/review.md)
- [reproducible order](representative/held-out/reproducible-order/review.md)
- [stream framing](representative/held-out/stream-framing/review.md)
- [telemetry units](representative/held-out/telemetry-units/review.md)
- [token audience](representative/held-out/token-audience/review.md)
- [token boundary](representative/held-out/token-boundary/review.md)
- [trace link](representative/held-out/trace-link/review.md)
- [traffic drain](representative/held-out/traffic-drain/review.md)
- [unicode identity](representative/held-out/unicode-identity/review.md)
- [vector compatibility](representative/held-out/vector-compatibility/review.md)
- [watcher rename](representative/held-out/watcher-rename/review.md)

## Held-out coding

- [bounded decoder](representative/held-out/bounded-decoder/review.md)
- [circuit window](representative/held-out/circuit-window/review.md)
- [decimal register](representative/held-out/decimal-register/review.md)
- [framed stream](representative/held-out/framed-stream/review.md)
- [identifier catalog](representative/held-out/identifier-catalog/review.md)
- [interval calendar](representative/held-out/interval-calendar/review.md)
- [notice bundle](representative/held-out/notice-bundle/review.md)
- [path policy](representative/held-out/path-policy/review.md)
- [route precedence](representative/held-out/route-precedence/review.md)
- [sensor matrix](representative/held-out/sensor-matrix/review.md)
- [snapshot pages](representative/held-out/snapshot-pages/review.md)
- [structured redaction](representative/held-out/structured-redaction/review.md)
