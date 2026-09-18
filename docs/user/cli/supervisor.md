# Supervisor inspection

## Private proposals and human review

When the operator configures `[supervisor.interaction]` with a private, hash-bound
Workshop work manifest, its saved result is available in the exact project:

```sh
memory supervisor proposals --project memory
memory supervisor result --project memory --action ACTION_UUID
memory supervisor review --project memory --action ACTION_UUID --digest RESULT_SHA256 --request-id NEW_UUID --decision approve
```

The review command previews the exact terms. Repeat with `--confirm` only after
reading the result. Use `reject` to decline. Review is permanent, digest-bound
and idempotent for the same reviewer/request/terms; preserve the request UUID
after an uncertain response. **Approval does not execute the suggestion**, edit
a memory, install software or authorize a model call.

Private reads require current project `memory_read`; review additionally requires
`loops_run`, a writable primary and fresh credentials. Changed/withdrawn source
versions fail closed; current versions are never silently substituted. The
ordinary supervisor feed remains metadata-only. CLI output includes exact source
version IDs and evidence hashes; JSON output is available with `--json` before
the subcommand. No source paths are included.

In the TUI Supervisor tab, press `v` for private proposals in the selected
project, `j`/`k` to select, Enter to inspect, `a`/`x` to prepare approval/rejection,
and Enter again to confirm. `c` closes a pending confirmation; inspect afterward
if the outcome was uncertain. Escape returns to operational status. Web provides
the same saved-result and explicit-review workflow.

This saved-result increment is not a question-submission queue or an autonomous
agent. Interactive new questions and their isolated execution remain separate.

## Operational metadata

`memory supervisor` reads this installation's metadata-only policy, budget and
ordered history. It requires **global `memory_read`**; a project administrator
cannot inspect other projects' instance-wide supervisor activity.

```sh
memory supervisor
memory supervisor --json
memory supervisor --after 123 --limit 50
```

The service uses an explicitly configured durable identity:

```toml
[supervisor]
instance_id = "REPLACE_WITH_DURABLE_INSTANCE_UUID"
area = "workshop"
deterministic_checks = false
```

This is an inactive configuration example, not an activation procedure. The
matching database binding must be provisioned separately through the reviewed
deployment path. Configuration does not create a binding, install policy, enroll
a participant, start Pi or allow model disclosure. The process ID from `/healthz`
is regenerated on startup and is **not** this durable identity.

`execution: not_integrated` describes Pi/model agency, not the separately opt-in
database observer below. A stored enabled policy is not a heartbeat or proof that
an agent is running. Missing configuration,
unbound database identity, missing policy, paused/revoked policy, access denial,
offline storage and identity mismatch are separate states.

Amounts are exact millionths of USD, using UTC accounting days. Reserved amounts
include in-flight and uncertain attempts; do not subtract them from prior spend
or reset them after restart. Decimal-string cursors avoid integer precision loss.

Web: **More → Supervisor**. TUI: select **Supervisor**, press `r` to inspect,
`PageDown` for more history, `Home` for the first page and `j`/`k` to scroll.
The project selector does not change this view. Refresh clears old results before
making a request so denied/offline state is not shown as current healthy state.

These surfaces do not expose questions, private proposals, source text, evidence
bytes, model prompts or provider bodies. They never invoke a model or mutate
policy merely by inspecting it. Private human review uses the separate
project-scoped workflow above; supervisor chat and model scheduling remain
separate implementation work.

## Explicit pause and resume

An existing policy can be paused/resumed by a current **global `system_admin`**,
not a project administrator. Read-only and evaluation services deny changes.
Controls do not create identities/policies, change limits or authorize new data
disclosure. Resume permits an already configured worker to run eligible approved
work, including a paid proposal. A pause cannot recall already transmitted bytes.

Web provides **Pause supervisor / Resume supervisor**, then a confirmation with
the exact instance, policy and request ID. TUI uses `p` / `e` to prepare and
`Enter` to confirm; `c` closes and refreshes without undoing a sent request.

```sh
memory supervisor --json
memory supervisor pause --instance INSTANCE_UUID --policy POLICY_UUID --request-id NEW_REQUEST_UUID
# Review the preview, then repeat the identical command with --confirm.
memory supervisor pause --instance INSTANCE_UUID --policy POLICY_UUID --request-id NEW_REQUEST_UUID --confirm
memory supervisor
```

Replace `pause` with `resume` only after inspecting a currently paused policy.
No-op or stale-state requests fail. Preserve the request UUID: after a lost
response, retry the **identical** request to reconcile its receipt. Replaying an
old resume never undoes a later pause. The receipt describes that request, not
current worker health; inspect afterwards. Policy/history and a metadata-only,
actor-attributed protected auth audit commit atomically. Errors omit raw storage
and provider diagnostics. Browser changes require normal CSRF/Origin checks.

## Deterministic operational checks

After migration 0057, the separately provisioned identity and explicitly enabled
policy can run fixed metadata-only checks when the operator sets
`deterministic_checks = true`. This setting defaults off. Read-only, relay and
evaluation services never start this worker. It observes project/history counts,
blocked/failed background jobs and uncertain supervisor actions only; no content,
model invocation, remote contact, knowledge change or installation is involved.

