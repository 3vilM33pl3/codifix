# Full-context reservation profile (v2)

This is a conservative accounting mode, **not an exact tokenizer**, model
recommendation, paid-run approval or proof of provider billing behavior. It
avoids assuming that local text tokenization exactly represents hidden Responses
message/schema framing. Actual usage still comes from validated provider receipts.

The reviewed profile currently accepts only the dated
`gpt-5.4-mini-2026-03-17` snapshot and `text-embedding-3-small` at 1536 dimensions.
The provider profile must explicitly set `version: 2` and
`input_accounting: "full_context_reservation"`. Version 1 retains its original
canonical bytes, local-counter interface and task-budget behavior. New semantics
cannot be attached to version 1 silently.

Before every Responses call, reserve **400,000 input tokens**, the model's full
documented context capacity, plus the configured maximum output. This deliberately
over-reserves: output already shares the context window. Whole-message framing
cannot exceed the capacity of an accepted model request. Bodies remain text-only,
tool-free, non-streaming, without history or automatic truncation, and are capped
at 128 KiB of serialized JSON. Embeddings reserve **8192 input tokens**, accept one
text at a time, and additionally cap that text at 8192 UTF-8 bytes. Oversize inputs
fail rather than being truncated. These are shared limits across conditions.
[Model limits](https://developers.openai.com/api/docs/models/gpt-5.4-mini),
[embedding limits](https://developers.openai.com/api/docs/guides/embeddings)

The pinned standard rates, reviewed on 2026-09-16, are $0.75 input / $0.075 cached
input / $4.50 output per million answer tokens and $0.02 per million embedding
tokens. The implementation rejects unreviewed models, aliases, rates and context
ceilings. A future price/model change requires a reviewed code/profile update.
Only the standard global endpoint is eligible; no batch, regional or priority
pricing is inferred. Newer models' cache-write charges are not supported by this
profile.
[Answer prices](https://developers.openai.com/api/docs/models/gpt-5.4-mini),
[embedding price](https://developers.openai.com/api/docs/models/text-embedding-3-small)

The legacy `responses_tokenizer` / embedding `tokenizer` slots pin the reservation
implementation under the explicit IDs `full-context-responses-v1` and
`full-context-embeddings-v1`. They do not assert tokenization. The trusted owner
must verify those pinned bytes equal the reviewed `context-reservation.mjs`;
caller-supplied counter callbacks cannot override this mode. Each request journal
payload includes `input_accounting`, making the accounting interpretation explicit.

Confirmed receipts release unused reservation and record actual input, cached
input and output (including reasoning) usage. Unknown, malformed or lost receipts
retain the full reservation and stop subsequent calls. Replays reuse confirmed
results without billing again. If the provider violates the documented context
or usage contract, the attempt becomes uncertain; this is not an absolute billing
guarantee against provider defects or account-level charges.

For v2, whole-task reservations multiply one call's allowance by the manifest's
`provider_budget.max_calls_per_task`. This includes any task query-embedding calls
in the same call cap. Per-call receipts retain the original model limits. The
outer campaign and inner request journals therefore agree on enough headroom for
a three-step coding sequence. Global request/token/spend caps remain authoritative.
Unused headroom is released only after confirmed results. V1 uses its historical
single-call-sized aggregate task envelope unchanged.

For example, with 4096 maximum output tokens and 12 calls, a task initially needs
4,849,152 token headroom and $3.821184 of available budget. That is a reservation,
not a forecast of consumption: actual short tasks should settle for much less.
A $10 campaign can stop early when less than one full task reservation remains,
even if the next task would probably be inexpensive. Do not automatically raise
limits or switch to approximate accounting to get past that stop.

Tests use the real durable Rust budget mediator with fake provider responses.
They establish reservation/settlement and failure behavior, not live model
compatibility, answer quality or invoice reconciliation. Paid dispatch uses the
[approved runtime launcher](paid-launcher.md) and still requires independent
label/model/spend approval; packaging a profile supplies none of those approvals.
