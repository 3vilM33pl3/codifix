# Mac project onboarding

This feature is under implementation. The development package now includes a
resumable sign-in/select/clone/offline-sync operator and a separate Codex/Claude
setup stage. Optional runtime-bundle packaging and guided first-run now exist.
The ARM64 candidate is operating on the test Mac: sign-in, project selection,
repository setup, both downloads, local queries, curated uploads and reviewed
corrections have been observed. Process restart recovery passed. Host reboot,
live device revocation and clean installation of the final combined package
remain unverified; the working installation includes documented repair receipts.
See the [execution contract](../reviews/mac-onboarding/README.md) for completion
checks and the [deployment ledger](../reviews/mac-onboarding/company-deployment-20261015.md)
for the installed candidate and owner changes.

## Using an already configured laptop

From a selected repository, run `codifix tui`; its operator-owned routing chooses
the corresponding local area. Outside a mapped repository, use
`codifix --area homelab tui` or `codifix --area workshop tui`. An unavailable area
does not redirect writes somewhere else.

Use **Query** for normal local retrieval, or **More → Offline** for downloaded
owner knowledge, citations, source versions, synchronization freshness and upload
status in the Web UI. The Web **Memories** tab's local count is not the number
of downloaded owner versions. The updated TUI **Memories** tab combines local
and synchronized records, clearly marking owner copies read-only. Its **Offline**
tab remains available for detailed subscription status and cache history.
`codifix urls` lists configuration-derived entry points; their presence alone
does not assert live service health.

The watcher captures local work. Only curated additions and explicitly staged
corrections synchronize; raw captures and session bodies stay local. Setup does
not enable paid curation. Git commit/push and owner correction approval remain
separate actions. The background worker checks queued work promptly and refreshes
downloads every five minutes while online.

## Guided first-run candidate

An operator-built package containing the reviewed runtime includes
`codifix-onboard`. The existing lightweight package does not include that launcher.
With your existing projects folder, start:

```bash
codifix-onboard first-run --company https://YOUR-COMPANY-MEMORY-ORIGIN \
  --runtime /ABS/INSTALLED-RUNTIME/runtime-prerequisites.json \
  --name "My Mac" --workspace-parent "$HOME/Projects" \
  --state "$HOME/Library/Application Support/codifix/onboarding"
```

Sign in in the browser, select authorized offline projects by number, and press
Enter for suggested repository paths or specify an existing repository. Then
review and confirm the repository/tools, offline-copy and local-services plans
with `yes`. No installer JSON, source build or provider key is needed. Git still
uses your own Git credentials. Existing repositories, dirty work and custom
settings are preserved. Trust is shown as remote-only, not copied to your Mac.

Use the installed runtime descriptor included in your package; `--runtime` is
required. Run from your logged-in Mac session with its login Keychain unlocked.
An SSH session can report `macOS Keychain write failed` / `User interaction is
not allowed` before enrollment starts, even when the login Keychain is already
unlocked. Run first-run in Terminal on the Mac's logged-in desktop, not an SSH
login session. If the Keychain is locked, use `security unlock-keychain` there
and enter the Mac password locally. There is no plaintext credential fallback.

On macOS, allow Memory's Local Network access when prompted so it can reach your
private company server. Check **System Settings → Privacy & Security → Local
Network** if a desktop process cannot reach the server although SSH/curl can.
Those processes can have different privacy permissions; successful curl alone
does not prove that Memory's background services can connect. Keep TLS validation
enabled and verify actual service synchronization after granting access.

### Full curated-project synchronization candidate

Add `--curated-sync` to first-run for the new two-way project mode. During browser
device approval, explicitly allow curated contributions for each selected project;
this option is available only when your current owner-side permissions allow it.
Existing reader devices do not acquire write permission automatically.

