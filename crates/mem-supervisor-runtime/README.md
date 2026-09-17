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
for this model destination. Before counting it immutably binds the instance,
area, project, policy, action, complete request and ordered evidence hashes in
PostgreSQL. It rechecks that exact context and current authority before and after
counting, reserves the exact request digest/cost in PostgreSQL, commits `begin`,
and consumes its one-shot send permit. Response reads are bounded to 64 KiB and
the lease deadline. Non-2xx responses, missing usage and transport ambiguity
retain reservations; errors contain fixed classifications, never provider bodies.
Authoritative usage is committed even after revocation. Over-bound settlement
revokes policy in the store; current authority is rechecked before returning
a proposal. These local checks cannot recall a dispatched request or substitute
for current source eligibility and collection/model-destination consent.

The lower-level `execute` returns a proposal without marking work complete.
`execute_and_persist` retains that pre-dispatch project/attempt binding and atomically stores
the private structured proposal, ordered hashes of supplied evidence, its digest
and the Verified action outcome. Recovery reads use the store's exact
area/project/action scope and do not repeat the model call. A lost response before
persistence remains unfinished/uncertain; known billing is still retained. Neither
API claims that an in-memory suggestion is a durable completed decision.

Workers must retain the reviewed source/version-to-input mapping independently:
evidence-byte hashes bind indices but are not source identities or proof of truth.
Current source eligibility, project existence/authorization and disclosure
consent remain caller obligations **before dispatch**. The stored instance/project
binding prevents relabeling work after dispatch; it does not establish source
authorization. A scheduler, Pi admission proxy, service authorization and
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
read denial. Missing projects, wrong instance IDs, attempts to rebind a project's
input and revocation during token counting fail closed. No real credential or
provider is used.

## Pi request admission (not a running proxy)

`pi_admission::PiAdmission` supplies the exact single-message prompt and fixed
system instructions for an isolated pinned Pi process. Its validator rejects
requests over 256 KiB, malformed/duplicate JSON, unknown fields, changed inputs,
history, files/images, tool declarations, provider storage and model/options
overrides. Empty or absent `tools` is accepted; explicit null is not. Disable Pi
cache-affinity metadata with `PI_CACHE_RETENTION=none`; session identifiers are
not approved context. Only the pinned app's canonical English date suffix is
accepted, then discarded. The suffix is never a trusted clock or model input.

`OpenAiTransport::execute_pi_and_persist` performs that validation before token
counting or journal mutation, then uses the existing durable execution path.
It sends the original immutable `PreparedRequest`, **never** Pi's bytes or headers.
This is an intentional restricted profile translation, not a transparent OpenAI
proxy: Pi requests streaming with its thinking control off; Memory alone fixes
low reasoning, strict proposal JSON, no tools/storage and non-streaming upstream.
Authoritative provider usage is settled independently of Pi. No synthetic usage
is invented for a paid response, and no streaming-response adapter exists yet.

Required-database fake-provider tests cover rejection without side effects,
exact upstream payloads, concurrent duplicate admission, durable private result
recovery, cross-project/area denial, revocation during counting/generation and
uncertain billing without reissue. Concurrent valid requests can both reach the
counter before reservation; only one generation can consume a send permit. Do
not claim exactly-once token counting. Attempts/leases remain trusted caller
bindings, not values supplied by Pi.

This in-process API is not a listening HTTP server, authenticated ingress, an
egress sandbox, a scheduler, or source/destination disclosure authorization. A
future broker must enforce those boundaries and validate current reviewed-source
eligibility before disclosing even to the counter, and again before accepting
results. Pi production execution remains disabled; these tests use synthetic
input and fake providers only. The Pi crate uses the validator as a **test-only**
dependency, so its production process adapter still has no SQL/provider client.
