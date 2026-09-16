# R3 paid evaluation: remaining gates

Status checked 2026-09-16. This is a readiness record and **proposal**, not
approval, an official result, or a claim that paid execution is enabled.

## Gate status

| Gate | Status and evidence |
| --- | --- |
| Dedicated evaluation credential | Provisioned in the approved OpenBao KV entry, version 1. No key in the repo or `.env`. |
| Provider authentication | A read-only `GET /v1/models` returned HTTP 200 with the dedicated key. This establishes authentication/model visibility, not billing credit or inference compatibility. |
| Approval preflight | `memory eval campaign check-approvals` verifies the manifest, assets and matching label/spend records without provider calls. Success does not enable dispatch. |
| Secret/provider owner | Connects the version-pinned key handoff to fixed API destinations, rechecking approvals before every call and cancelling bindings on exit/failure. Tests use fake secrets and mocked OpenAI responses. |
| Input accounting | Explicit [v2 full-context reservations](context-accounting.md) avoid unverified guesses about message/schema framing. Durable settlement/denial/replay tests pass. This is not exact tokenization or live billing validation. |
| Real provider configuration | Reviewed v2 model/rates/limits implemented; campaign selection remains unapproved. Pin implementation and evidence into the actual calibration manifest; verify live wire/usage compatibility only after spend approval. |
| Live launcher | Credential/provider owner connects to the disposable campaign session and supports cancellation with retained uncertain charges. Shared IPC dispatch now persists attempts/results and fences crashed owners; SIGKILL and duplicate/lost-response tests pass. The complete approved command-line launcher, locked runtime packaging and whole-launcher recovery rehearsal remain open; the paid CLI guard stays enabled. |
| Benchmark labels | Awaiting Olivier's review of the [label review pack](label-review.md). No labels frozen or approval files fabricated. |
| Development spend | Awaiting approval of the exact locked manifest and cap; candidate scope below. |
| Official campaign | Separate approval after calibration fixes operational budgets and selects the cost/latency reference baseline. No official calls authorized or made. |

The existing offline artifact and its limitations are recorded in the
[execution ledger](../../docs/reviews/federation-first/r3-execution.md).
Fake-provider passes are harness evidence, not measured Memory quality.

## Proposed first calibration, not an authorization request yet

Use one complete development knowledge family (four questions) and one complete
three-step coding family, all five conditions and three repeats: **75 task
executions**, not 75 independent observations. Proposed families:
[artifact origin](representative/development/artifact-origin/review.md) and
[configuration rollout](representative/development/configuration-rollout/review.md).
No held-out tasks, customer facts, dev-service corpus or host agent history.

Candidate answer/agent model: **`gpt-5.4-mini-2026-03-17`**, medium reasoning,
identical settings across conditions. Its dated snapshot is documented and was
visible to the dedicated key. Published standard text rates are $0.75 input,
$0.075 cached input and $4.50 output per million tokens. This is a reproducible,
cost-bounded starting candidate, not a finding that it is the best evaluator.
[Official model specification](https://developers.openai.com/api/docs/models/gpt-5.4-mini)

Candidate embeddings: **`text-embedding-3-small`**, explicitly 1536 dimensions,
same provider/model/dimensions for vector baseline and Full Memory; document
preparation and task queries accounted separately. Published input price is
$0.02 per million tokens. The documented encoding is `cl100k_base`.
[Model pricing](https://developers.openai.com/api/docs/models/text-embedding-3-small),
[embedding guide](https://developers.openai.com/api/docs/guides/embeddings)

Proposed maximum: **US$10 of provider usage**, including preparation, no automatic
retries or top-up; at most 500 provider calls and ten million total tokens.
The proposed v2 profile reserves 400,000 input / 4,096 output tokens per call, with
at most 12 provider calls per task and 300 seconds per complete task. These are
starting operational limits to validate offline, not promises that every task
will finish. Whole-task reservations cover all 12 calls and need 4,849,152 token
headroom / $3.821184 remaining before a task starts. Actual usage releases the
unused amount after settlement. This replaces the earlier proposed two-million
token cap, which cannot fit even one such task reservation; the $10 spending
proposal is unchanged. Stop and retain incomplete evidence when any bound is hit.
Taxes/account billing and unknown in-flight charges are not a guarantee supplied
by the local usage ledger. Do not retry ambiguous charges automatically.

Before asking for spend approval, build the actual locked campaign, verify its
provider/tokenizer and isolation path, and show its SHA-256 plus exact limits.
The first billable calls then belong to that approved development calibration;
model-list access alone does not prove those calls will succeed. Do not create
approval JSON from this proposal or from approval of secret storage.

## Accounting choice and remaining live verification

Responses input includes roles, message boundaries and structured-output schema
formatting. Plain-text tokenization alone is not exact; OpenAI exposes a separate
input-count endpoint for complete requests. The current R3 counter callback is
deliberately local/synchronous. Do not hide an unmediated HTTP request inside it.
The implemented v2 alternative reserves full documented model capacity, then
settles actual validated usage. No additional counting endpoint is invoked.
This deliberately trades tighter preflight estimates for conservative budget
headroom; it does not pretend to measure the exact input before transmission.
[Official token-counting guidance](https://developers.openai.com/api/docs/guides/token-counting)

The newer GPT-5.6 Sol/Luna documentation includes cache-write pricing that the
current R3 input/cached-input/output accounting does not represent. Do not switch
to those models merely because the account lists them. Such a change needs an
explicit price/usage contract and tests; absence of a field in the current codec
does not imply zero cost.
[Sol specification](https://developers.openai.com/api/docs/models/gpt-5.6-sol),
[Luna specification](https://developers.openai.com/api/docs/models/gpt-5.6-luna)

## What Olivier can review now

The [review pack](label-review.md) explains what counts as a correct answer and
links every family. Its fixture tree remains
`198d202a976298539e51219da0b015b6cb9658cb` (last fixture commit `370eba7`), verified
against the current checkout on 2026-09-16. Feedback or approval of those labels
is independent of the model and spending decisions. Final approval records must
still bind the complete campaign manifest. No need to create or rotate a key again.