This mode downloads curated histories, canonical state and asserted same-project
relations, including knowledge that has **not** been published to other federation
participants. The derived heuristic graph remains owner-local.
Raw captures, session bodies and private source paths are excluded. Unavailable
source references remain explicitly marked. The existing publication-only mode is
unchanged and remains read-only.

The local queue records curated additions transactionally. Accepted additions
receive an owner receipt; changes to an existing owner stream become correction
proposals, with version conflicts visible rather than overwritten. The client
retains operation IDs across retries. `codifix projects sync-status --profile
<private-profile>` includes queued, accepted, proposed, conflict and blocked counts.
Downloaded records stay in a separate read-only projection and never enter the
upload trigger. Git commit, merge and push remain your normal Git workflow.

Inspect an owner's downloaded versions without following private source links:

```bash
codifix --area homelab history CANONICAL_UUID --subscription SUBSCRIPTION_UUID --json
```

Use `--after-version N --limit 16` for subsequent pages. The Web query/Offline
views provide **View owner history**; TUI Synchronized knowledge uses `h` for
history and `n` for more versions. These are read-only owner copies.

This is an internal candidate with the bounded native evidence linked above,
not a public release. Mutable-state/relation/deletion uploads are not an
owner-write protocol; the remaining acceptance checks stay on the execution
checklist.

To propose a correction to an owner copy, inspect its downloaded version and
digest, then use the correction form in its history view or the local CLI:

```bash
codifix --area homelab projects propose-correction --subscription SUBSCRIPTION_UUID --request correction.json
codifix --area homelab projects propose-correction --subscription SUBSCRIPTION_UUID --request correction.json --confirm
```

The first command previews only. The request contains a stable `operation_id`,
`source_memory_id`, bare SHA-256 `expected_digest`, and `content` with
`canonical_text`, `summary`, `memory_type`, `importance` (1–5) and `confidence`
(0–1). Reuse the exact file/operation ID after an interrupted confirmation.
Confirmation queues local work for automatic upload and owner review; it does
not approve or overwrite the remote version. A changed/withdrawn source requires
a fresh review. No model is invoked.

The state parent and projects folder must exist and remain separate. First-run creates
new private state; it does not change an existing directory's permissions. If a
selected port is occupied, choose explicit `--pg-port`/`--api-base-port` values;
no other installation is stopped. Confirmed native apply creates a missing user
LaunchAgents directory and only namespaced jobs. `--prepare-only` saves selections
without cloning or starting services.

After interruption, repeat the same command and inputs. It retains the selected
projects, identifiers and completed effects; changed manifests/executables need
new review, not automatic substitution. After the journey completes, separately
approve the owning-human Codex/OpenAI and Claude/Anthropic grants with:

```bash
codifix-onboard journey-agents --journal "$HOME/Library/Application Support/codifix/onboarding/journey.json"
codifix-onboard journey-status --journal "$HOME/Library/Application Support/codifix/onboarding/journey.json"
```

These commands enable no paid model calls. Current implementation supports the
reviewed Memory skills format, not arbitrary plugins. The new curated-sync mode
connects a separate durable contribution queue to authenticated owner receipts;
legacy raw drafts are not uploaded by that path. Trust expertise integration
with the new laptop identities remains outside this increment.

This candidate has both synthetic tests and observed two-project native operation,
but not a clean-Mac replay of the final combined package.
[Runtime packaging and recovery details](../../packaging/macos/operator-README.md)
explain prerequisites and remaining native validation.

## Administration in the development build

The Access page now includes Users, Projects, Devices and Company alongside the existing
memberships and service tokens. Company administration requires global
`system_admin`; a project administrator is not automatically a company
administrator. An enrolled OIDC human can see their own Devices/Agent access
without gaining the administration inventory. Browser
mutations also require CSRF protection and a writable service.

- Users can be registered using the configured OIDC issuer and a verified stable
  subject before their first Memory login, then assigned project memberships.
  Email alone never identifies or merges an account. Effective access shows both
  direct grants and configured group grants.
