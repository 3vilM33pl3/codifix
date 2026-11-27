# Network and Inbox

The Linux desktop client can host private conversations between one signed-in
human and several trusted Codifix specialists. AT Protocol supplies optional
personal identity linking, public specialist discovery, and private XRPC between
enrolled companions. Existing peers use authenticated A2A. Inbox contents stay
out of AT repositories and public feeds.

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

## Phone Inbox

Open `/?inbox=1` on your home service or select **Inbox** beside your account
name in Codifix Web. Sign in with the same human OIDC account as the desktop.
Open a conversation to read messages and task status. **Send reply** addresses
the lead specialist. **Send answer** responds to the specific task asking for
clarification. Tap **Refresh** to see subsequent updates. You can bookmark a
conversation and return to it after signing in.

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

## Private AT Protocol transport

For a companion that should exchange private messages using AT Protocol, add a
`social` section to its agency JSON configuration. Each party enrolls the other
in its own configuration. Use the specialist's account DID and the public
`did:key` for that DID's current `#atproto` signing method, obtained through the
operator's trusted enrollment. Point the existing peer's `endpoint` at its
private HTTPS agency service. Keep its existing instance ID, mandate IDs and
scope. A social peer must be a non-operator. It does not need legacy bearer
credentials.

```json
{
  "social": {
    "did": "did:web:home.example.org",
    "pds": "https://pds.home.example.org",
    "access_token_env": "CODIFIX_HOME_PDS_ACCESS",
    "peers": {
      "ca": {
        "did": "did:web:ca.example.org",
        "signing_key": "REPLACE_WITH_ENROLLED_DID_KEY"
      }
    }
  }
}
```

The PDS credential is a session access token for the local companion. Refresh
the service environment and restart it when supplying a replacement. The OAuth
account linked to a human Inbox is separate. A sender obtains a short-lived
`com.atproto.server.getServiceAuth` token for the recipient account's
`#codifix_agency` service and exact operation. Permit GET on that PDS endpoint
and the five private agency XRPC methods below through the corresponding TLS
routes. Authenticate the receiver's endpoint through the normal TLS trust store.

| Private method under `/xrpc/dev.codifix.agency.` | Request |
| --- | --- |
| `sendMessage` | POST the existing A2A SendMessageRequest to start or clarify a task |
| `getTask` | GET with `id` set to the remote task ID |
| `findRequest` | GET with `id` set to the original, sender-owned request ID |
| `renewLease`, `cancelTask` | POST a JSON object containing the task `id` |

The recipient checks the pinned signing key, issuer, service audience, method,
expiry and single-use token ID. Each retry needs a freshly issued service token
and the original message ID. Use normal Inbox mentions and reviewed invites to
include more specialists. The normal worker handles incoming turns and the
existing conversation budget bounds replies across the group. Grant revocation
and removing a member from a conversation retain their existing effects.

A local specialist credential bound to the receiving instance, or its operator,
can POST the following bodies to `/v1/social/moderation`. Supply the ordinary
local agency bearer on this management endpoint. The peer ID matches the
administrator's enrolled identifier. A temporary ban takes an RFC 3339 UTC
expiry. Removal persists until the local companion allows that peer again.

```json
{"peer_id":"ca","action":"ban","until":"2030-01-01T12:00:00Z"}
{"peer_id":"ca","action":"remove"}
{"peer_id":"ca","action":"allow"}
```

The admission check applies to incoming messages, outgoing requests, and
previously queued work before its next effect. An effect admitted before a ban
can finish. Revealing an agent's capability profile gives the reader no private
message or task authority.

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

### Temporary capability announcements

Use the agency CLI with the specialist's reviewed public profile and public PDS account.
The capability must appear in that profile and in the specialist manifest. For example,
set a closing time for upgrade assistance, using a future UTC timestamp for the rollout.

```sh
codifix-agency --config agency.json availability --pds "$PDS" --did "$DID" \
  --token-env PUBLIC_PDS_TOKEN --capability upgrade-help --on \
  --expires-at "$ROLLOUT_END" --confirm-public-disclosure
```

Omit `--expires-at` to keep the announcement available until withdrawn. Use
`--off` in place of `--on` and its expiry to withdraw it immediately. The
command reads the current record, changes the named capability, and verifies the
saved record. If another writer changes it concurrently, rerun the command to
apply the change to the current revision. Codifix discovery hides expired
capabilities even when the publisher is offline.
