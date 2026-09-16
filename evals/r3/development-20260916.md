# R3 development calibration — interrupted, not quality evidence

## Authorized scope

Olivier approved this run on 2026-09-16: manifest
`40c6f8dd554cb2f6a52575a87133a31edce8003c25a99b969e14dcb48f6b9d3c`,
artifact-origin knowledge and configuration-rollout coding, five conditions and
three repeats (75 tasks). The maximum was US$10 including preparation, 500 calls
and ten million tokens. No retries, top-ups or held-out execution were authorized.

The frozen pack and original journals are retained at
`/data/tmp/r3-calibration-20260916/`. The launcher used the approved dedicated
OpenBao credential and a fresh disposable database, not the dev-service corpus.

## Outcome

**Incomplete: zero of 75 tasks produced accepted observations.** Corpus
preparation completed; the first task, configuration-rollout / vector / repeat 1,
failed locally after its query embedding and before any answer-model request.
The pinned report command rejects the artifact as invalid R3 evidence. This is a
harness failure, not an incorrect model answer or evidence that Memory is worse
than another condition. No quality, cost-comparison, latency or ROI claim follows.

| Recorded usage | Requests | Input tokens | Accounted micro-USD |
| --- | ---: | ---: | ---: |
| Corpus preparation | 37 | 2,910 | 82 |
| First-task query embedding | 1 | 10 | 1 |
| Answer/agent model | 0 | 0 | 0 |
| Total settled provider receipts | 38 | 2,920 | 83 |

The journal's rounded-up usage estimate is **US$0.000083**, not an independently
verified provider invoice. Every per-call reservation has a settled receipt.
The incomplete task still has a conservative US$3.821184 reservation in the
outer dispatch ledger; that reservation is **not a charge** and has not been
deleted, refunded or converted into a successful task. The controller marks the
task uncertain because no complete task receipt exists. No new paid calls were
made after the launcher stopped.

## Reproduced cause

The neutral embedding binding hashes binary64 vector values. The JavaScript
owner sends float32-normalized components promoted to binary64 as JSON. Rust's
`serde_json` decoder without `float_roundtrip` changed nine of 1,536 components
in a retained vector. The integrity check correctly rejected the resulting
different bytes. Constant `0.5` fake vectors did not exercise this boundary.

Offline evidence:

- The original pinned retriever rejects retained decimal vectors but accepts
  an exact-binary control. No provider is called by this reproduction.
- A standalone `serde_json` 1.0.149 diagnostic finds nine changed components
  without `float_roundtrip` and zero with it.
- Enabling `float_roundtrip` for `mem-eval` makes the rebuilt retriever accept
  the retained vectors without weakening text, profile or vector digest checks.
- A synthetic cross-language regression fails against the original pinned
  executable and passes against the rebuilt one; deliberately changed vectors
  remain rejected. Rust additionally checks 4,096 promoted-float32 round trips.

Focused validation: 75 evaluator tests, three JavaScript retriever tests,
evaluator Clippy, formatting and boundaries pass. The launcher test's fake
vectors now use varied decimal components. See the
[execution ledger](../../docs/reviews/federation-first/r3-execution.md) for the
complete post-fix rehearsal status and retained paths.

## Next gate

The original campaign and pinned binaries are immutable evidence. Do not patch
them, clear their fences or invoke paid resume. A corrected runtime changes the
manifest identity and needs a separately approved replacement campaign. Retain
the settled usage when setting its cap; do not silently restore the full $10.
The broader benchmark labels, official run and blinded result review remain open.