- Disabling a Memory user fences that user's sessions and enrolled devices. It
  does not pretend that the external Authentik account was also disabled.
- Projects have append-only reviewed manifests. Editing a manifest requires its
  current revision; a conflicting edit must be reviewed again. Pinned tool
  metadata does not itself install software or authorize model disclosure.
- Devices have independent revocation. New enrollment requires an actual OIDC
  human browser session, not the installation's local-owner token.

Local controls administer this instance. The Company view coordinates grants
through explicitly configured owner delegates: choose an enrolled owner, enter
the exact project UUID and verified user subject, review the role/revisions and
confirm. Owners apply their own exact-project ceilings and reject stale revisions
or another issuer. Trust is shown as remote expertise only. Catalog entries never
create permissions in another area's database by themselves.

An interrupted grant retains its operation ID. Retry only that exact request or
reconcile its durable operation: reconciliation checks the owner before retrying
an absent effect. History is not current access. The separately timestamped owner
result displays direct and group roles; revoking a direct grant does not remove
group permissions. Server transport addresses and delegate credentials are not
sent to the console. The company device path uses separate delivery delegates,
not those membership-management credentials.

### Selective owner delivery setup

Each non-Trust owner explicitly configures a `company_delivery_delegations`
entry: company, owner, area, shared issuer, dedicated service principal/token,
policy revision and an exact project allowlist. The owner's delivery-delegate
provisioner issues that configured credential once, after protected audit commit;
it cannot recover a lost secret or restore revoked authority. Normal roles do
not include its custom `project_scoped_delivery` permission.

The company registry opts in using a separate `delivery_token_env` and
`delivery_policy_revision` for each owner. Keep it distinct from the existing
`token_env` membership-management credential. Both remain server-side. Existing
registries without the delivery pair retain their legacy behavior; they do not
silently enable laptop delivery. Trust cannot opt into offline delivery.

### Optional Authentik management

The server-only management adapter is disabled by default. A configured operator
may enable it using `auth.authentik_management`: an HTTPS provider origin,
`token_env`, a restricted `user_path`, and an optional `invitation_flow` UUID.
The provider service account must have only the required user/invitation object
permissions. Live Authentik RBAC has not yet been rehearsed by this development
work; the adapter's checks do not replace provider-side permission restrictions.

Create, invite and provider-disable actions retain a durable operation reference.
An invitation is created in Authentik; this implementation does not send its
email automatically. Use the provider's invitation administration for delivery.
Provider IDs are not assumed to be OIDC subjects.

If an operation is pending or uncertain, use the displayed operation inventory
and provider correlation to reconcile it in Authentik before starting another
operation. Refresh reads status; it never retries an external account action.
Credentials and provider response bodies are excluded from these displays.

## Device approval in the development build

The requesting client presents a public code and a credential fingerprint.
When company delivery is configured, the public browser return includes
`company_device=1`. Sign in and review the device, expiry, exact owner/area/project
identities, manifest hashes, offline-copy consent, tools and output destinations.
Company offline selections are reader-only by default. A separate curated-write
checkbox binds contribution consent to that device and exact project; it does not
authorize new tools or model disclosure. Trust is never an offline selection.

The older `/?device_code=ABCD-1234` local-owner flow remains explicit and permits
reader/writer choices only within current local permissions. A company error
never downgrades to it. Once a company approval intent is recorded, that same
challenge cannot be claimed by local approval.

The server approves the hash of a credential generated by the client; polling
never returns a bearer secret. Current human permissions continue limiting the
device after approval. A device cannot inherit a new project or higher role
merely because its human later receives more access. Company approval journals
stable owner operation IDs and immutable selections before contacting owners.
Pending or uncertain confirmation retains the exact request; explicitly retry
only that same approval. It never treats partial owner acceptance as a completed
device enrollment or replaces pinned trust keys while resuming.

