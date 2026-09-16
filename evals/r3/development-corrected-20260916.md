# Corrected R3 development calibration — interrupted

## Authorization and execution

Olivier explicitly replied “Approve corrected run”. The development receipt at
`2026-09-16T13:48:09Z` binds manifest
`169c57de207ca8ba46cf13443b9fccbfb7da65f51d1cd75c3eac39d6c45c9c9c`
to US$9.99, 462 requests and 9,997,080 tokens, including preparation. The reviewed
artifact-origin and configuration-rollout families, five conditions, three
repeats, model settings and 300-second task limit were unchanged. No automatic
retry, top-up or held-out execution was authorized.

The dev Memory checkpoint, pinned runtime/image checks and real approval
preflight passed. The pinned launcher ran once against a fresh loopback-only,
tmpfs-backed PostgreSQL database, using the approved dedicated OpenBao key.
Existing dev services, credentials and original campaign evidence were untouched.

## Outcome and diagnostic limits

**Incomplete: zero accepted observations out of 75 expected tasks.** Preparation
completed. The first configuration-rollout / vector / repeat 1 task completed its
query embedding and passed the corrected vector binding, then reserved its first
Responses-model operation without producing a settled receipt. The launcher
exited 1; no retry or extra paid diagnostic call followed.

This is not evidence of an incorrect answer or a failed product-quality gate.
There is no retained answer, response identifier, HTTP status or allowlisted
failure classification. The transport, provider owner and mediator intentionally
collapse exceptions, and unvalidated response bodies are not persisted. The
retained evidence therefore cannot distinguish an approval/transport failure,
provider rejection, or response-decoding/usage rejection. Do not infer that the
provider billed nothing, blame a specific API field, or claim the response codec
has passed live validation.

The next engineering step is offline-only, metadata-only failure diagnostics:
retain allowlisted stage/error categories and safe HTTP status/request identifiers
without credentials, prompts, outputs or provider error bodies. Such diagnostics
cannot recover the lost failure detail retrospectively. Reconcile the unresolved
call before any newly approved, separately pinned live compatibility attempt.
Do not edit this frozen runtime or weaken validation to make the run pass.

## Recorded usage and uncertainty

| Corrected attempt | Settled requests | Input tokens | Accounted micro-USD |
| --- | ---: | ---: | ---: |
| Corpus preparation | 37 | 2,910 | 82 |
| First-task query embedding | 1 | 10 | 1 |
| Total settled receipts | 38 | 2,920 | 83 |

One additional model operation retains an unresolved reservation of 400,000 input
tokens, 4,096 output tokens and **318,432 micro-USD (US$0.318432)**. Reservation
does not establish successful transmission or an actual charge. The parent task
also retains its US$3.821184 allowance in the outer dispatch ledger; that is a
nested conservative reservation, not an additional provider call or invoice.
Neither journal was cleared, refunded or rewritten.

Across the [original attempt](development-20260916.md) and this attempt there are
76 settled embedding calls, 5,840 input tokens and **166 accounted micro-USD
(US$0.000166)**, plus the one unresolved model operation. Including its full
per-call reservation gives US$0.318598; this is conservative local accounting,
not verified account billing. There are 77 reserved call identities across both
attempts, not 77 confirmed provider executions.

## Evidence and cleanup

Retained private root: `/data/tmp/r3-calibration-corrected-20260916/`.

- `campaign.json`, `private/labels-approval.json` and
  `private/development-spend.json`: exact authorization and frozen inputs.
- `private/dispatch/`: completed preparation, uncertain first task, durable
  relaunch fence. `inspect-dispatch.mjs` reports uncertainty with exit 2.
- `run/preparation/provider/budget.jsonl` and
  `run/work-00000/provider/budget.jsonl`: settled receipts and unresolved call.
- `private/launch-status.log`, `private/run-diagnostic.txt` and first-task
  `driver.stderr`: sanitized interruption evidence.
- `run/artifact.json`: `completed: false`, `offline: false`, zero observations.

The pinned report command fails with `invalid R3 evidence`, listing incomplete
campaign and missing observations. Its `uncertain_attempts: 0` and zero execution
cost summarize accepted observations only; they do **not** reconcile this
interrupted task's provider journal. Use the journals above for uncertainty and
partial usage, not the incomplete report totals. Read-only report-check output
is retained at `/tmp/r3-corrected-report-check.log`.

The owned disposable database is removed after the launcher exits. Its synthetic
working state is discarded; frozen inputs, receipts, reservations and artifacts
are retained. No ordinary dev/customer service is removed. R3, broader label
approval, blinded answer review and the separately authorized official campaign
remain open. No quality, cost comparison, latency comparison or ROI conclusion
is available from this attempt.