A database schedule permits one observation every five minutes, checked every
ten seconds. The action, result, schedule and history commit together. Competing
processes serialize on the schedule; restart does not repeat completed periods.
Database disconnection leaves the period due; reconnection resumes polling.
Shutdown/config reload cancels this process's worker. It never creates an identity
or enables policy itself. Pausing/revoking policy blocks subsequent observations.

The UI shows **last observation**, its policy revision and next due time. Old
observations remain visible after policy changes and are not proof of present
availability, backups, network access or healthy models. Successful collection
does not mean the observed counts indicate a healthy system.

## Bounded Workshop proposal worker

For an application deployment that intentionally cannot access Docker or a
provider, use the separate **operator-only** `memory-workshop-worker` executable.
It has no HTTP listener and must never become an MCP tool or application command.
The trusted operator owns Docker access and the approved normal provider key;
neither authority is given to the Memory application or Pi. Its fixed transport
can only contact OpenAI's approved API, with redirects, proxies and retries off.
This is not an OS-wide egress firewall for the operator account.

```sh
# Explicit private paths and durable IDs; never put a secret in command arguments.
memory-workshop-worker observe /PRIVATE/work.json SHA256 INSTANCE_UUID
memory-workshop-worker run /PRIVATE/work.json SHA256 INSTANCE_UUID
```

`DATABASE_URL` must explicitly name a loopback `memory_workshop_` database.
`run` may perform one approved paid generation; it requires the separately
prepared normal `OPENAI_API_KEY`. Missing credentials still permit metadata
recovery but never a claim. `observe` journals/reconciles metadata only. Existing
running or completed actions are not relaunched. A stopped/uncertain run must
be inspected, not retried under a fresh identity to bypass its reservation.
SIGINT/SIGTERM blocks new disclosure while allowing bounded billing settlement.
Output is metadata only; durable history remains visible through the normal
Supervisor views. The in-application worker status is not a heartbeat for this
separate process. It does not provision, migrate, enable policy, import a corpus
or complete the four-area deployment.

This is separate from full autonomous agency and defaults off. An independently
provisioned Workshop service can opt into **one fixed proposal**, with a dedicated
exact-project internal identity and frozen source grant. A private JSON work
file fixes schema version 1, action/source-event/attempt UUIDs, the grant, question
and sandbox artifacts. It must be owned by the service user, mode 0600, inside a
0700 directory. Symlinks, unsafe ancestor permissions, duplicate JSON keys,
unknown fields, changed hashes and other areas are rejected.

```toml
# Add only after target, source exclusions and artifact verification are complete.
[supervisor.model_work]
manifest = "/ABSOLUTE/PRIVATE/work.json"
sha256 = "REPLACE_WITH_EXACT_MANIFEST_SHA256"
```

The host process inherits the approved normal `OPENAI_API_KEY`; Pi receives only
an opaque one-work broker token. Do not point this process at an R3 credential or
an existing mixed-area database. No credential lookup/fallback is performed by
the worker. Current implementation requires non-root Linux AMD64, local Docker,
the pinned local Ubuntu image, hash-verified patched Pi and sandbox driver in
non-group-writable paths, and a separate private runtime directory. Pi gets no
network, host workspace, Docker socket, database credential or provider key.

This operator-host launcher is **not yet compatible with the hardened named
instance service**: that service deliberately denies Docker and external egress
and strips inherited provider keys. Do not weaken it or add Docker-group access.
Use the separate operator path above instead of weakening that named service.
The [Workshop local preview](../workshop-preview.md) demonstrates this separation
with an independently verified containerized app and target; it does not turn
this in-service configuration example into a supported named-service activation.

The worker polls every two seconds and claims once with a 180-second lease.
Shutdown/reload blocks new disclosure and results; an already-started commit can
complete, and already-sent provider calls must still be accounted. Abrupt death
retains durable reservations; recovery expires the action to `uncertain`, never
automatically requeues or regenerates it. Changing the work file requires a new
hash in configuration; reusing an ID with changed terms fails. New work is a new
disclosure/billing decision, not a recovery shortcut. The existing USD 2.50 and
20-generation daily maximum remains enforced independently by the store.

Web, CLI and TUI show `model_work` separately from `execution: not_integrated`
(full autonomous agency). `completed` means the exact stored proposal's hash and
usage were verified against child delivery; `stored` means durable output exists
but this process cannot confirm delivery. Neither means the proposed correction
is correct or has been applied. No result is executed as an instruction. Status
and history omit questions, source contents, paths and private proposal bodies.

## Disposable verification

The ignored service test `supervisor_real_browser_cli_rehearsal` creates its own
synthetic PostgreSQL database, paused policy and read-only service on an allocated
loopback port. It runs the real CLI, TUI through a PTY, and built Web UI through
Playwright, verifies their durable identity against storage, then confirms no
actions were created. It requires built Web assets, a TUI-enabled CLI, Chromium,
Python 3 with `scripts/tests/supervisor-pty-requirements.txt` installed in a test
virtual environment on `PATH`, and the explicitly configured disposable test database.
Set `MEMORY_SUPERVISOR_REHEARSAL_CLI` to that CLI's absolute path before running
the test with `--ignored --nocapture`. Never point it at an existing deployment.