This is not yet the full Mac onboarding journey. The gateway/owner selective feed,
read-only projector and prerequisite-based native provisioning operator now exist
in the development build. Cross-area device approval and selective delivery are
implemented, as are read-only Web/TUI cache views and terminal selection of
already-approved projects. A prerequisite-based installer journey and separate
agent identities now exist. Native Mac acceptance remains unfinished.

## Journaled workspace preparation in the development build

After a reviewed device login, `codifix projects available --company
https://<company-origin>` lists the device's current project scope. The initial
workspace commands require that approved Keychain credential; Linux tests use
fake stores, not a production plaintext fallback. Native Mac execution is still
unverified.

In a native Mac candidate, start the existing browser-assisted login with:

```bash
codifix auth login --company https://<company-origin> --name "My Mac"
```

After browser approval, a TUI-enabled build can prepare one project interactively:

```bash
codifix projects select --company https://<company-origin> \
  --workspace /Users/<you>/Projects/<repository> \
  --journal /Users/<you>/Projects/project-onboarding.json
```

Up/Down selects an exact area/owner/project; Enter opens the complete manifest,
tool/output policy, templates and local paths. Space consents to reader/offline
preparation; C confirms. Escape goes back and clears consent; q or Ctrl-C cancels
without preparing a journal. Only currently eligible reader/offline projects can
be confirmed. Terminal state is restored before fresh authorization and manifest
checks. Confirmation prepares metadata only; it does not clone or activate
anything. Repeat for another project, or use non-interactive prepare below.
Lean builds without TUI support report the picker unavailable explicitly.

Prepare a preview using the exact area, owner instance, project UUID and reviewed
revision, not the project slug. Both paths must be absolute, their parent
directories must already exist, and the journal must be outside the repository:

```bash
codifix projects prepare --company https://<company-origin> \
  --area workshop --owner-instance <instance-uuid> \
  --project <project-uuid> --revision <reviewed-revision> \
  --workspace /Users/<you>/Projects/<repository> \
  --journal /Users/<you>/Projects/project-onboarding.json
codifix projects apply --company https://<company-origin> \
  --journal /Users/<you>/Projects/project-onboarding.json \
  --plan-sha256 <exact-digest-from-preview> --confirm
codifix projects status --journal /Users/<you>/Projects/project-onboarding.json
```

Prepare writes only a metadata journal containing immutable manifest/template
inputs and a request UUID; it does not clone, download tools or store credentials
in that journal. Review the full preview before confirming its digest. Apply
fetches current device scope again before repository and artifact effects. A
changed manifest, revoked reader/offline grant, different device or changed plan stops the
operation; local stages already completed are retained. Status reads local
metadata and does not assert that remote access is still authorized.

New repositories clone through the user's separately configured Git credentials,
never the Memory device token. Redirects, repository hooks, submodule recursion
and automatic maintenance are disabled. Existing repositories must have the
reviewed origin URL; their branch and uncommitted changes are left alone. No
existing repository is reset, checked out, cleared or automatically recloned.
Every apply/resume rechecks the repository and reviewed origin, including completed
stages; newly prepared repositories also require their immutable request/plan
receipt. Checkout of new private clones ignores global/system Git configuration
and external attributes. Repository filters (including required filters), local
filter definitions/config includes and external local attributes are explicitly
unsupported rather than executed or silently materialized incorrectly. Normal
user Git credentials remain available for the `--no-checkout` clone.
Interrupted work reuses the recorded request and private staging. An unusable
partial clone or publication conflict requires inspection rather than automatic
destructive cleanup. Trust is remote-only and rejects local workspace preparation.

The exact packaged Sol/Astra project templates are applied only when
`.codex/config.toml` or `.codex/agents/astra_adviser.toml` is missing. Existing
Codex settings/advisers, Claude files and global settings are preserved; a skipped
custom file is listed for manual review rather than silently replaced.

