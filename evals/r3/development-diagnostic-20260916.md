# R3 diagnostic retry and usage-schema repair

## Instrumented attempt

Olivier requested diagnosis and another attempt. The replacement
`/data/tmp/r3-diagnostic-retry-20260916/campaign.json`, identity
`d424e71ad19473266d7c3305574a9b73491bcf67b55b2fb1fcab719aa53fe8c9`,
ran once with US$9.68, 423 requests and 9,590,064 tokens remaining. These caps
preserved all previous settled usage and the full unresolved model-call allowance.
No original campaign, receipt or replay fence was changed.

Preparation succeeded; the first coding task again stopped with zero accepted
observations. This time the metadata-only diagnostic established:

- The API returned a **completed** response from the expected model.
- Its required numeric token fields passed the decoder's consistency and bounds
  checks; the subsequent exact **usage-shape** check rejected it.
- A private provider response identifier was retained. No prompt, response text,
  provider error body or credentials were added to diagnostics.

The diagnostic is at `run/work-00000/provider/failure-<payload-digest>.json`.
This attempt proves a local receipt-compatibility failure, not an authentication,
network or model-answer-quality failure. It does not establish the exact extra
field present: that version of diagnostics records the stage, not raw usage.
A read-only GET for the response returned 404 (the request used `store:false`),
so no receipt or charge was recovered. There was no additional generation call.

## Narrow compatibility fix

