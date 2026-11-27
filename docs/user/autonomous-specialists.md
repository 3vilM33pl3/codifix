# Run autonomous specialists

This first implementation supplies a separate `codifix-agency` service, a Linear
project specialist and an independent OpenBao recovery controller. Installation
and grants are explicit. Building the binaries does not start a service, enroll
a custodian, publish a profile or change a ticket.

## Prepare one instance

Build the companion binaries from the reviewed revision:

```sh
cargo build -p codifix-agency-runtime -p codifix-agency-openbao --release --locked
```

Copy the [example configuration](../../crates/codifix-agency-runtime/examples/config.json)
to a protected operator-owned location. Configure an instance UUID, specialist
manifests, the intended database and distinct credentials for each enrolled peer.
Use a TLS reverse proxy for the private endpoint; the service itself binds only
loopback. Keep the database and peer credentials in the service environment,
never in tasks, model prompts, public profiles or command arguments.

```sh
codifix-agency --config /etc/codifix-agency/config.json check
codifix-agency --config /etc/codifix-agency/config.json migrate
codifix-agency --config /etc/codifix-agency/config.json serve
```

`migrate` explicitly applies the repository's additive migrations to the selected
database. Back it up and verify its identity first. `check` validates configuration
shape; it does not test remote authorization or approve a production rollout.
An optional [systemd template](../../crates/codifix-agency-runtime/examples/codifix-agency.service)
shows a separate service account and environment file. It is not installed by
Codifix's existing installer.

Open `/operator` on that private instance URL to inspect tasks, spending,
deadlines and clarification requests. Enter a separately provisioned operator
token; it stays only in page memory. The screen can cancel a task or pause,
resume and revoke a mandate. A disconnected page does not pause standing work.
CLI users can use `tasks`, `submit`, `respond` and `control` instead.

## Grant bounded authority

Only a configured operator can install root mandates. A mandate names one
specialist, exact capabilities and resources, permitted model destinations,
expiry, money/call/turn limits and delegation depth. It belongs to an existing
Codifix project UUID. An enrollment lists which mandate IDs a peer may use;
learning about an instance through AT Protocol confers no private access.

```sh
codifix-agency --config /etc/codifix-agency/config.json grant \
  --peer operator --file reviewed-grant.json
codifix-agency --config /etc/codifix-agency/config.json submit \
  --peer operator --file task.json
codifix-agency --config /etc/codifix-agency/config.json tasks --peer operator
codifix-agency --config /etc/codifix-agency/config.json control \
  --peer operator --id MANDATE_UUID --action pause
```

The grant document is `{ "project_id": "UUID", "mandate": { ... } }` using the
[mandate contract](../../crates/codifix-agency/src/mandate.rs). Configure child
grants with smaller authority and reserved budgets. A revoked mandate cannot be
resumed; issue a new reviewed grant. Pause/revoke stop future admissions, not an
external request already sent.

A task document uses these fields:

```json
{
  "request_id": "unique-message-id",
  "mandate_id": "00000000-0000-4000-8000-000000000002",
  "specialist_id": "operations",
  "objective": "Observe this runtime's active authority",
  "acceptance_criteria": ["status_observed"],
  "capability": "inspect",
  "resource": {"kind": "service", "id": "local-runtime"},
  "deadline": "REPLACE_WITH_FUTURE_UTC_TIME_WITHIN_GRANT_AND_SEVEN_DAYS",
  "budget": {"micro_usd": 0, "calls": 1, "turns": 2, "delegation_depth": 0},
  "payload": {"kind": "inspect"},
  "parent_id": null
}
```

The deadline placeholder is deliberately invalid until reviewed. Reusing a
request ID is permitted only for the exact original request. To schedule work,
send `{ "id": "UUID", "interval_seconds": 60, "template": TASK }` with
`schedule --peer operator --file schedule.json`. Each due slot receives its own
stable request ID and deadline. A missed expired slot is skipped. Missing peers
are disabled; denied slots advance instead of blocking other schedules. The
`inspect` operation observes runtime authority, not an arbitrary remote service.
Incoming authenticated tasks and native recovery monitoring are the other event
entry points.

## Plan, discuss and apply in Linear