The initial pinned-tool installer supports only the existing Memory skill ZIP
layout, reviewed under tool name `memory-skills`: credential-free HTTPS, an exact
SHA-256 pin and a matching version in every bundled known Memory `SKILL.md`.
Archives must contain `.agents/skills/<known-memory-skill>/...`; paths, file types,
download/expansion sizes and duplicate entries are checked. Only missing skill
directories are installed. Selected names use minimal ASCII; case aliases at any
directory component, receipt aliases and file/directory prefix conflicts fail
before installation. Journal containment is checked through physical filesystem
identities and existing canonical ancestors, not path spelling alone. Linux
alias fixtures are covered; native case-insensitive APFS validation is pending.
Missing reviewed Memory skills are installed in both `.agents/skills/` for Codex
and `.claude/skills/` for Claude. Custom skills are retained, scripts are not
executed or made executable, and no generic executable plugin is installed. Unknown tool
formats or failed hashes/versions stop with the completed journal stages retained.

Agent registration is a separate explicitly approved stage described below;
the broad device-login credential is not reused for MCP. Workspace commands
alone do not provision PostgreSQL, start services,
build source, enable paid model calls, clone Trust sources or perform selective
sync. Local repository/template stages are not a completed company onboarding
installation.

## Selective synchronization commands in the development build

Full/embedded PostgreSQL packages include `project-sync`; a lean binary reports
that the capability is unavailable rather than claiming synchronization worked.
New clients address the company using the full area/owner/project identity, never
a project UUID alone. The company uses its dedicated delivery credentials to ask
each selected owner for its currently authorized feed; it never forwards the
laptop's bearer token. Current owner manifests, permissions and pinned bootstrap
must still match before repository preparation or delivery. The company rechecks
the linked human and device after remote calls before returning bytes.

The legacy local-owner endpoints remain available to local enrolled devices.
A company device has no ordinary project memberships or global permissions and
cannot fall through to those endpoints, broad exports or whole-space federation
pulls. Filtered pages verify gateway attestations and record digests, not the
complete repository commitment.

For a provisioned development laptop with a reviewed private replica profile:

```bash
codifix projects sync --profile /Users/<you>/.config/codifix/subscriptions/<project>.json
codifix projects sync --profile /Users/<you>/.config/codifix/subscriptions/<project>.json --watch
codifix projects sync-status --profile /Users/<you>/.config/codifix/subscriptions/<project>.json
codifix projects offline-search --profile /Users/<you>/.config/codifix/subscriptions/<project>.json \
  --question "What is the approved procedure?" --limit 8
```

These commands are not yet the one-click installer; do not point them at an
existing server database. Profiles are bounded protected
JSON, in an owner-only directory with a single-link owner-only file. Device
credentials are loaded from Keychain and are never stored in the profile. The
profile contains a separately generated **local** PostgreSQL connection, pinned
company/device/project/area/owner/space/catalog identity, and the public gateway
receipt-key schedule. Its database must be an explicit loopback address/port and
`codifix_laptop_workshop`, `codifix_laptop_homelab` or `codifix_laptop_general`.
No fallback, database migration or remote-database connection occurs here.

Sync checks current project consent and the authenticated control-plane bootstrap
before initialization. Changed review, device or receipt keys require explicit
profile renewal. Each received page is nonce/audience/scope/cursor-bound, signed,
and individually receipt-verified before its separate projection/cursor commit.
No whole-space stream, CAR, raw capture, session or unreviewed dependency is
downloaded. A filtered page is not a complete repository proof.

`--watch` starts immediately, polls every five minutes, retries transient outages,
and stops on Ctrl-C without losing committed progress. Failed or revoked access
is visible; old offline copies are not falsely described as recalled. Status and
offline search inspect existing projections without creating subscriptions or
contacting the company. Status reports observation age and explicitly unknown
current remote authority. Search returns read-only source/shared versions,
original authors, reviewed evidence, locators and digests. It does not mutate
local authoritative knowledge or authorize model forwarding.

