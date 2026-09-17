# Supervisor model adapter

The first adapter is an offline-testable, **stateless, text-only** OpenAI
Responses profile. It does not start supervisors, read credentials, call models,
or confer permission to disclose a collection. Model suggestions are not actions
or approvals. The normal Memory credential was selected for reuse on 2026-09-17;
the R3 credential and spending authorization are excluded.

`model::PreparedRequest` fixes `gpt-5.6-sol`, standard service tier, low reasoning,
800 total output tokens (including reasoning), no tools, no conversation state,
no background execution and no response storage. Only the exact question and
explicitly reviewed evidence are included. The caller remains responsible for
current project, collection and destination consent; validation is not a content
scrubber or a prompt-injection proof. Inputs are never automatically drawn from
ordinary query prompts, history, environment files or local paths.

The official input-token counter must measure the identical instructions, input,
reasoning and response schema. Counting itself discloses input and therefore needs
the same current egress authorization as generation. Counts over 16,384 tokens,
malformed results or counts older than 120 seconds block generation. There is no
heuristic tokenizer fallback. An unavailable counter blocks model mode.

Reserve all input at $5/M (including potential cache writes) and output at $20/M,
without assuming a cache discount. The largest allowed reservation is $0.097920.
Actual usage charges $4/M ordinary input, $0.40/M cache hits, $5/M cache writes and
$20/M output, rounded upward once to a microdollar. Reasoning is already part of
output; it is not counted twice. Unknown usage retains the reservation. Billable
failures still return known costs independently from answer validity. A mismatch
against counted input or admitted output blocks content acceptance and requires
an incident review; long-context charges are accounted if a provider violates the
input bound. Prices expire at 2026-11-21 00:00 UTC pending a new review.

Sources reviewed 2026-09-17:

- [GPT-5.6 Sol model and pricing](https://developers.openai.com/api/docs/models/gpt-5.6-sol)
- [Input-token counting](https://developers.openai.com/api/reference/typescript/resources/responses/subresources/input_tokens/methods/count)
- [Responses request and usage contract](https://developers.openai.com/api/reference/cli/resources/responses/methods/create)

The eventual network adapter must obtain a committed physical-attempt permit from
`mem-supervisor-store` immediately before a single send. It must disable HTTP
retries, redirects, inherited proxies and provider fallback; bind the request
digest; keep credentials outside Pi; bound responses; and journal sanitized
outcomes. No paid provider or live Pi compatibility result is claimed by protocol
unit tests. Existing user Web/TUI interfaces are not yet connected to this crate.