The [Linear Agent configuration](../../crates/codifix-agency-runtime/examples/linear-agent.json)
exposes Linear as its only external tool. Replace the sample actor, project,
team and mandate identifiers. Install the corresponding mandate with the
`linear_project` resource and give each originating companion a peer binding
to this specialist. The optional local model endpoint is a template. Record a
current workload evaluation for the installed model revision, enable its
`accepted_locally` flag, and permit that destination on its mandate before
using it to classify requests, draft text or summarize project progress.
The router selects the lowest measured cost among eligible models.

For an issue request, delegate `linear_track` with criterion
`linear_issue_readback_verified` and resource
`{"kind":"linear_project","id":"<configured project UUID>"}`. Its payload is

```json
{"kind":"linear_track","lookup_title":"Review recovery checks","title":"Verify service recovery","description":"Check health and exercise a representative request."}
```

The agent searches that team and project for the original title. It asks for
missing wording or an issue ID when there are multiple matches. The
originating peer can correct the complete payload or answer the returned
`confirm <digest>` challenge in the same task. Only the reviewed wording is
written. The result includes the verified issue ID and Linear URL. Pass this
`issue_id` on later updates, including after a rename. A repeated request for
the same saved wording returns the link.

Start with a designated test team/project and a restricted test credential.
`Config.linear` contains `endpoint`, `token_env` and an `authorities` map keyed by
mandate UUID. The endpoint is normally `https://api.linear.app/graphql`. A raw
personal API key or full OAuth `Bearer ...` value goes in the named environment
variable. The operator-owned [authority example](../../crates/codifix-agency-projects/examples/standing-authority.json)
binds the source, exact plan UUID, allowed teams/projects and adopted records.
Never take authority from the planning model's output.

Use the provider-neutral [plan example](../../crates/codifix-agency-projects/examples/plan.json):

1. Submit `plan_prepare` with resource `{ "kind": "plan", "id": PLAN_UUID }`,
   criterion `plan_valid` and payload `{ "kind": "plan_prepare", "plan": PLAN }`.
   Explicit unresolved questions return `input_required` to the source agent.
2. Reply in the same A2A task/context with an answer and, when needed, a revised
   payload. The originating agent must supply the revised plan/source version;
   there is no invented answer if its session is unavailable. Preserve the
   questions for a human handoff in that case.
3. Inspect the returned roadmap/change set and `prepared_id`. With agreed
   authority, the operator's authority must bind that exact plan digest/source
   version. Standing authority permits only its preconfigured scope.
4. Submit `plan_apply` with the same mandate/resource, criterion
   `linear_readback_verified` and payload `{ "kind": "plan_apply",
   "prepared_id": "UUID" }`. The reviewed baseline is retained across retries.
5. Submit or schedule `plan_inspect`, criterion `linear_state_observed`, with
   `{ "kind": "plan_inspect", "plan": PLAN }`. It reports current state, links,
   missing resources and unresolved questions. It does not invent forecasts.

A plan can set project start and target dates, lead and status, milestone
target dates, and issue assignees, states, priorities, labels, due dates,
cycles and parent issues. Put a parent before its child. Use the `roadmap`
list for initiatives, initiative/project associations, project dependencies
and project or initiative updates. Updates carry Markdown progress, risks and
date changes together with `onTrack`, `atRisk` or `offTrack` health. These
dates and associations feed Linear's existing timeline and roadmap views.
The Linear Agent example requires the originating peer's exact wording
confirmation for plan operations that change issue text. Keep stable plan
UUIDs and item keys between revisions. The originating agent can relay each
verified outcome from the private task, and use `plan_inspect` to report
relevant subsequent changes. [Reconciliation details](../../crates/codifix-agency-projects/README.md)

A2A calls use `A2A-Version: 1.0`, bearer authentication and an object data part.
Use `configuration.returnImmediately: true` for asynchronous work and poll the
returned task ID. `respond` accepts a complete SendMessageRequest containing the
same task/context IDs and `{ "answer": "...", "replacement_payload": {...} }`
in its data part. A remote `delegate` task supplies an enrolled `peer` and a
nested `request`. Both instances must implement Codifix's execution-lease and
source-policy extension; generic A2A alone is insufficient for delegated authority.

See [SSH host certificate renewal](ssh-host-renewal.md) for the operator-approved Step CA and OpenBao handoff.

