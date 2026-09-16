# R3 budget-bounded development continuation

## Authority and unchanged scope

Olivier requested: “Keep going until you run out of budget.” Checkpointed at
`2026-09-16T22:18:40Z`. This authorizes bounded development continuation within
the original **US$10 / 500 calls / 10,000,000 tokens**, not a new budget or an
official held-out run. Keep the approved two development families, models,
reasoning settings, scoring and isolation. Existing dedicated key is reused.
Each attempt gets a new pinned pack; no old fence or journal is cleared. Failed
attempts are inspected before another dispatch, and uncertainty stays reserved.

## Reconciled starting allowance

Read provider-level journals from the six prior packs and the one-call diagnostic
(excluding nested task reservations): 231 settled calls and five unresolved calls,
2,081,764 used/reserved tokens, 1,756,282 used/reserved micro-USD. Thus available:
**264 calls / 7,918,236 tokens / US$8.243718**. Uncertain reservations are not
confirmed charges; this is conservative accounting, not an invoice.

## Continuation 1

Pack: `/data/tmp/r3-continue1-20260916`, manifest
`ff0f115055302565a3eb82c12e67b1122627a2614b79f2b5a981f9b99570bcac`.
Caps equal the reconciled allowance above. Sources, labels and model match the
previously reviewed pack; all 29 nonconfiguration asset pins and runtime integrity
verify. The 75 development task executions remain planned, not completed evidence.
Dedicated disposable PostgreSQL uses loopback port 55439; no existing service or
data is modified. Strict output decoding remains unchanged. Further attempts may
only consume the re-reconciled remainder under the same user authorization.

The run stopped at the second coding call, with zero completed observations.
The structural diagnostic shows reasoning / explicit final / reasoning / explicit
final, both final texts 3,861 bytes. That does not establish equality; raw text
was not retained. The failure remains uncertain, not retrospectively settled.
This attempt settled 39 calls (38 embedding, one model), 10,719 tokens and
US$0.024075; one unresolved call retains 416,384 tokens / US$0.373728.

Remaining aggregate authority: **224 calls / 7,491,133 tokens / US$7.845915**.

## Exact-final compatibility repair

The local decoder now accepts multiple explicit finals only when every message
passes existing role/status/content/annotation/size checks and every text is
byte-for-byte identical. It emits a single action/answer, settles response-level
usage once, and records the duplicate count. Different text (even equivalent JSON
with different whitespace), unphased duplicates, trailing commentary, refusals,
malformed content and invalid JSON remain rejected. No model/prompt, source,
label, scoring or retrieval change. Old frozen runs remain untouched.

