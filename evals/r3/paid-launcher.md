# Approved R3 launcher

The launcher connects the frozen provider profile, dedicated OpenBao credential,
durable budget journals, disposable services and existing campaign CLI. It is
not approval to spend. Validation uses synthetic credentials and fake provider
responses; live API compatibility and measured quality remain separate gates.

Ordinary non-offline `campaign run` commands remain blocked. The launcher supplies
`--approved-runtime`, exact label/spend approval files and `--allow-shell` to the
runner. Both Rust and the owner validate the runtime. Approval preflight still
prints `paid_execution_enabled: false`: that command checks authority but does
not itself enable or execute a run.

## Prepare, without spending

Build `memory` and the `r3_snapshot`, `r3_score`, `r3_retriever` and `r3_budget`
examples. Use existing immutable Node/Rust container images from the offline
rehearsal. Preparation does not pull images, provision services or read secrets.

Create a runtime-input file with absolute paths to the executables and full
container image IDs (the placeholder values below must be replaced):

```json
{
  "binaries": {
    "memory": "/absolute/build/memory",
    "snapshot": "/absolute/build/examples/r3_snapshot",
    "score": "/absolute/build/examples/r3_score",
    "retriever": "/absolute/build/examples/r3_retriever",
    "budget": "/absolute/build/examples/r3_budget",
    "node": "/absolute/path/to/node",
    "bao": "/absolute/path/to/bao"
  },
  "images": { "node": "sha256:<64 hexadecimal digits>", "rust": "sha256:<64 hexadecimal digits>" }
}
```

```bash
node evals/r3/prepare-paid-calibration.mjs /short/private/path/pack /absolute/runtime-inputs.json
```

The destination must not exist. This prepares only the `artifact-origin` and
`configuration-rollout` development families: four questions and one complete
three-step sequence, five conditions, three repeats (75 executions). It uses the
proposed model/profile and limits in [paid readiness](paid-readiness.md): $10,
500 requests and ten million tokens, including preparation. These remain
**proposals**, not a spending authorization. No held-out cases are copied.

The output stays draft/smoke-only. Sources, labels, runtime modules and executable
copies are pinned. Executables are private copies, so later Cargo rebuilds cannot
replace a running campaign. The pack is bound to its absolute location; moving it
requires repackaging and new approval. Use a short path for the Unix socket.

For an independently prepared draft v2 campaign, the narrower command is
`node evals/r3/lock-paid-runtime.mjs /absolute/pack /absolute/runtime-inputs.json`.
It packages only; it does not select cases, models, budgets or approve labels.
Do not package an already reviewed campaign or merge with an existing runtime.

## Human gates

1. Review the [label pack](label-review.md) and the selected pack's sources,
   private rubrics, acceptance tests and provider-review document.
2. Only after genuine label approval, freeze `label_status` as `reviewed`.
   This changes the manifest hash. Run the pinned `memory eval campaign plan`
   again to obtain the **final** identity; the draft hash is not the spend target.
3. Record actual label and development-spend approvals using that final hash,
   real reviewer/date and agreed maximum microdollars. Never derive them from
   key-storage consent or generate them from this runbook. The existing
   `check-approvals` command verifies them without secret access or model calls.
4. Held-out evaluation requires a different complete campaign and its own
   official-spend approval; this calibration cannot become official evidence.

## Explicit private settings

Create a mode-0600 settings file outside the agent workspace. It contains secret
**references**, never the OpenAI key. Use the already approved dedicated secret;
there is no need to create or rotate another credential.

```json
{
  "version": 1,
  "split": "development",
  "approvals": ["/absolute/labels-approval.json", "/absolute/development-spend.json"],
  "disposable_database": true,
  "database_url": "postgresql://evaluation:temporary-password@127.0.0.1:55432/memory_test",
  "secret": {
    "address": "https://secrets.example.com",
    "mount": "secret",
    "path": "memory-layer/r3-evaluation",
    "field": "OPENAI_API_KEY",
    "version": 1
  }
}
```

Supply a dedicated local disposable PostgreSQL/pgvector instance, never dev or
customer data. The database name must be `memory_test` or start with `memory_r3_`;
the launcher rejects remote destinations and requires explicit disposable consent.
The snapshot owner creates its own databases and project-scoped read-only services.
Existing Bao authentication is used with system CA verification; no TLS bypass,
provider environment lookup, custom API destination or secret-file output is added.

## Launch after approval

```bash
/absolute/pack/private/executables/node /absolute/pack/runtime/launch-paid.mjs /absolute/settings.json --allow-shell
```

Agents receive neither settings, approval files, credentials, controller socket,
host configuration nor hidden graders. The trusted host alone invokes the fixed
Responses/embeddings destinations. It rechecks approvals and frozen assets before
every provider call, reserves limits first, and settles only validated usage.

Exit 0 means the report's applicable gates pass; 2 means execution completed but
review/quality gates remain pending or failed; 1 means refusal/interruption. Read
the report instead of treating process success as evidence of product quality.
Completed knowledge tasks normally need the existing blinded human-review flow.
Preparation and execution costs are separate; unknown charges remain reserved.

## Interruption and recovery

SIGINT/SIGTERM stops new work, aborts transport and drains the active dispatch
before closing its database session. SIGKILL cannot perform graceful cleanup;
the persistent owner fence, per-call journals and broken-IPC handling still prevent
a second launcher from repeating uncertain work. Keep incomplete artifacts.

```bash
node evals/r3/inspect-dispatch.mjs /absolute/pack/private/dispatch
```

Inspection is metadata-only: exit 2 flags uncertainty, not permission to retry.
New runtimes also retain `run/<task>/provider/failure-<payload-sha256>.json`
for failed reserved calls. These mode-0600, exclusively created diagnostic files
bind the manifest and payload digest to a closed-vocabulary failure category.
HTTP status, strictly bounded provider request/response identifiers, allowlisted
error codes/parameter names and incomplete-response reasons may be included.
Credentials, prompts, model output, raw error bodies/messages and stacks are never
written there. Unknown exceptions stay `unknown`. Diagnostics do not settle or
refund a reservation, authorize retries, or retroactively recover an old failure.

Existing `run`, dispatch journal or socket paths block relaunch **before another
credential read**. Do not delete them, move the pack, or pass generic `--resume`
to restart a paid session. Automatic reconstruction of a lost prepared database
is deliberately unsupported. Reconcile incomplete/uncertain usage and owned
resources; any replacement paid campaign needs explicit scope/budget approval.
Already completed artifacts remain reportable without reopening the provider.

The operator/host filesystem and installed runtime libraries remain trusted;
hashes do not authenticate human approval or attest the operating system. Tests
do not prove account billing, current provider availability or customer ROI.
