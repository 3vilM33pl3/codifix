# Single-response R3 diagnostic

This is a provider-output diagnostic, **not a benchmark run**. It cannot create a
valid R3 observation, execute a returned command or start/retry a campaign.

## Exact synthetic request, no preparation calls

`prepare-response-diagnostic.mjs` reconstructs the second model request from the
retained `configuration-rollout` vector-condition attempt. Its first model action
was `finish`, so the next step used unchanged source files, the single selected
source, the next public task prompt and no prior command output. It excludes
private probes and grading feedback. Before packaging, its prepared request must
hash-match an unsettled reservation in the source journal.

The real reconstruction matches `503b22223ccc9d67669be07b86980fc5f6f131603aae7b2418a37353901e5a79`;
the wire request is 12,622 bytes. This does not reconstruct the missing response.

```bash
node evals/r3/prepare-response-diagnostic.mjs /absolute/used-source-pack /absolute/new-diagnostic-pack
```

The destination must not exist. This reuses the normal runtime/executable packager
and pins the diagnostic module, input and limits as configuration assets. Sources,
labels and model settings must match the original reviewed pack. It creates no
approval, secret access, database, embedding request, container or model call.

## Approval and execution

Maximum actual execution: **one Responses request**, 400,000 reserved input tokens
plus 16,384 maximum output tokens, 120 seconds, **373,728 micro-USD (US$0.373728)**.
This uses the existing locked model/rates and preserves all earlier uncertainties.
It is not a tighter input-token estimate or a verified provider invoice.

The existing v2 campaign approval schema requires a nonempty preparation allocation
strictly below its total. For compatibility the wrapper has two logical slots and
416,385 tokens: one unfunded preparation slot/token that is **never invoked**, plus
the diagnostic. The actual durable provider journal permits **one request and
416,384 tokens**. The ordinary campaign launcher explicitly rejects the diagnostic
marker. The wrapper's 75 planned benchmark items are not diagnostic executions.

Carry forward only the genuinely approved unchanged labels. Record separate,
explicit diagnostic-development spending approval against the final reviewed
manifest identity. Never reuse a spent campaign approval, derive authority from
this runbook or clear an attempt directory to retry.

Use a mode-0600 settings file with only `version: 1`, `approvals` (absolute label
and development receipt paths), and the existing `secret` reference contract.
There is no database URL or endpoint selection. After approval:

```bash
/absolute/pack/private/executables/node /absolute/pack/runtime/response-diagnostic.mjs /absolute/private-settings.json --allow-one-request
```

The trusted owner reuses the existing credential, fixed Responses destination,
approval rechecks and durable budget mediator. An exclusive
`private/diagnostic-attempt` directory prevents competing or repeated launches
before another key read. A failed or interrupted attempt is never automatically
resumed, even if it failed before any provider call. SIGINT/SIGTERM abort transport;
an ambiguous call keeps its entire reservation. Process restart cannot clear the
attempt marker or use it to claim a free retry.

The model's accepted action is discarded, not executed or persisted as an answer.
The private journal retains validated usage/provider metadata; rejection retains
only the bounded content-free output shape and sanitized failure reason. No raw
response text, refusal text or reasoning is saved. A valid diagnostic reports zero
benchmark observations and `official_evidence: false`. It does not settle an older
uncertain call, establish Memory quality or authorize another invocation.

## Offline verification and prepared instance

Four real-CLI/budget tests with fake Bao and mocked provider traffic pass without
skips: concurrent launch, ambiguous output, cancellation, missing approval,
tampered inputs, ordinary-launcher denial and restart fencing. Returned commands
are not executed or saved as answer content. The final preparer's model/asset
guards were rerun through the concurrent-launch case. Thirty-seven receipt,
transport and mediator regressions, formatting and boundaries pass.

Logs: `/tmp/r3-onecall-tests.log`, `/tmp/r3-onecall-final-preparer.log`,
`/tmp/r3-onecall-regressions.log`, `/tmp/r3-onecall-boundaries.log`.

Prepared pack: `/data/tmp/r3-onecall-20260916/`, reviewed manifest identity
`e3c4307af14a7ab18428393a8bf4c1609f4796b8b3ae57b2b7b79130a4a23359`.
Existing labels are unchanged. Spending approval is requested separately; this
preparation record does not claim approval or a live result.