[Official phase guidance](https://developers.openai.com/api/docs/guides/latest-model?model=gpt-5.4)
distinguishes commentary from finals; it does not establish that these previous
finals were identical or promise repeated-final semantics. Exact deduplication is
our bounded compatibility rule, not a claimed upstream guarantee or permission
to choose the last answer. The retained length-only evidence motivated a test,
not a conclusion about the lost contents.

Forty focused receipt/request/billing/transport tests and four diagnostic safety
tests pass with no skips. The full 75-task fake-provider launcher rehearsal passes
with duplicate finals on every model response, one billing settlement per call,
unchanged isolation snapshots, 60 pending human reviews and denied restart.
Evidence: `/data/tmp/r3-paid-urbZEu/pack/run/`, logs
`/tmp/r3-exact-finals-{tests,diagnostic,launcher,boundaries}.log`.
Formatting and boundaries pass. These are synthetic harness results, not quality.

## Continuation 2

Pack `/data/tmp/r3-continue2-20260916`, identity
`461ccc8fae2ffe06dda6e07d91297640bdd434b9e40c1dd4dc5e81701968da14`,
is pinned with the exact-final repair and **224 calls / 7,491,133 tokens /
US$7.845915**. The original continuing-development authorization covers this
reconciled remainder. Same models, prompts, 29 reviewed nonconfiguration assets,
75-task order and isolation; runtime and approval gates pass. This pack has not
inherited any success or settled charge from the prior failed pack.

R3 and official/product-quality gates remain open; do not treat budget consumption
or a successful diagnostic as quality proof.

### Continuation 2 outcome and ordinary command-error repair

The exact-final repair was exercised live: one settled response records
`duplicate_final_messages: 1`. Five model calls were accepted, with one action
per response and no duplicated billing. The task then stopped after an action
selected `/usr/bin/cargo`, which does not exist in the pinned Rust image. The
guest rejected `spawn` ENOENT, causing the supervisor/campaign to abort. Reproduced
offline with that exact executable path; no further provider call was required.

The guest now returns bounded exit 127/126 feedback for ENOENT/EACCES, preserving
selected files so the agent can correct its command within the existing call and
time budget. Other spawn errors, timeout, excessive output, unsafe exports and
container failures still fail closed. No host fallback, PATH broadening, command
rewriting, added tool access or free model retry. Twelve container/agent/sequence
tests pass, including three-step hidden grading, error recovery, accounting,
missing/non-executable paths, timeout and boundary failures.
Four additional real-Rust grading tests pass. The exact missing `/usr/bin/cargo`
path now produces exit 127 on the pinned Rust image without losing selected files.
Logs: `/tmp/r3-guest-errors-tests.log`, `/tmp/r3-guest-rust-tests.log`;
formatting and boundary checks also pass.

All provider calls in this attempt settled: **43 calls**, 17,886 input + 21,096
output = 38,982 tokens, **US$0.098120**. No new uncertain provider allowance;
the task observation is nevertheless absent, so the campaign remains invalid.
Next available aggregate caps: **181 calls / 7,452,151 tokens / US$7.747795**.
Prior six unresolved allowances remain reserved.

## Continuation 3

Pinned pack `/data/tmp/r3-continue3-20260916`, identity
`8448ed7d1c1422c5aad846bcb1b0680fac3af569028d22a7a1a9e1158db34c7e`.
The validated guest-error repair is included; models and 29 reviewed source/label
assets are unchanged. Caps are **181 calls / 7,452,151 tokens / US$7.747795**,
under the continuing-development authorization. No prior pack is resumed.

### Continuation 3 outcome and task-window correction

The five-minute outer task deadline expired with the seventh model request in
flight. Retained cancellation and its full allowance; no successful task claimed.
Settled: **44 calls / 20,963 input + 38,684 output tokens / US$0.179056**.
Additional uncertain reservation: **416,384 tokens / US$0.373728**. Remaining:
**136 calls / 6,976,120 tokens / US$7.195011**. Seven older uncertain calls now
remain in the cumulative ledger.

The agent previously tracked its own time but not elapsed service/retrieval or
inter-step grading time. For live coding it now subtracts elapsed task time,
reserves 100 seconds of grading/finalization headroom, uses an absolute monotonic
deadline across steps, and starts a model call only if its full 120-second window
plus the 30-second command window fits. Retrieval calls count against the same
per-task call allowance. Exhausted execution returns an explicit failed submission
with known usage (including valid zero-model-call cases), not a fabricated success
or omitted result. This conservative allocation may produce negative coding
results under the original five-minute budget; report that operational limitation.
It does not guarantee every external/grading failure becomes recoverable.

A knowledge-only subset was considered but **not run**: the campaign validator
requires both tracks. The unapproved draft `/data/tmp/r3-knowledge-20260916` remains
unused. Retain the complete two-track plan and original limits instead of weakening
the validator or dropping coding failures. Subsequent runs may stop at the original
call budget before all planned tasks complete; missing observations remain invalid.

Verification: 13 container/agent/sequence tests, ten provider-session/agent tests
(including disposable PostgreSQL and both tracks), and six final agent tests pass;
no skips. Logs `/tmp/r3-live-window-{tests,integration,final,boundaries}.log`.
Formatting and boundaries pass. No live calls were made during this repair.

## Continuation 4

Pack `/data/tmp/r3-continue4-20260916`, identity
`6152f952c472810588fd9f81fef40f7ff2ef3f28d6a634c25b27798763f7da9b`.
Caps: **136 calls / 6,976,120 tokens / US$7.195011**, under the existing
continuation approval. Same 75 tasks, order, models, prompts, source/label assets
and five-minute deadline. Runtime pins and approval gates verify; the live-window
correction is included. The unapproved knowledge-only draft is not used.

The first coding task now produces a complete **failed** observation with known
usage rather than interrupting the campaign. The run subsequently reaches 12
completed task records. This is progress evidence only: the run is still active,
knowledge judgments require human review, and incomplete/capped execution cannot
pass the campaign or product-quality gate. Final accounting follows on exit.
