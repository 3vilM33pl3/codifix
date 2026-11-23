# R3 phase-aware development attempt: ambiguous output

## Outcome

The single explicitly approved replacement ran and stopped with **zero completed
benchmark tasks**. The pinned report rejects the incomplete/missing observations:
`execution_valid`, `harness_valid`, `official_evidence` and `quality_passed` are
all false. No further paid evaluation call, automatic retry or top-up followed.

Manifest: `/data/tmp/r3-phase-fixed-20260916/campaign.json`, identity
`df12ab2b403b0a18ca82d1c1515671c4ab5dd0a89df751c2cf2f998012ac4e86`.
Olivier's “yes” approved US$8.65 / 305 requests / 8,350,086 tokens, recorded at
`2026-09-16T16:31:30Z`. Runtime and exact approval preflight passed before launch.
The same reviewed development families, models and per-task limits were retained.

## Observed failure and limits of diagnosis

Preparation completed. The first real model response decoded and settled:
2,961 input tokens, 2,398 output tokens, 13,012 micro-USD. The second response was
`completed` but failed `receipt_output` with **`output_reason: multiple_messages`**.
This is not a quota, transport, output-token-limit or numeric usage rejection.

In this frozen decoder, that branch means an earlier non-commentary answer was
already selected and a second final or unphased message was encountered. The
second message was not marked commentary: that would produce `phase_order`.
The branch does not establish whether either message's text was valid JSON, which
phase labels were present on both candidates, or which message was intended as
the answer. Those details were not retained. Do not retrospectively claim that
the response contained two valid JSON answers or that choosing the last message
would be correct. Explicit commentary-plus-final fixtures still pass, but that
fix alone did not make this live output acceptable.

Private evidence: `run/work-00000/provider/failure-<payload-digest>.json`, its
`budget.jsonl`, the first settled result and the incomplete artifact under the
frozen root above. Report check: `/tmp/r3-phase-live-report.log`. Rejected raw
output was not retained; the shape cannot be reconstructed from these records.

## Offline diagnostic follow-up

Added bounded structural diagnostics for future `receipt_output` failures: at
most 16 items with closed-vocabulary type/phase/status/role, bounded content-kind
lists/counts and capped text byte lengths. This shape contains no text, message
IDs, reasoning summaries, refusal text, annotations, provider extra fields or raw
error bodies; the existing private response-identifier diagnostic is unchanged.
Returned diagnostics are defensively copied. The shape does not authorize a
result, settle usage or choose among ambiguous messages.

Fourteen receipt/request/diagnostic tests and 23 transport/mediator/billing tests
pass with zero skips. They distinguish duplicate explicit finals from duplicate
unphased output, verify no private text is serialized, bound oversized shapes,
and verify persisted failure metadata leaves usage uncertain and fences another
dispatch. Logs: `/tmp/r3-shape-tests.log`, `/tmp/r3-shape-integration.log`.
Formatting, diff checks and boundary checks pass. This local diagnostic change
does not alter the frozen campaign or recover its missing details.

Do not launch another complete campaign merely to discover another response
shape. Further work needs an explicitly bounded diagnostic strategy and approval
for any new provider calls; do not silently choose a candidate, drop usage,
change models, broaden output acceptance or substitute offline observations.

## Spending reconciliation and cleanup

This attempt settled 39 calls (38 embedding, one model): 5,881 input tokens,
2,398 output tokens and **13,095 micro-USD**. One model call remains unresolved,
retaining its full 416,384-token / 373,728-micro-USD allowance.

Across all six attempts, provider journals contain **230 settled calls**
(228 embedding, two model), **23,442 input + 6,079 output tokens**, and
**32,296 micro-USD (US$0.032296)**. Five unresolved model allowances retain
**2,045,056 tokens / 1,702,752 micro-USD (US$1.702752)**. Combined conservatively
accounted usage/allowances: **US$1.735048**, not a verified invoice. Nested task
reservations are not additional charges. The incomplete report has no task
observations and is not a replacement for the provider journals' spending data.

After launcher exit, removed only `memory-r3-phase-live-20260916`, the owned
disposable database. All frozen packs, receipts, diagnostics and replay fences
remain. Existing dev services, project data and credentials are unchanged.
R3, human adjudication and official/product-quality gates remain open.