The current [Responses API reference](https://developers.openai.com/api/reference/typescript/resources/responses/methods/create)
includes `input_tokens_details.cache_write_tokens`, including zero-valued
examples. The old decoder rejected that documented shape. A synthetic regression
reproduces this incompatibility without any paid call.

The decoder now accepts that optional field **only when exactly numeric zero**.
Old receipts without the field still work. Nonzero, negative, fractional, null,
boolean/string values and unknown billing fields remain rejected: no unpriced
cache writes can disappear from accounting. Existing pricing, benchmark inputs,
models and limits per task are unchanged. The live diagnostic is consistent with
this documented incompatibility; the exact extra field in the failed response
cannot be recovered retrospectively.

Twenty focused receipt/call/mediator tests pass. The full fake-provider launcher
fixture now includes the documented zero field, so a complete rehearsal tests
the new shape rather than repeating the older incomplete fake response schema.
The preceding diagnostic-only runtime passed a separate complete 75-task fake
rehearsal and HTTP rejection test; those are not paid quality evidence.

## Usage and proposed replacement

The instrumented attempt settled another 38 embedding calls, 2,920 input tokens
and 83 micro-USD. Its model-call reservation of 318,432 micro-USD is unresolved.
Across all three attempts: **114 settled embedding calls, 8,760 tokens and
249 micro-USD**, plus **two unresolved model calls**, each reserving 404,096 tokens
and 318,432 micro-USD. Combined settled-plus-full-uncertain allowance is
637,113 micro-USD (US$0.637113), not a verified invoice. Outer task reservations
are nested allowances, not further provider charges.

The disposable live database was removed after launcher exit. All frozen packs,
diagnostics, receipts and fences remain. Existing dev services were not changed.

Olivier explicitly approved the usage-fixed replacement on 2026-09-16, recorded
at `2026-09-16T14:27:28Z`: same models and reviewed families, **US$9.36, 384 calls
and 9,183,048 tokens**, no automatic retry or top-up. This remains below the
original combined ceiling even if both unresolved model allowances are consumed.
Manifest: `/data/tmp/r3-usage-fixed-20260916/campaign.json`, SHA-256
`91524471c8c325b4e3c4e296ed1ee6f481063f0a0fe00abfaac2244e91236098`.
Runtime verification and offline validation precede launch; outcome is separate
from approval. R3 and official/product-quality gates remain open.

## Approved usage-fixed run: explicit output-limit failure

The updated-schema full fake-provider rehearsal passed all 75 tasks, with valid
execution evidence and 60 pending human reviews. Evidence:
`/data/tmp/r3-paid-MZpLZW/pack/`, log `/tmp/r3-cache-write-launcher.log`.
Formatting and boundary checks also passed. This fake run does not demonstrate
measured model quality. The exact runtime and approval preflight passed, then
the approved usage-fixed live launcher ran once on its own disposable database.

Live preparation succeeded, but the first coding call returned
**`status: incomplete`, `incomplete_details.reason: max_output_tokens`**. The
model reached the approved 4,096 output-token ceiling. The decoder correctly
rejected this truncated response before checking usage shape; this attempt does
**not** establish live acceptance of the cache-write fix. Zero of the 75 tasks
produced accepted observations. The pinned report rejects the artifact as
incomplete/invalid R3 evidence. No automatic retry or limit increase occurred.

The run's private diagnostic is under
`/data/tmp/r3-usage-fixed-20260916/run/work-00000/provider/` and records the safe
status and reason. Its per-call journal retains the uncertain model reservation.
Report-check output: `/tmp/r3-usage-fixed-report-check.log`.

This fourth attempt settled another 38 embedding calls / 2,920 input tokens /
83 micro-USD. Across all four attempts: **152 settled embedding calls, 11,680
tokens and 332 micro-USD (US$0.000332)**. Three unresolved model-call allowances
remain, totalling **955,296 micro-USD (US$0.955296)** and 1,212,288 tokens. Including
settled usage, the conservatively retained amount is US$0.955628, not an invoice.
There are 155 reserved call identities, not 155 confirmed provider executions.

The owned live and test databases were removed after their processes exited;
all frozen artifacts and reservations remain. Existing dev services and secrets
are unchanged.

### Next decision at the end of the usage-fixed attempt

Another live attempt needs revised output/deadline settings and explicit approval,
not replay of any frozen pack. The current medium-reasoning coding call exhausted
4,096 output tokens. Review a larger allowance (for example 16,384) together with
the model-call deadlines: the current implementation also has a 30-second coding
call deadline and a 10-second knowledge-call default underneath the 300-second
task cap. More tokens alone does not guarantee completion before those limits.
Keep container-command limits separate from model-call time, apply identical
settings across conditions, and test changes offline before another paid run.

Under current conservative accounting, a future cap of US$9.04 / 345 calls /
8,776,032 tokens would remain below the original combined ceilings. This is a
proposal, **not approval or a prepared spending receipt**. Unresolved calls cannot
be marked free, missing observations cannot be dropped, and these development
attempts establish no quality, superiority, cost/latency or ROI result.

## Approved expanded run: completed output rejected

Olivier subsequently approved the proposed expanded profile: 16,384 maximum
output tokens, 120-second model-call deadlines, unchanged 300-second task cap,
US$9.04 / 345 calls / 8,776,032 tokens. No model, benchmark, rate or reasoning
change. The immutable replacement is `/data/tmp/r3-expanded-20260916/`, manifest
`3cf114f9abd409eab22966d7cc5a1a92bda6d826dc85e168db9687cba2bbd4d4`.
Approval was recorded at `2026-09-16T14:53:09Z`; exact runtime and approval checks
passed before the single launch.

Eleven focused deadline/receipt/agent tests and the full 75-task fake-provider
rehearsal passed first. Fake evidence: `/data/tmp/r3-paid-fS4a4x/pack/`, logs
`/tmp/r3-limits-focused.log` and `/tmp/r3-limits-launcher.log`.

Live preparation passed. The **first model response was accepted and settled**:
2,961 input tokens, 3,681 output tokens and 18,786 micro-USD. This verifies a real
successful traversal of the usage/output decoder. The second response was
`completed` but rejected at **`receipt_output`**, after identity, model and usage
validation. No task completed. The pinned report correctly returns
`execution_valid: false`, `harness_valid: false`, `quality_passed: false`, with
interrupted/missing-observation errors. This is not evidence of model quality.

The retained diagnostic cannot distinguish multiple messages, refusal, message
shape/status, annotations, oversized text or absent text. Raw rejected output was
not retained. Do not claim a specific cause or silently relax output validation.
The subsequent local change splits these branches into closed-vocabulary
`output_reason` metadata without accepting new shapes or retaining provider text.
Twelve receipt/diagnostic/transport tests pass, with zero skips, including all ten
output-reason categories and unknown-field non-disclosure. Log:
`/tmp/r3-output-diagnostics-tests.log`. This change is **not** applied to the frozen
run and cannot recover its missing diagnostic detail. No further paid call ran.

### Final accounting and cleanup

This attempt settled 39 calls (38 embedding, one model): 5,881 input tokens,
3,681 output tokens and 18,869 micro-USD. Its second model operation retains the
full unresolved 416,384-token / 373,728-micro-USD allowance.

Across all five attempts, the provider journals contain **191 settled calls**
(190 embedding, one model), **17,561 input + 3,681 output tokens**, and
**19,201 micro-USD (US$0.019201)**. Four unresolved model-call allowances total
**1,628,672 tokens / 1,329,024 micro-USD (US$1.329024)**. The combined conservative
amount is **US$1.348225**, not an invoice. Nested task allowances are not extra
charges. The incomplete campaign report lacks the interrupted task's observations
and therefore does not replace these provider journals for spending reconciliation.

The owned live database was removed after launcher exit; frozen packs, private
diagnostics, receipts, replay fences and reservations remain. Existing dev
services and credentials were not changed. Another paid invocation is not
authorized. Before proposing one, resolve the strict output-profile compatibility
and improve fixture coverage offline; the next step is not another blind retry.
R3 implementation/evidence, human adjudication and official/product-quality gates
remain open.
