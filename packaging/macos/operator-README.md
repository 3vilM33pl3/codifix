# Explicit native onboarding operator

This is a prerequisite-based provisioning candidate, not a verified self-contained
Mac installer. The package bundles these first-party helpers read-only but does
not run Python, install PostgreSQL, log in, start services or provision agent
identities during postinstall. Native macOS/Keychain/launchd/APFS acceptance is
still pending.

## Prerequisites

Run as a non-root logged-in Mac user. Supply an already installed Python 3.11+
runtime, the installed full Memory binary (embedded PostgreSQL service and
`project-sync` support), and one reviewed installed PostgreSQL 16, 17 or 18
distribution with pgvector. These bounds are operator policy, not a claim that
all native distribution/version combinations were tested.

The PostgreSQL project's [macOS download page](https://www.postgresql.org/download/macosx/)
lists native distributions. [Postgres.app documents bundled pgvector](https://postgresapp.com/extensions/)
and its [command-line tool locations](https://postgresapp.com/documentation/cli-tools.html);
its [downloads](https://postgresapp.com/downloads.html) identify universal Mac
packages. No release version/hash is assumed by this operator. It does not run
Homebrew, fetch an installer, mount a DMG, compile Memory or build extensions.
An optional reviewed runtime-bundle packaging path now exists (see below).
Producing and verifying its real native dependency binaries remains unfinished.

Use explicit real paths, not `latest`/Homebrew symlinks. Installed executables,
extension library/control/SQL files and the operator/Python runtime are SHA-256
pinned. They and their ancestors must not be group/world-writable (a system
sticky temporary ancestor is permitted). Memory, PostgreSQL tools and vector
library must contain the current Mac execution architecture, checked with
`lipo` before executing them. Missing prerequisites, changed pins, port conflicts
and unsupported layouts fail without changing an existing installation.

Confirmed apply creates `~/Library/LaunchAgents/` if absent, without changing an
existing directory or its jobs. Supply an
existing current-user private `0700` parent for a new operator data root, and
a separate private journal/input directory. No chown, permission repair,
existing cluster adoption, default-global config fallback or destructive cleanup
is performed.

## Guided first-run with an optional runtime bundle

Packages built with the reviewed runtime option include `memory-onboard` bound
to their exact bundled Python and prerequisite descriptor. They do not depend on
an unpinned system Python or ask you to write installer JSON. After installation,
with an existing repository parent and Memory Application Support directory:

```bash
memory-onboard first-run --company https://YOUR-COMPANY-MEMORY-ORIGIN \
  --name "Olivier Mac" --workspace-parent "$HOME/Projects" \
  --state "$HOME/Library/Application Support/memory-layer/onboarding"
```

The state directory is created private if missing; existing public or relocated
state is not adopted or repaired. First-run validates the exact installed runtime
inventory, Python, architecture and Memory before login. It shows authorized
projects, accepts up to eight explicit selections and lets you reuse an existing
repository path. Trust is displayed as remote-only and cannot be selected for
offline installation. Equal names at different owners remain separate.

Review the repository/tools, offline-copy and native-services plans and type
`yes`. Current owner/device checks still run before effects. Ports default to
`55432` for PostgreSQL and `55440` upwards for the selected area services; use
`--pg-port` and `--api-base-port` for an explicit different allocation. Occupied
ports fail without stopping other installations. `--prepare-only` enrolls and
saves the selected public inputs without cloning or starting services.

Run the identical command to recover an interruption. Its private receipt stores
the full selected input before publishing `journey-input.json`; lost confirmation
does not regenerate identities or ask for another selection. Changed inputs,
runtime, manifest or executable are rejected, not silently upgraded. It reuses
the existing joined journey rather than introducing a different installation
engine. Check progress and separately approve Codex/Claude disclosure with:

```bash
memory-onboard journey-status --journal "$HOME/Library/Application Support/memory-layer/onboarding/journey.json"
memory-onboard journey-agents --journal "$HOME/Library/Application Support/memory-layer/onboarding/journey.json"
```

The builder/runtime hash tests use synthetic files. No usable native dependency
bundle or clean-Mac acceptance result is claimed. The lightweight package still
uses the explicit installed prerequisites below and has no runtime launcher.

## Inputs and preview

First enroll the device and produce credential-free, current reviewed selections
with `memory projects subscription`. Its confirmed create-only metadata contains
the stable company/area/owner/project/space, reviewed revision, authority receipt
keys, device and subscription UUID. It contains no database URL or company
credential. Exact replay retains its subscription UUID.

Prepare a private `0600` JSON operator input with exactly these fields. Placeholder
pins/UUIDs below must be replaced with actual reviewed installed-file values;
this example is not an executable default installer profile:

```json
{
  "schema_version": 1,
  "root": "/Users/YOU/PrivateMemoryInstalls/selected-installation",
  "memory": {
    "path": "/usr/local/bin/memory",
    "sha256": "ACTUAL_LOWERCASE_SHA256",
    "version": "ACTUAL_MEMORY_VERSION_OUTPUT"
  },
  "postgres": {
    "bin_dir": "/Applications/Postgres.app/Contents/Versions/16/bin",
    "major": 16,
    "tool_sha256": {
      "postgres": "ACTUAL_LOWERCASE_SHA256",
      "initdb": "ACTUAL_LOWERCASE_SHA256",
      "pg_ctl": "ACTUAL_LOWERCASE_SHA256",
      "psql": "ACTUAL_LOWERCASE_SHA256",
      "pg_config": "ACTUAL_LOWERCASE_SHA256",
      "pg_controldata": "ACTUAL_LOWERCASE_SHA256"
    },
    "vector_library": "ACTUAL_PG_CONFIG_PKGLIBDIR/vector.so",
    "vector_library_sha256": "ACTUAL_LOWERCASE_SHA256",
    "vector_control": "ACTUAL_PG_CONFIG_SHAREDIR/extension/vector.control",
    "vector_control_sha256": "ACTUAL_LOWERCASE_SHA256",
    "vector_version": "ACTUAL_INSTALLED_VERSION",
    "vector_sql": {
      "vector--ACTUAL_INSTALLED_VERSION.sql": "ACTUAL_LOWERCASE_SHA256"
    }
  },
  "pg_port": 55432,
  "areas": [
    {"name": "workshop", "instance_id": "NEW_LOCAL_AREA_UUID", "api_port": 55440}
  ],
  "subscriptions": ["/Users/YOU/PrivateSelections/project-subscription.json"]
}
```

Pin the actual SQL installation file (and any additionally reviewed vector SQL
files), not a checksum copied from this example. `pg_config --sharedir` and
`--pkglibdir` must match the pinned extension paths. Ports are explicit,
unprivileged and distinct, never silently reassigned. One to three unique
Workshop/Homelab/General areas and one to eight exact subscriptions are supported.
Every selected area must have at least one subscription. Trust is always rejected.
The new local area UUID is not the remote owner's UUID.

Using the explicit installed Python prerequisite:

```bash
/ABS/PYTHON3 -I -S -B /usr/local/share/memory-layer/onboarding/operator/onboard.py \
  prepare --input /ABS/PRIVATE/input.json --journal /ABS/PRIVATE/native-plan.json
/ABS/PYTHON3 -I -S -B /usr/local/share/memory-layer/onboarding/operator/onboard.py \
  apply --journal /ABS/PRIVATE/native-plan.json --plan-sha256 EXACT_PREVIEW_SHA256 --confirm
/ABS/PYTHON3 -I -S -B /usr/local/share/memory-layer/onboarding/operator/onboard.py \
  status --journal /ABS/PRIVATE/native-plan.json
```

Prepare records only metadata and immutable installed-tool/helper pins. Apply
freshly checks each selection through the Keychain-backed subscription producer
before local provisioning. Changed identities/revisions/receipt keys, unavailable
authority or revoked scope block effects rather than expanding privileges.
Status is local journal inspection, not current device/subscription health.

## Local effects and recovery

Apply records durable intents/receipts for layout, initdb, named PostgreSQL
LaunchAgent, each area's database/config/service and each subscription worker.
It creates only the confirmed new data root and exact new plists. Every file is
create-only or exact idempotent replay; altered settings/credentials/configs are
preserved and block continuation. Journals/configs/secrets/profiles use `0600`,
data/profile directories `0700`, descriptor-pinned no-follow paths, physical
containment checks and an exclusive journal lock. Native APFS alias behavior
remains an acceptance gate despite Linux alias fixtures.

The dedicated cluster binds 127.0.0.1 only, has Unix sockets disabled, SCRAM
passwords and reject rules for other addresses. PostgreSQL
[documents initdb/password-file authentication](https://www.postgresql.org/docs/current/app-initdb.html)
and [SCRAM](https://www.postgresql.org/docs/current/auth-password.html).
The local administrator and per-area login passwords are generated here; no
server database credentials or PDS/gateway secrets are downloaded. Passwords are
not in process arguments, logs or the metadata journal. The private local
profiles add their own loopback database URL for the scoped replica client.

Each area gets a distinct `memory_laptop_<area>` database and non-superuser login,
a distinct loopback API port and a new local area identity. Its installed Memory
service applies the existing embedded migration inventory, not duplicated SQL
migrations. Background work, reinforcement (including validation), consolidation,
embedding automation, discovery and MCP are explicitly disabled. Model settings
have an empty model and credential reference, so the current service rejects
enrichment before contacting a provider; the configured destination is loopback
port 1, not an external provider. Service read-only mode is enabled. Schema checks and a healthy primary
service with the exact configured durable service tag are required before
registering workers; unknown/unready responses do not count as success.

Labels are namespaced by the installation journal's request UUID:

- `com.memory-layer.onboarding.<request>.postgres.cluster`
- `com.memory-layer.onboarding.<request>.area.<area>`
- `com.memory-layer.onboarding.<request>.sync.<subscription>`

Workers use launchd `RunAtLoad` and `StartInterval=300` for one guarded
`memory projects sync --profile ...` invocation. They retry at a subsequent
interval after connection failure; registration is not proof of successful sync.
Inspect `memory projects sync-status --profile ...` and explicit offline search
for committed progress/citations. [Apple documents timed per-user LaunchAgents](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).
Every launch runs the pinned operator wrapper, which rechecks private files and
installed prerequisite pins; a changed installed binary blocks execution until
an explicit reviewed upgrade path exists.

No `restart-all`, `bootout`, `kickstart -k`, process killing or unrelated service
control is used. Exact loaded jobs are retained; an absent own job can be
bootstrapped on explicit apply. Lost initdb/bootstrap confirmation resumes the
same journal/request, verifies existing state and does not initialize/launch a
duplicate. A partial cluster without a valid version/control file is retained
for manual inspection, never cleared or automatically recreated.

Existing shared Memory services, clusters, config/env files, repositories,
custom Codex/Claude settings and user installations remain untouched. This native
provisioning step alone does not clone repositories or provision agent identities.
The joined journey and explicit agent stage below reuse it; neither grants company
membership or marks the full onboarding plan complete.

## Joined login and workspace journey

`onboard.py journey --input /ABS/PRIVATE/journey-input.json --journal
/ABS/PRIVATE/journey.json` joins login, project selection, cloning/approved Memory
skills, public subscription metadata and the native operator. Its schema-v1 input
contains `company`, `name`, `no_browser`, `allow_loopback_development`, the native
prerequisites above (without `subscriptions`) and one to eight output slots:
`projects: [{workspace, journal, subscription_output}]`. Each path is absolute;
private journals remain outside repository and installation roots. Each selected
area must be represented by a selected project. This v1 input remains compatible.
Guided first-run generates v2 slots with a `selection` binding the exact area,
owner, project, revision and complete public manifest digest. `projects prepare`
then checks that selection again; changed content under the same revision also
blocks continuation. It does not reopen a picker and accidentally select another
project while resuming.

Review each displayed plan, then type `yes` to confirm its exact recorded digest.
The full digest is still accepted for compatibility; you do not need to retype
64 characters. The operator retains stable request IDs, subscription IDs and
effect receipts. Re-run that same command after interruption; it rechecks current
authority and existing repositories rather than recloning or resetting them.
Git uses the user's own credentials. No arbitrary repository setup hook executes.

## Separate agent setup

After the native journey finishes:

```bash
/ABS/PYTHON3 -I -S -B /usr/local/share/memory-layer/onboarding/operator/onboard.py \
  journey-agents --journal /ABS/PRIVATE/journey.json
/ABS/PYTHON3 -I -S -B /usr/local/share/memory-layer/onboarding/operator/onboard.py \
  journey-status --journal /ABS/PRIVATE/journey.json
```

Prepare opens the company browser with a public approval request. The actual
owning OIDC human must confirm separate Codex/OpenAI and Claude/Anthropic project
disclosure grants. If signed out, sign in and import the displayed public JSON
into Access → Devices → Agent access. Tokens remain in Keychain; the Python
operator and public approval URL never receive them. The project manifest must
permit each destination. Only then confirm the generated Memory MCP settings
changes, which preserve custom agent settings and Claude's independent model.

The bridge provides only exact-project read-only cache status/search. It enables
no model call, ordinary query, mutation or Trust remote expertise. Reload each
agent and accept its normal project/MCP trust prompt before use.

Optional `--allow-offline` permits explicitly consented, previously activated,
unexpired cache disclosure during network disconnection. Known denials stay
fenced; server errors do not silently enable offline disclosure. Grants expire
within 24 hours. To replace an expired or denied grant, repeat `journey-agents`
with `--renew-grant OLD_AGENT_UUID` (and the original offline flag, if selected).
This retains old history, creates independent new credentials, and requests fresh
browser consent; no automatic consent extension occurs. Remove expired Memory
MCP entries manually if desired; they cannot be revived by renewal.

The separate `.agents.json` journal contains public metadata/digests only.
`journey-status` reports its recorded progress, not current authority. Exact
replay recovers a lost settings acknowledgement without repeating clones,
credentials or completed confirmations. Settings changed since review are
preserved and require a new explicit configuration review.

## Offline validation only

```bash
python3 packaging/macos/tests/test_onboarding_operator.py
python3 packaging/macos/tests/test_package_contract.py
python3 packaging/macos/tests/test_onboarding_journey.py
python3 packaging/macos/tests/test_onboarding_agents.py
bash -n packaging/build-pkg.sh
```

Tests inject fake native commands and synthetic files/metadata. Native package
installation/signing, Python/PG/pgvector prerequisites, architecture checks,
Keychain approval, launchd restart/login behavior, initial live sync and
case-insensitive APFS all still need Mac acceptance. No native install or live
provider/model call is evidenced by these tests.