This is the explicit CLI read-only projection path. The new dedicated Web
`More → Offline` and TUI `Offline` views inspect the same isolated cache when the
native operator has emitted `[laptop_cache]` bindings. They show subscription,
area, owner/project identity, last verified page time and plain-text citations.
Uninitialized/stale caches do not appear fresh. Search never contacts another
instance or a model, and selection changes cancel stale results. Ordinary service
retrieval still does not merge these projections or forward them to a model.
No Mac-native result is claimed from the Linux test harness.

### Native provisioning candidate

Export public trust metadata for an exact selected project. The parent directory
must already be private (0700); the output is create-only (0600), never a device
credential or a server database connection:

```bash
codifix projects subscription --company https://<company-origin> \
  --area workshop --owner-instance <instance-uuid> \
  --project <project-uuid> --revision <reviewed-revision> \
  --output /Users/<you>/.config/codifix/subscription.json
codifix projects subscription --company https://<company-origin> \
  --area workshop --owner-instance <instance-uuid> \
  --project <project-uuid> --revision <reviewed-revision> \
  --output /Users/<you>/.config/codifix/subscription.json \
  --bootstrap-sha256 <digest-from-preview> --confirm
```

An identical confirmed replay retains the local subscription UUID. A different
device, manifest, receipt-key schedule or existing output stops without replacing
the file. The operator consumes this metadata and adds only its separately
generated loopback database credential.

The candidate package bundles `onboard.py` and its operator instructions under
`/usr/local/share/codifix/onboarding/operator/`. Follow the
[native operator runbook](../../packaging/macos/operator-README.md) for the exact
reviewed input, preview and confirmed apply. It currently requires an installed,
architecture-matched Memory binary, Python 3.11+ and a reviewed native
PostgreSQL/pgvector toolset. It does not download these prerequisites or build
source on the laptop. This is not yet a self-contained first-run installer.

The operator creates only fresh, per-user area services and a private loopback
cluster, schedules each selected project every five minutes, preserves unrelated
launchers/services, and resumes the same journal. Its local area services are
read-only at this stage; local capture/proposal onboarding is not yet connected.
Linux fake-command verification is not proof that Keychain, LaunchAgents or
case-insensitive APFS work on a real Mac.

### Separate Codex and Claude project access

The candidate's `journey` command joins browser login, project selection, reviewed
repository/skill preparation and native provisioning. Once those stages are
recorded, run the separate agent setup using the same private journey journal:

```bash
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
  journey-agents --journal /ABS/PRIVATE/journey.json
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
  journey-status --journal /ABS/PRIVATE/journey.json
```

Each selected project gets independent Codex/OpenAI and Claude/Anthropic
credentials, held only in Keychain. The owning human reviews each public request
in Access → Devices → Agent access. The operator opens that exact company page;
if already signed out, sign in and import the displayed public JSON. It contains
no credential. Project membership/offline permission is not this separate model
disclosure consent. The reviewed project manifest must also permit the exact
caller/provider destination. No API call or model invocation is made by setup.

After browser approval, review the generated settings plan and confirm it. Only
Memory-owned MCP entries are added to project `.codex/config.toml` and `.mcp.json`;
custom entries, Codex model settings and Claude's own model configuration survive.
Restart/reload the respective agent to use its new project tools, and accept its
normal project/MCP trust prompt. Those entries contain public IDs and local
profile paths, never bearer tokens or database URLs.

The MCP bridge exposes only `codifix_project_cache_status` and
`codifix_project_cache_search`: read-only, cited lexical search of that exact
project's approved cache. It cannot select another project, invoke ordinary query
tools, perform mutations, call a model or make a Trust expertise request. Results
are untrusted source data, not agent instructions. Remote F1 integration with
these new credentials remains work; existing approved remote expertise is not
automatically inherited. This scopes Codifix MCP calls; it is not an operating-
system sandbox against an assistant with unrestricted same-user filesystem or
database access. Keep private installation/profile directories outside agent
workspaces, and review the agent's own filesystem/tool permissions separately.

