# Supervisor model adapter

The first adapter is an offline-testable, **stateless, text-only** OpenAI
Responses profile, with a separately tested one-shot HTTP adapter. It does not
start supervisors or confer permission to disclose a collection. Model suggestions are not actions
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

`transport::OpenAiTransport::from_environment` reads only `OPENAI_API_KEY` from
the explicitly prepared process environment. It has no credential-file discovery,
R3 fallback or arbitrary endpoint configuration. No secret is written to disk or
passed to Pi. Production endpoints are fixed to `https://api.openai.com/v1`.
HTTP retries, redirects and inherited proxies are disabled. The private alternate
base URL is used only by in-crate synthetic localhost tests.

`execute` needs a currently claimed, authorized action and input already approved
for this model destination. It rechecks common area policy before and after
counting, reserves the exact request digest/cost in PostgreSQL, commits `begin`,
and consumes its one-shot send permit. Response reads are bounded to 64 KiB and
the lease deadline. Non-2xx responses, missing usage and transport ambiguity
retain reservations; errors contain fixed classifications, never provider bodies.
Authoritative usage is committed even after revocation. Over-bound settlement
revokes policy in the store; current authority is rechecked before returning
a proposal. These local checks cannot recall a dispatched request or substitute
for current source eligibility and collection/model-destination consent.

The lower-level `execute` returns a proposal without marking work complete.
`execute_and_persist` adds an exact project/attempt binding at publication and atomically stores
the private structured proposal, ordered hashes of supplied evidence, its digest
and the Verified action outcome. Recovery reads use the store's exact
area/project/action scope and do not repeat the model call. A lost response before
persistence remains unfinished/uncertain; known billing is still retained. Neither
API claims that an in-memory suggestion is a durable completed decision.

Workers must retain the reviewed source/version-to-input mapping independently:
evidence-byte hashes bind indices but are not source identities or proof of truth.
Current source eligibility, project existence/authorization and disclosure
consent remain caller obligations **before dispatch**. The current action/attempt
schema binds area, not project; passing a project UUID to this trusted wrapper
does not establish pre-send project authorization. A scheduler, Pi admission proxy, service authorization and
Web/TUI views remain integration work. No paid provider result is claimed from
these tests. The intended $10 fleet allocation requires exactly one authoritative
$2.50 ledger per area and a verified instance-to-area binding. Duplicate/cloned
ledgers are not detected by these APIs; deployment and recovery must fence stale
writers before enabling provider access. This is not a distributed fleet-cap
proof or a cap on other applications using the normal shared key.

Focused tests use a required disposable PostgreSQL database and fake HTTP
providers. They prove begin-before-send accounting, duplicate denial, no hidden
redirect/rate-limit retries, uncertain reservations, billing of invalid answers,
result denial after revocation, durable private recovery and wrong-project/area
read denial. No real credential or provider is used.
