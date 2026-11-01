# Network and Inbox acceptance

Validated on Linux on 1 November 2026, on `feat/network-inbox`, based on
`feat/specialist-agency` at `1d1d24ea`. Incremental commits cover contracts,
persistence, human authorization, identity/discovery, execution, desktop and
acceptance. The feature is disabled by default in both the human service and
agency companion. See the [operator and user runbook](../../user/network-inbox.md).

## Implemented behavior

- Optional personal AT identity linking uses the isolated, pinned official Node
  OAuth SDK 0.5.13. Authoritative public DID/PDS discovery carries record provenance;
  neither a linked identity nor a discovered profile grants private access.
- Human-owned conversations, memberships, selected disclosure snapshots, ordered
  messages, delivery tickets and response budgets live in PostgreSQL. Migrations
  `0095`–`0097` are additive. Service and companion share that database and home UUID.
- A dedicated non-operator gateway redeems exact human invocation templates.
  Ownership follows delegated work. Current human, project, membership and grant
  authority are rechecked at admission; explicit persisted project access is
  required even when the human service also supports IdP group mappings.
- Conversation models return validated replies, clarification or typed proposals.
  Deterministic admission enforces standing authority and at most eight specialist
  turns per originating human message, including peer requests and follow-ups.
  Successful actions return scoped results and can continue automatically under
  that original budget. Failed or uncertain effects are not blindly retried.
- The desktop exposes Network and Inbox without a project selection. It includes
  lead selection, explicit responders, reviewed invitations, text/plan sharing,
  older-message loading, unread state, task cards and truthful cancellation state.
  Account/service changes clear private client state. Existing opt-in notifications
  remain opt-in. The native bridge narrowly allowlists routes and link attempts.

## Executed checks

All database checks used disposable local PostgreSQL 17 with pgvector and isolated
test databases. `CODIFIX_TEST_REQUIRE_DB=1` made missing database support fail rather
than silently skip. Credentials and identities in the fixtures are synthetic.

| Check | Result |
| --- | --- |
| Six focused Rust crates: agency, runtime, OpenBao, projects, Inbox, identity | 137 passed; two existing opt-in tests ignored |
| Service Network/Inbox HTTP and protected Unix broker integration | 3 passed |
| Existing native-human login, authorization policy and OpenAPI route inventory | 11 passed |
| Official Node identity broker SDK and transport fixtures | 22 passed |
| Desktop renderer workflows | 39 passed |
| Native desktop tests | 18 passed; two existing host-specific probes ignored |
| Strict Clippy for new crates/runtime/service and native bridge | Passed |
| Workspace/native formatting, production frontend and embedded-assets native build | Passed |
| Existing CLI lean build | Passed; existing dead-code warnings remain |
| Real Chromium renderer workflow and visual inspection | Passed using synthetic IPC/service data |
| Linux native startup with isolated Xvfb, D-Bus and disposable XDG directories | Stayed alive for the ten-second harness; no installed app changes |

The six-crate command was:

```sh
CODIFIX_TEST_DATABASE_URL='<disposable PostgreSQL URL>' \
CODIFIX_TEST_REQUIRE_DB=1 \
cargo test -p codifix-inbox -p codifix-identity -p codifix-agency \
  -p codifix-agency-runtime -p codifix-agency-projects \
  -p codifix-agency-openbao --locked
```

Service checks used `--no-default-features`, with `network::tests`,
`auth::desktop::tests`, `auth::policy::` and the `openapi_contract` integration test.
Frontend/native reproduction commands and the Chromium fixture are documented in
the [desktop README](../../../desktop/README.md). Broker reproduction is in its
[runbook](../../../identity-broker/README.md).

## Boundary evidence

| Requirement | Evidence in source tests |
| --- | --- |
| Identity and callback binding | Broker fixtures execute the real SDK PAR/PKCE/DPoP exchange, reject wrong issuer/PDS and replay, and verify credential cleanup. Service tests reject wrong-session polling and conflicting DID ownership, preserve completion across polling retries, and fence concurrent unlink/session revocation. Four failed starts do not consume the pending-attempt quota. |
| Human and service isolation | Inbox tests deny cross-owner/home reads, forged or altered tickets and human-global-role shortcuts. Service HTTP tests deny machine credentials and disabled humans, enforce feature opt-in, and check history ownership and `no-store`. |
| Reviewed disclosure | Sixteen PostgreSQL boundary tests include late old-context replies, peer rephrasing of excluded history, explicit disclosure of a reply, reinvitation and action-result scoping. New participants cannot retrieve excluded context through response descendants. |
| Automatic agency and bounds | Local integration runs an authorized inspect action and returns its result through an automatic second model turn. Cards and continuations retain the original human/root. Tests enforce the lower configured turn bound, the shared eight-turn limit, live project/grant checks and atomic admission locks. |
| Two-instance coordination | Actual local HTTP A2A servers and synthetic model endpoints exercise human → home ticket → remote specialist → private transcript → clarification → same-root answer. Another human cannot retrieve, answer or cancel the work. Lost submission responses reconcile by the original request ID without another model call. |
| Revocation and pause | Tests disable/revoke/remove before admission, lock identity/membership through physical admission, preserve paused queues, stop revoked queued actions and verify cancellation cards clear only after reconciliation. Remote work retains existing bounded leases. |
| Durability and external effects | Successful model artifacts survive retry without a second effect. Existing Linear adapter tests cover lost responses, deterministic external IDs, readback, manual edits, changed authority and interrupted writes. Inbox action continuation is idempotent; uncertain/failed effects wait for input. |
| Compatibility | Existing human PKCE/session/cookie tests, route permissions and API inventory pass. Peer task APIs remain supported; OpenBao's protected controller/custodian flow and secret-isolation tests pass. No unseal-share input was added to chat. |

An independent read-only service review found the failed-link-start quota issue;
it was fixed, regression-tested and re-reviewed. No additional concrete service
ownership, machine-token, feature-flag or session-completion issue remained in
that review.

## Practical limits and activation

- No production enrollment, public capability publication, live AT login, live
  model evaluation, live CA recovery or real Linear workspace mutation was
  performed. Linear acceptance here uses an isolated HTTP test workspace fixture.
  A designated live test project and real identity login remain activation checks.
- The pinned SDK dependency tree still reports the Zod collection-memory
  exhaustion advisory documented in the broker runbook. Response-size, JSON
  depth/collection and concurrency bounds mitigate exposure; the upstream audit
  is not claimed clean. Re-evaluate this dependency before production activation.
- The ordinary Rust run leaves the existing actual-OpenBao executable and agency
  operator-browser probes opt-in. Their previous agency evidence is in
  [specialist acceptance](../specialist-agency/acceptance.md); it was not rerun or
  relabeled as new Inbox evidence. The separate new Inbox Chromium smoke passed.
- Native Xvfb startup does not establish real desktop portals, notifications,
  keyring integration or installed IdP login. Two existing native keyring/TLS
  probes remain ignored in the ordinary native run. Windows/macOS are untested.
- This is one human owner per conversation, one home database and trusted enrolled
  specialists. It is not public Bluesky messaging or a multi-human room. The list
  shows the latest 200 conversations; transcript history is paged. Local model
  quality/cost benefits require workload evaluation after configuration.

Activation uses separate explicit human grants and existing scoped mandates,
evaluated model profiles, same-database configuration and optional broker setup.
Stopping a desktop session or hiding the feature does not revoke standing tasks;
use grant revocation, pause/cancellation or stop the companion as documented.
