# Mac project onboarding

This guide covers the macOS native onboarding candidate in current source on
`main`. Start with [team setup](https://codifix.dev/docs/team-setup) for the
platform-neutral access and project journey. The published `3.0.0-rc.1` standard
package contains client/operator tooling, not the optional combined runtime
bundle. The post-candidate Mac binding and boolean-serialization fixes are
separately documented in its release-specific operator procedure.

The development package includes resumable sign-in, project selection,
repository preparation and offline synchronization, followed by a separate
Codex/Claude access stage. Optional runtime-bundle packaging and guided first-run
exist. Recorded ARM64 candidate observations include sign-in, project selection,
repository setup, two project downloads, local queries, curated uploads,
reviewed corrections and process restart recovery. These are bounded historical
observations, not proof of a clean installation of the final combined package.
Host reboot, live device revocation and that clean installation remain unverified;
the observed installation includes repair receipts.

The [execution contract](https://github.com/3vilm33pl3/codifix/blob/main/docs/reviews/mac-onboarding/README.md)
and [historical deployment ledger](https://github.com/3vilm33pl3/codifix/blob/main/docs/reviews/mac-onboarding/company-deployment-20261015.md)
retain the original evidence and remaining checks.

## Using an already configured laptop

From a selected repository run `codifix tui`; operator-owned routing chooses its
local area. Outside a mapped repository, select `--area workshop` or
`--area homelab`. An unavailable area does not redirect writes elsewhere.
See [areas and projects](https://codifix.dev/docs/areas) for context selection.

Use Query for ordinary retrieval and Web **More → Offline** or TUI **Offline**
for subscription freshness and owner versions. The updated TUI Memories view
marks owner copies read-only. The Web Memories local count does not count those
downloaded versions. See [offline and synchronization](https://codifix.dev/docs/offline)
for read-only status/search, upload receipts and corrections. Raw captures and
session bodies stay local; setup enables no paid curation. Git actions and owner
correction approval remain separate.

## Guided first-run candidate

An operator-built package containing the reviewed runtime includes
`codifix-onboard`. The existing lightweight package does not include that launcher.
With your existing projects folder, start:

```bash
codifix-onboard first-run --company https://knowledge.example.com \
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

On macOS, allow Codifix's Local Network access when prompted so it can reach your
private company server. Check **System Settings → Privacy & Security → Local
Network** if a desktop process cannot reach the server although SSH/curl can.
Those processes can have different privacy permissions; successful curl alone
does not prove that Codifix's background services can connect. Keep TLS validation
enabled and verify actual service synchronization after granting access.

### Full curated-project synchronization candidate

Add `--curated-sync` to first-run for the candidate's two-way project mode.
During browser device approval, explicitly consent to curated contributions for
each eligible project. Existing reader devices gain no write access automatically.
See [offline and synchronization](https://codifix.dev/docs/offline#publication-copies-and-curated-project-copies)
for copied data, upload receipts, owner review, history and correction commands.
This remains an internal candidate; mutable-state, relation and deletion uploads
are not an owner-write protocol.

The state parent and projects folder must exist and remain separate. First-run
creates new private state and does not change an existing directory's permissions.
For occupied ports, choose explicit `--pg-port`/`--api-base-port` values. Confirmed
native apply creates a missing user LaunchAgents directory and only namespaced
jobs. `--prepare-only` saves selections without cloning or starting services.

After interruption repeat the same command and inputs. It retains selections,
identifiers and completed effects. Changed manifests or executables require new
review. After the journey completes, separately approve the owning-human
Codex/OpenAI and Claude/Anthropic grants:

```bash
codifix-onboard journey-agents --journal "$HOME/Library/Application Support/codifix/onboarding/journey.json"
codifix-onboard journey-status --journal "$HOME/Library/Application Support/codifix/onboarding/journey.json"
```

These commands enable no paid model calls. Current implementation supports the
reviewed Memory skills format, not arbitrary plugins. Raw drafts do not upload
through the curated contribution queue. Trust expertise integration with these
new laptop identities remains outside this increment.

Observed native operation covers two projects, not a clean-Mac replay of the
final combined package. The
[runtime packaging and recovery runbook](https://github.com/3vilm33pl3/codifix/blob/main/packaging/macos/operator-README.md)
records prerequisites and remaining native validation.

## Administration in the development build

Use [company administration](https://codifix.dev/docs/team-setup/administration)
for Users, Projects, Devices, company grants, revision conflicts and independent
device revocation. Company administration requires `system_admin`; project
administration alone is insufficient. Browser mutations require CSRF protection
and a writable service.

### Selective owner delivery setup

Owner delivery needs separate server-side delegation and credentials from
membership management. Follow
[selective device delivery](https://codifix.dev/docs/team-setup/administration#enable-selective-device-delivery).
Trust cannot opt into offline delivery.

### Optional Authentik management

The optional server-only Authentik adapter is disabled by default. Its provider
permissions and pending operations require explicit operator review; the adapter
does not automatically send invitation email. See
[Authentik management](https://codifix.dev/docs/team-setup/administration#optional-authentik-management).
Live provider RBAC remains unverified by this development work.

## Device approval in the development build

Follow [device review](https://codifix.dev/docs/team-setup#sign-in-and-review-the-device)
for exact project, offline-copy and curated-write consent. Company return URLs
include `company_device=1`. The older `/?device_code=ABCD-1234` local-owner flow
remains separate; a company error never downgrades into it. A recorded company
approval intent cannot be claimed by local approval.

The cross-area approval and selective delivery implementation exists in current
source. Pending owner operations retain their stable IDs and immutable selections;
partial owner acceptance is not completed enrollment. Native Mac acceptance of
the final combined journey remains unfinished.

## Journaled workspace preparation in the development build

Use [project preparation](https://codifix.dev/docs/team-setup#prepare-and-apply-a-project)
for `projects available`, `select`, `prepare`, `apply` and `status`, immutable
journals and preservation of existing repositories/settings. The native candidate
requires the approved Keychain device credential; Linux fake-store tests are not
a production plaintext fallback. The bounded native observations above do not
establish every filesystem or installer acceptance case.

The pinned installer currently accepts only the reviewed `memory-skills` ZIP
layout. Missing known skills are installed into `.agents/skills/` for Codex and
`.claude/skills/` for Claude; custom skills survive. Scripts are not executed or
made executable. Unknown formats and failed hashes/versions stop with completed
journal stages retained. Journal containment uses physical filesystem identities
and canonical ancestors. Linux alias fixtures are covered; native
case-insensitive APFS validation remains pending.

Project preparation alone does not provision PostgreSQL, start services, build
source, enable paid model calls or complete selective synchronization. Native
provisioning and the separate agent access stage follow below.

## Selective synchronization commands in the development build

See [offline and synchronization](https://codifix.dev/docs/offline) for
`projects sync`, `sync-status`, `offline-search`, receipt verification and
contribution/correction behavior. Profiles must come from the reviewed operator;
never point them at an existing server database. Native credential storage uses
Keychain. A lean binary reports missing `project-sync` explicitly.

Company devices use exact area/owner/project scope and do not fall through to
local-owner endpoints, broad exports or whole-space federation pulls. Web and TUI
cache views require the operator's `[laptop_cache]` bindings. Their read-only cache
search does not itself call a model or another instance. Ordinary retrieval and
cache views have distinct paths; Linux tests alone do not establish native Mac
behavior.

### Native provisioning candidate

Export public trust metadata for an exact selected project. The parent directory
must already be private (0700); the output is create-only (0600), never a device
credential or a server database connection:

```bash
codifix projects subscription --company https://knowledge.example.com \
  --area workshop --owner-instance 11111111-1111-4111-8111-111111111111 \
  --project 22222222-2222-4222-8222-222222222222 --revision 1 \
  --output /Users/example/.config/codifix/subscription.json
codifix projects subscription --company https://knowledge.example.com \
  --area workshop --owner-instance 11111111-1111-4111-8111-111111111111 \
  --project 22222222-2222-4222-8222-222222222222 --revision 1 \
  --output /Users/example/.config/codifix/subscription.json \
  --bootstrap-sha256 DIGEST_FROM_PREVIEW --confirm
```

An identical confirmed replay retains the local subscription UUID. A different
device, manifest, receipt-key schedule or existing output stops without replacing
the file. The operator consumes this metadata and adds only its separately
generated loopback database credential.

The candidate package bundles `onboard.py` and its operator instructions under
`/usr/local/share/codifix/onboarding/operator/`. Follow the
[native operator runbook](https://github.com/3vilm33pl3/codifix/blob/main/packaging/macos/operator-README.md) for the exact
reviewed input, preview and confirmed apply. It currently requires an installed,
architecture-matched Codifix binary, Python 3.11+ and a reviewed native
PostgreSQL/pgvector toolset. It does not download these prerequisites or build
source on the laptop. This earlier prerequisite-based operator path is distinct
from the optional runtime-bundled first-run candidate described above.

The operator creates only fresh, per-user area services and a private loopback
cluster, schedules each selected project every five minutes, preserves unrelated
launchers/services, and resumes the same journal. The earlier prerequisite-only
stage provided read-only local area services;
the later combined curated-sync candidate adds the separate contribution queue
and observed upload/correction paths described above.
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
caller/provider destination. Setup makes no model-provider API call or model
invocation.

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
codifix projects agents prepare --company https://knowledge.example.com \
  --caller codex --workspace-journal /ABS/PRIVATE/project-onboarding.json --open-browser
codifix projects agents status --company https://knowledge.example.com --agent-id AGENT_UUID
codifix projects agents configure --company https://knowledge.example.com \
  --agent-id AGENT_UUID --profile /ABS/PRIVATE/installation/profiles/SUBSCRIPTION_UUID.json \
  --workspace-journal /ABS/PRIVATE/project-onboarding.json
```

`configure` previews without changing settings. Confirm the exact returned plan
with the same arguments plus `--plan-sha256 SHA256 --confirm`. Changed settings,
permissions or profile pins stop rather than overwriting your configuration.
Prepare accepts `--new-request` only to explicitly replace an expired or
definitively denied request; it cannot renew a still-active grant silently.

## Identity and project access

Human roles, Git access, federation membership and model-disclosure consent are
separate permissions. Workshop, Homelab and General are eligible offline areas;
Trust remains owner-local and available only through separately approved remote
expertise. Revocation cannot recall delivered bytes. See
[team setup](https://codifix.dev/docs/team-setup) and
[areas and projects](https://codifix.dev/docs/areas).

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

Historical implementation tests used an isolated development service and
disposable database, with provider calls, background work, federation and
watchers disabled. Those fixtures were not the company deployment and contained
no customer data. They establish bounded development evidence, not clean-Mac
acceptance or production service health.