For authenticated service alerts, runbooks and incident tracking, see [Respond to a service outage](incident-response.md).

## Recover OpenBao without an LLM or database

Use the separate [OpenBao enrollment and custody runbook](../../crates/codifix-agency-openbao/README.md).
Verify the actual seal type, threshold, native binary/configuration/storage and
maintenance generation. Do not change seal arrangement as part of this pilot.
Keep bootstrap TLS trust, attestation credentials and replay state available
when OpenBao, Workshop, discovery and cloud models are offline.

The controller requests a fresh ceremony. Each independently enrolled custodian
validates it and sends only its own share directly to the pinned OpenBao target.
Automated custodians use protected local files. An interactive custodian accepts
its enrolled operator's share through the dedicated HTTPS handoff. Codifix chat,
agent messages, memory and ordinary logs carry only request and status metadata.
Separate roles and processes in tests are not proof of separate production
custody: provision independent host/account/failure domains so no automation
principal can retrieve a quorum.

Use `codifix-agency-openbao controller-watch CONTROLLER_CONFIG` for independent
native monitoring. A failure stops and emits a secret-free escalation; connect
its service failure/journal signal to the chosen human alert channel during
installation. The independent watcher emits local observations. For social coordination,
schedule the CA specialist's `openbao_recover` operation and configure its
notice recipients. Run one controller for the instance. The phone opens the
receiver-configured protected handoff from the waiting notice in Inbox. The general runtime may coordinate a
configured profile using `openbao_recover`, criterion `openbao_unsealed`, resource
`{ "kind": "openbao", "id": PROFILE }`, payload
`{ "kind": "openbao_recover", "controller": PROFILE }`. Its API cannot supply
custodian shares or replace the independent controller's authorization.

The attestor verifies the stable installation and fresh native process after a
routine reboot. OpenBao does not expose cryptographic proof that an arbitrary
external rekey never happened: planned rekey/storage/identity changes require the
maintenance fence and reviewed reenrollment. Expired trust or identity mismatch
stops recovery. Auto-unseal recovery keys cannot substitute for an unavailable
external seal key.

## Route models and evaluate improvements

`model_draft` returns a JSON artifact only. Configure explicit endpoints and
locally accepted measured profiles; the router will not fetch models or use a
cloud provider merely because it is cheaper. Tasks supply a routing workload,
quality/context/latency limits and permitted destinations. Grants independently
restrict disclosure. Credentials, reviewed token rates and conservative maximum
spend live in configuration. Actual provider usage and estimated all-in cost are
recorded; unknown sends retain their reserved spend.

Use `improve --peer operator --file decision.json` for the artifact lifecycle.
`baseline` returns the target specialist/workload's digest. `propose` binds an
immutable prompt, drafting checklist or configured model-profile artifact to
that baseline. `evaluate` records an independently checked held-out result tied
to both digests and a verifier ID. `rollout` requires a local decision reference
and at most 100000 parts per million (10%). Operational success alone cannot
promote an artifact. `assess` records one immutable external quality assessment
per completed task, with distinct evidence and case identity. Promotion requires
at least 20 passing canary assessments, all assigned canary tasks completed and
assessed, no failures and quality at least the
held-out baseline. `revert` restores the prior baseline for subsequent tasks.
The [typed learning API](../../crates/codifix-agency-runtime/src/learning.rs)
defines these documents and rejects undeclared fields such as a reusable
`passed: true` claim. Only operator/evaluation credentials can accept these
decisions; candidates cannot rewrite their own criteria or authority.

For comparative evaluation, use `compare --plan evaluation.json --outcomes
receipts.json`. Supply matched deterministic, single-agent and network cases
with independent verification, all-in compute/cost, latency and human corrections.
A synthetic report is not evidence of real cost savings or better task quality.

## Deployment evidence

The implementation has local HTTP fixtures and real PostgreSQL coverage, plus
an actual disposable OpenBao restart/unseal test. Live Linear compatibility,
real distributed custody, your TLS/alerting setup and production rollout remain
operator acceptance work. The [architecture](../developer/architecture/agency.md)
and specialist runbooks describe these limits. Jira, additional specialisms,
model-weight training, autonomous deployment and live trading are later work.
