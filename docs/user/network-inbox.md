# Network and Inbox

The Linux desktop client can host private conversations between one signed-in
human and several trusted Codifix specialists. AT Protocol supplies optional
personal identity linking and public specialist discovery. Private conversations
and tasks use the authenticated agency transport; nothing in an Inbox is posted
to an AT repository or Bluesky.

## Using the client

Select the home service/area and sign in with your existing human account. Open
**Network** or **Inbox** without selecting a project. An older service or one with
the feature disabled shows an unavailable state.

Network separates public discovery from enrolled contacts. Search by handle,
DID or an `at://` Codifix capability URI to inspect public identity and provenance.
A public profile is not a grant. Only contacts enrolled by the operator and
covered by your current invocation grant can receive private work. Public-only
PDS discovery requires the optional identity broker; existing enrolled contacts
continue to work without it.

Start a conversation with an enrolled specialist. It becomes the lead. Messages
go to the lead unless you select other responders using mentions. Mentions change
who responds, not who can see shared context. If the lead is unavailable, select
another lead or address a remaining participant explicitly.

Before inviting another specialist, review/edit the joining summary and select
any earlier messages to disclose. The new participant receives that exact
snapshot and eligible subsequent messages. Replies and peer requests derived
from excluded earlier context are held back from that participant; they remain
visible to you. Removing a participant prevents future admission and requests
cancellation of its ongoing work. Re-inviting requires a fresh reviewed snapshot.

Send plain text, Markdown or a structured Codifix project plan. Model proposals
are admitted under existing standing grants. Routine authorized work does not
require another approval. New authority, unavailable context and missing
requirements require clarification. A response has at most eight specialist
turns (or a lower configured bound), with shared cost, call and deadline limits.
Clarification uses the remaining response budget.

Task cards show progress, clarification, artifacts and external links. Delivery
is confirmed only after durable admission. Cancellation remains pending until
reconciled; an already admitted effect may finish. Pause prevents new admissions.
A failure at an uncertain external effect is not automatically retried.

Link your personal AT account from Network if desired. Codifix opens the system
browser and finishes verification through the home service. Account linking
changes identity metadata only; it does not enroll peers or grant task authority.
Unlinking preserves history. Notifications remain opt-in under existing Settings.

Never submit OpenBao unseal shares in chat. Recovery tasks use the separate
protected custodian/controller workflow described in [Autonomous specialists](autonomous-specialists.md).

## Operator setup

For the 3.1.0-rc.1 Linux candidate, use the separate
[isolated candidate rehearsal runbook](network-inbox-candidate.md) to verify
artifacts and exercise the candidate without a production installation or
live grants.

This feature is disabled by default in both services. Use a staging environment
first. The human service and agency companion must use the **same PostgreSQL
database** and the same durable home UUID. Migrate using the updated service or
`codifix-agency --config <path> migrate` before enabling the client.

Add to the human service configuration:

```toml
[inbox]
enabled = true
home_instance_id = "11111111-1111-4111-8111-111111111111"
# Optional; omit to run private enrolled conversations without AT linking/discovery.
identity_broker_socket = "/run/codifix-identity/broker.sock"
```

When an area identity is configured, its instance UUID must match the inbox home.
Use the existing human OIDC/native login configuration. Machine API tokens cannot
impersonate Inbox users, even if they carry administrator permissions. Explicit
human invocation grants are separate from conversation membership and public
identity. Persist the human's project membership or explicit global read
permission in the shared database so the companion can recheck it independently.
An IdP group-only service mapping is not a substitute for that persisted access.

In the agency JSON configuration, set `inbox_enabled: true`, add
`conversation_turn` to the participating specialist's capabilities and install an
appropriate standing mandate. Configure an evaluated model profile with a
`conversation` workload, its permitted destination and a conservative per-call
reservation. No model configuration or production grant is installed by the
feature itself. The existing `serve` worker loop dispatches and reconciles Inbox
work; `worker-once` is useful in an isolated rehearsal.

Create an invocation grant JSON using the type in
`crates/codifix-inbox/src/lib.rs`. It contains:

- `id`, `home_instance_id`, `principal_id`, `expires_at`.
- `contact`: stable contact UUID, target instance/specialist IDs, display metadata,
  capabilities, `enrolled: true`, `available: true`, `remote`, and nullable
  `grant_id`/`mandate_id` display fields (computed on reads).
- `request`: an exact `Submit` template with the selected mandate, specialist,
  capability, resource, deadline, finite budget, acceptance criterion and payload.
  The gateway replaces message IDs, expiry-bounded deadlines and conversation
  snapshots; a model cannot choose authority fields.
- `routing`: the existing evaluated `RoutingRequest` for conversation work.
- `actions`: optional `{key, request}` templates for `inspect`, `plan_prepare`,
  `plan_apply`, `plan_inspect` or `openbao_recover`. Each template has its own exact
  mandate/resource binding. No shell, arbitrary HTTP, model routing or delegation
  action templates are accepted.

A local conversation template uses `capability: "conversation_turn"`,
`acceptance_criteria: ["conversation_reply_valid"]` and
`payload: {"kind":"conversation_turn"}`. Its specialist and instance must match
its standing mandate and contact. Plan operations retain the existing exact plan
ID and Linear authority mapping; creating a plan outside those resources requires
an appropriately scoped grant.

For a remote contact, the local template is `delegate`, scoped to the exact
enrolled peer ID, with criterion `delegated_result_reported`. Its payload contains
`peer` and an exact nested remote `Submit` for `conversation_turn`. The target
instance/specialist must match the contact and enrolled peer. The nested request
has narrower budgets/depth; reserve enough parent budget for initial activation
and delegated work. Private remote endpoints/credentials come from the existing
operator peer configuration, never the discovered public profile.

Install or revoke the human grant through an existing local operator:

```sh
codifix-agency --config agency.json inbox-enroll --peer operator --file human-grant.json
codifix-agency --config agency.json inbox-revoke --peer operator --id <grant-uuid>
```

No special gateway bearer is created. `inbox-gateway:` is reserved for the
same-database ticket path and rejected in configured HTTP peer identities.
Revocation, disabled humans, removed memberships and changed project permissions
stop future admission. Remote work observes cancellation and its existing
short-lived execution leases. Stopping a desktop session does not revoke a
standing mandate; use grant controls for that.

## Optional AT identity broker

Follow the [identity broker runbook](../../identity-broker/README.md) for the
pinned SDK, dedicated OS user, protected socket and HTTPS public metadata/callback.
Only those two OAuth endpoints need public forwarding. Keep task APIs private.
The service omits query strings from HTTP tracing; configure the reverse proxy to
avoid recording OAuth callback queries too.

The broker requests only the `atproto` identity scope. It verifies issuer,
subject/DID and authoritative PDS bindings, attempts remote token revocation,
discards local OAuth credentials and returns a one-use completion receipt.
Polling is idempotent until the service commits the unique human/DID link and
acknowledges it. A broker restart expires pending attempts; start linking again.
The pinned SDK dependency audit finding and its bounded-input mitigation are
recorded in the broker runbook. Re-evaluate it before production activation.

## Boundaries and operations

The service APIs are `/v1/network/...` and `/v1/inbox/...`, authenticated using the
existing human session, with browser Origin/CSRF checks and native credential
storage. Responses are `no-store`. Account/service changes clear client caches.
Tasks are attributed to their originating human, conversation and grant through
local and delegated work; a shared gateway identity never gives another human
access to those records.

History is stored in PostgreSQL, with bounded reads and older-message paging. The
conversation list displays the latest 200 conversations. Messages and plan inputs
must fit the bounded context envelope; the client validates UTF-8 byte sizes.
No multi-human rooms, public posts, attachment execution, automatic peer
enrollment or autonomous code deployment are included.

Use the existing agency operator task/effect ledger for failed or uncertain work.
Inspect the Inbox cards and protected database records for dispatch status; logs
must not contain message bodies, OAuth responses, credentials or recovery shares.
Back up the shared database as one unit. Disabling the feature hides client APIs;
revoke grants or stop the companion when execution must stop. Additive migrations
preserve existing agency and memory APIs.
