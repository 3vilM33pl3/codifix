# Supervisor inspection

This reference follows current `main` (`3.0.0`). Private interaction controls
are an opt-in preview requiring separately provisioned policy, source consent
and an operator worker. See [areas](https://codifix.dev/docs/areas) for scope
and [team administration](https://codifix.dev/docs/team-setup/administration)
for installation responsibilities. A visible panel is not worker activation.

The operational view includes the disclosure grant's observed expiry when a
private interaction profile is configured. CLI and TUI use plain-language
warnings, and the TUI recomputes expiry/staleness while open. This does not renew
consent, prove current permissions or establish that a worker is running. Use
refresh to obtain a current observation; new questions still require explicit
confirmation. `--json` output retains the structured response contract.

## Private proposals and human review

When the operator configures `[supervisor.interaction]` with a private, hash-bound
project work manifest, its saved result is available in the exact project:

```sh
codifix supervisor proposals --project example-project
codifix supervisor result --project example-project --action ACTION_UUID
codifix supervisor review --project example-project --action ACTION_UUID --digest RESULT_SHA256 --request-id NEW_UUID --decision approve
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

The original fixed-proposal result remains supported. The opt-in question queue
below adds new reviewed requests; neither workflow provides autonomous agency.

## Explicit private questions

This is an opt-in project-local workflow, not remote expertise discovery or
four-area communication. Migration `0060`, an explicitly provisioned project
identity/policy, the approved exact-source grant, `[supervisor.interaction]`, and
a separate operator worker are required. Merely building this code or opening
the panel does not deploy or enable a paid question worker.

Prepare one question, using a fresh non-nil UUID for this new request:

```sh
codifix supervisor ask --project example-project --question 'What should we verify before demonstrating federation?' --request-id REQUEST_UUID
```

The preparation displays the **exact question**, OpenAI destination,
`gpt-5.6-sol` model, approved source count, current daily limits and two hashes.
It does not invoke a model or enqueue work. Inspect these terms, then copy both
hashes into the confirmation with the **same question and request UUID**:

```sh
codifix supervisor ask --project example-project --question 'What should we verify before demonstrating federation?' --request-id REQUEST_UUID --profile-digest PROFILE_SHA256 --request-digest REQUEST_SHA256 --confirm
codifix supervisor question --project example-project --request-id REQUEST_UUID
codifix supervisor questions --project example-project
# Optional bounded pagination:
codifix supervisor questions --project example-project --after LAST_REQUEST_UUID
```

Confirmation does not silently prepare replacement terms. A changed profile,
source, question or request digest fails closed. The question is limited to
4096 UTF-8 bytes. Only it and the approved collection are supplied; conversation
history, files, ordinary-query results and other areas are never attached.

**Queued is not answered.** The queue admits at most eight outstanding questions
per project, with a ten-minute deadline, and the operator processes one
question at a time. The standing source grant can expire earlier. The existing
20-generation / USD 2.50 daily ceilings are unchanged and may be reduced by
policy. Exhaustion does not authorize increasing limits or selecting another
model. Model suggestions are untrusted text, not executable instructions.

Poll status explicitly. When `proposal_available` is true, use its `action_id`
with `codifix supervisor result` above. Status listings are requester-private
metadata, not question bodies. After an ambiguous submission, retain the same
UUID, question and hashes: inspect status or repeat the exact confirmed command.
This only reconciles the original request; it does not authorize another provider
invocation. An `uncertain` provider attempt is not automatically regenerated.
Starting a new question can repeat billing. Expired work is not restarted.

Saved proposal bodies retain the existing **project-private** review contract:
another currently authorized project reader can inspect them while the original
requester's disclosure authority remains valid. Requester-only question status
listing does not make the eventual proposal a requester-only message.

Question intake and execution require current direct database permissions for
`memory_read`, `loops_run` and `bundle_export`, plus a revocable service token or
browser session. Group-mapping-only permissions are insufficient for this narrow
background path. The original submitting credential remains bound to queued
work: revocation/expiry blocks further execution and answer delivery even if the
same person later signs in again. Changed or withdrawn sources, an expired
standing grant, policy changes and disabled identities also block disclosure.
Already transmitted bytes cannot be recalled.

For an authenticated numeric-loopback single-user service, CLI/TUI verifies
`/v1/auth/me` and exchanges local-owner access for a revocable session through
the existing bootstrap endpoint. Its cookie/CSRF values remain only in client
memory, shared across TUI requests; they are never printed or saved to disk.
Redirects and proxies are disabled for these question calls. Remote/multi-user
service-token access is not exchanged for owner authority. A revoked cached
session is not automatically reminted to bypass denial.

### TUI questions

In **Supervisor**, press `v` for the selected project's private pane, then `q`
for questions (`q` does not quit inside that pane). Press `n`, type the exact
question and press Enter to prepare. Read the destination, model, limits and
hashes; Enter again confirms a potentially paid attempt. Escape cancels an
unsent preview. After an uncertain submission, Enter retries the same request
and `r` inspects it; no new UUID is generated. Use `l` for your question list,
`j`/`k` to select, Enter for status, `r` to refresh, and Enter on an available
answer to read its citations. Escape returns to private proposals.

## Operational metadata

`codifix supervisor` reads this installation's metadata-only policy, budget and
ordered history. It requires **global `memory_read`**; a project administrator
cannot inspect other projects' instance-wide supervisor activity.

```sh
codifix supervisor
codifix supervisor --json
codifix supervisor --after 123 --limit 50
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

The operational metadata surface does not expose questions, private proposals, source text, evidence
bytes, model prompts or provider bodies. They never invoke a model or mutate
policy merely by inspecting it. Private results, human review and explicit
questions use the separate project-scoped workflows above. Full autonomous
supervision remains outside this bounded interaction increment.

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
codifix supervisor --json
codifix supervisor pause --instance INSTANCE_UUID --policy POLICY_UUID --request-id NEW_REQUEST_UUID
# Review the preview, then repeat the identical command with --confirm.
codifix supervisor pause --instance INSTANCE_UUID --policy POLICY_UUID --request-id NEW_REQUEST_UUID --confirm
codifix supervisor
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

For deployment prerequisites and knowledge boundaries, see [areas](https://codifix.dev/docs/areas) and [team administration](https://codifix.dev/docs/team-setup/administration). Operator worker activation remains separately reviewed deployment work.