By default every disclosure checks current child and parent/device authority
online. Opting into `journey-agents --allow-offline` permits previously activated,
unexpired, explicitly approved access during a connectivity outage. It does not
override a known revocation or a server denial. Offline copies cannot be recalled
after transmission. The current grant has a maximum 24-hour lifetime, and
renewal is never automatic:

```bash
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
  journey-agents --journal /ABS/PRIVATE/journey.json --renew-grant OLD_AGENT_UUID
```

Replacement retains old history and denial fences, generates a new credential
and requires fresh browser consent. The new MCP key is independent; expired old
Memory entries remain fenced and can be removed manually from agent settings.
Repeat the same command after an interruption; do not delete journals or create
another login to recover it. Local status reports recorded progress, not current
remote authorization or successful native Mac acceptance.

Manual preparation/inspection uses the same implementation:

```bash
codifix projects agents prepare --company https://<company-origin> \
  --caller codex --workspace-journal /ABS/PRIVATE/project-onboarding.json --open-browser
codifix projects agents status --company https://<company-origin> --agent-id AGENT_UUID
codifix projects agents configure --company https://<company-origin> \
  --agent-id AGENT_UUID --profile /ABS/PRIVATE/installation/profiles/SUBSCRIPTION_UUID.json \
  --workspace-journal /ABS/PRIVATE/project-onboarding.json
```

`configure` previews without changing settings. Confirm the exact returned plan
with the same arguments plus `--plan-sha256 SHA256 --confirm`. Changed settings,
permissions or profile pins stop rather than overwriting your configuration.
Prepare accepts `--new-request` only to explicitly replace an expired or
definitively denied request; it cannot renew a still-active grant silently.

## Identity and project access

The company console uses Authentik for human accounts. Codifix project roles,
Git repository access, federation membership and model-disclosure consent are
separate permissions. A project assignment is not a Git credential or a grant to
send project contents to an assistant.

Workshop, Homelab and General are the eligible offline areas. Trust remains
CA-local: its selected expertise may be queried, but its database and sensitive
sources are not copied onto a laptop. Revocation stops new transfers; it cannot
recall bytes already delivered to an offline device.

## Coding and advisory models

This repository's `.codex/config.toml` selects GPT-6.1 Sol with high reasoning for
the main coding session, Sol/medium for ordinary subagents, and at most two active
subagents. `.codex/agents/astra_adviser.toml` selects GPT-6 Astra/high for bounded
planning and independent reviews. Ask Codex to use `astra_adviser` for a named
design or review question, then implement the result with Sol. Start a new session
to use changed project defaults; a live session's explicitly selected model may
take precedence.

The read-only agent default is complemented by instructions forbidding writes;
live permission overrides can still affect the agent's sandbox. It is not a
separate security boundary from the parent session. Claude retains its own model
selection and credentials.

Configuration follows the [official Codex subagent documentation](https://learn.chatgpt.com/docs/agent-configuration/subagents).
No paid model runtime is enabled by these files. A separately API-driven workflow
would need explicit per-agent models, provider credentials, disclosure policy and
budgets. The unidentified “Decision API” is not an onboarding dependency.

## Implementation environment

The approved plan was checkpointed in an isolated development service on Monolith
at `http://127.0.0.1:4250`, backed by the disposable
`memory-onboarding-dev-pg-20261005` database container. The development
configuration is outside the repository, under
`~/.local/state/memory-mac-onboarding-dev/`. Provider calls, background
work, federation and watchers are not enabled there.

This development service is not the company console deployment and contains no
customer data. The pre-existing dev overlay that points at the retired shared
database was deliberately left untouched.
