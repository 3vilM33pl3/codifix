# Explicit native onboarding operator

This is a prerequisite-based provisioning candidate, not a verified self-contained
Mac installer. The package bundles these first-party helpers read-only but does
not run Python, install PostgreSQL, log in, start services or provision agent
identities during postinstall. Native macOS/Keychain/launchd/APFS acceptance is
still pending.

## macOS local-network permission

Packages include **Codifix Network Access.app**. Open it once with the configured
company origin, for example:

```bash
open -a 'Codifix Network Access' --args https://company.example
```

Allow the macOS local-network prompt. The helper checks HTTPS reachability without
reading credentials; its success message does not claim successful synchronization.
Schema-v2 synchronization LaunchAgents run their existing verified operator under
this application so macOS attributes background network access consistently.
The helper pins Python and operator bytes, and the operator still verifies its
journal, Codifix executable, profiles and current permissions. Startup failures
retry after 30 seconds; stopping the job terminates its child with a five-second
grace period. Legacy schema-v1 periodic jobs retain their original lifecycle.

This follows [Apple's local-network privacy guidance](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy).
For internal unsigned candidates the app is signed ad hoc; native permission
behavior must be retested after replacing it. Release packages use the provided
Developer ID identity when configured. No system-wide network exception is added.

## Prerequisites

Run as a non-root logged-in Mac user. Supply an already installed Python 3.11+
runtime, the installed full Codifix binary (embedded PostgreSQL service and
`project-sync` support), and one reviewed installed PostgreSQL 16, 17 or 18
distribution with pgvector. These bounds are operator policy, not a claim that
all native distribution/version combinations were tested.

The PostgreSQL project's [macOS download page](https://www.postgresql.org/download/macosx/)
lists native distributions. [Postgres.app documents bundled pgvector](https://postgresapp.com/extensions/)
and its [command-line tool locations](https://postgresapp.com/documentation/cli-tools.html);
its [downloads](https://postgresapp.com/downloads.html) identify universal Mac
packages. No release version/hash is assumed by this operator. It does not run
Homebrew, fetch an installer, mount a DMG, compile Codifix or build extensions.
An optional reviewed runtime-bundle packaging path now exists (see below).
Producing and verifying its real native dependency binaries remains unfinished.

The native source producer builds a candidate without installing it or touching
existing services. Use a new explicit directory on macOS with Xcode command-line
tools, an existing Python interpreter and `make`:

```bash
python3 packaging/macos/build-runtime.py \
  --work-dir /ABS/NEW-BUILD-DIRECTORY \
  --bundle-id py3148-pg1711-vector087-aarch64-v1 --jobs 8
```

`runtime-sources.json` pins official PostgreSQL 17.11, pgvector 0.8.7 and
CPython 3.14.8 source bytes. The producer verifies downloads, stages through
`DESTDIR` for the exact versioned prefix, audits native loader dependencies and
applies ad-hoc signatures before hashing. Re-run the same command with `--resume`
after interruption; changing sources, architecture or prefix requires a fresh
directory. The output gives the bundle and manifest arguments for `build-pkg.sh`.
This Python runtime serves the first-party operator; pip and TLS capabilities
are not prerequisites or promised features. Ad-hoc signing is not Apple Developer
ID signing or notarization. Validate the produced runtime on the actual Mac,
including `_ctypes`, loader behavior and PostgreSQL/pgvector before acceptance.

Use explicit real paths, not `latest`/Homebrew symlinks. Installed executables,
extension library/control/SQL files and the operator/Python runtime are SHA-256
pinned. They and their ancestors must not be group/world-writable (a system
sticky temporary ancestor is permitted). Codifix, PostgreSQL tools and vector
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

Packages built with the reviewed runtime option include `codifix-onboard` bound
to their exact bundled Python and prerequisite descriptor. They do not depend on
an unpinned system Python or ask you to write installer JSON. After installation,
with an existing repository parent and Codifix Application Support directory:

```bash
codifix-onboard first-run --company https://YOUR-COMPANY-MEMORY-ORIGIN \
  --name "Olivier Mac" --workspace-parent "$HOME/Projects" \
  --state "$HOME/Library/Application Support/codifix/onboarding"
```

The state directory is created private if missing; existing public or relocated
state is not adopted or repaired. First-run validates the exact installed runtime
inventory, Python, architecture and Codifix before login. It shows authorized
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
codifix-onboard journey-status --journal "$HOME/Library/Application Support/codifix/onboarding/journey.json"
codifix-onboard journey-agents --journal "$HOME/Library/Application Support/codifix/onboarding/journey.json"
```

The builder/runtime hash tests use synthetic files. No usable native dependency
bundle or clean-Mac acceptance result is claimed. The lightweight package still
uses the explicit installed prerequisites below and has no runtime launcher.

### Curated project synchronization and local work

For a compatible company/device service, add `--curated-sync` to first-run. This
selects `curated_project_v1` explicitly in each reviewed subscription and binds
native schema 2 to the exact selected repository, project and area. An older
service or CLI that cannot issue this profile blocks provisioning before local
services are created. Existing schema-1 journeys remain read-only.

Schema 2 enables local capture and the offline queue in each selected area and
installs one named watcher LaunchAgent per selected repository. These jobs use
the exact area configuration and repository path; they do not discover or start
watchers for unrelated repositories. The private `area-routing.json` is under
the native state root and can be used with `codifix --area-routing PATH` for
interactive commands. Missing General remains unavailable rather than routing
unknown work to Workshop or Homelab. Paid curation and model execution remain
disabled until separately configured through the normal application settings.
An installation-owned `bin/codifix` launcher also supplies this routing argument;
invoke it from a selected repository or add this installation's `bin` directory
to your PATH. It does not replace an existing global Codifix command.
Curated profiles bind the reviewed manifest digest and the local repository to
the exact remote project ID. Selecting this workflow requires writer access and
the company browser approval must explicitly permit curated contributions.
Existing shared-publication agent grants do not authorize sending the larger
curated corpus to Codex or Claude. `journey-agents` prepares separate identities
with `curated_project_v1` disclosure explicitly shown in the company browser.
Only its matching approved receipt can read a curated profile; legacy grants
cannot be reused for the larger corpus.

Before watchers start, the first curated synchronization initializes the exact
local project and its outbound binding. A failed initial synchronization retains
the journal and leaves watchers off until recovery succeeds. The curated sync
LaunchAgent then runs `--watch`: it checks queued contributions every two seconds
and downloads remote updates on the normal five-minute schedule. Legacy shared
profiles retain their interval-based one-shot synchronization jobs.

Watcher stages, service stages and five-minute synchronization jobs retain their
immutable journal identities across interruption and reboot. Rerun the same
first-run invocation to recover an interrupted installation. Existing loaded
jobs are not restarted or duplicated. Changes to routing, area configuration,
subscription mode or reviewed runtime pins require explicit repair rather than
silently changing an existing journey. The native Mac acceptance demonstration
must still verify capture, curated contributions, restart and offline replay.

## Inputs and preview

First enroll the device and produce credential-free, current reviewed selections
with `codifix projects subscription`. Its confirmed create-only metadata contains
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
  "codifix": {
    "path": "/usr/local/bin/codifix",
    "sha256": "ACTUAL_LOWERCASE_SHA256",
    "version": "ACTUAL_CODIFIX_VERSION_OUTPUT"
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
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
  prepare --input /ABS/PRIVATE/input.json --journal /ABS/PRIVATE/native-plan.json
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
  apply --journal /ABS/PRIVATE/native-plan.json --plan-sha256 EXACT_PREVIEW_SHA256 --confirm
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
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

The dedicated cluster administrator is `codifix_onboarding_admin`. Each area
gets a distinct `codifix_laptop_<area>` database and non-superuser login,
a distinct loopback API port and a new local area identity. Its installed Codifix
service applies the existing embedded migration inventory, not duplicated SQL
migrations. Background work, reinforcement (including validation), consolidation,
embedding automation, discovery and MCP are explicitly disabled. Model settings
have an empty model and credential reference, so the current service rejects
enrichment before contacting a provider; the configured destination is loopback
port 1, not an external provider. Service read-only mode is enabled. Schema checks and a healthy primary
service with the exact configured durable service tag are required before
registering workers; unknown/unready responses do not count as success.

Labels are namespaced by the installation journal's request UUID:

- `dev.codifix.onboarding.<request>.postgres.cluster`
- `dev.codifix.onboarding.<request>.area.<area>`
- `dev.codifix.onboarding.<request>.sync.<subscription>`

Workers use launchd `RunAtLoad` and `StartInterval=300` for one guarded
`codifix projects sync --profile ...` invocation. They retry at a subsequent
interval after connection failure; registration is not proof of successful sync.
Inspect `codifix projects sync-status --profile ...` and explicit offline search
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

Existing shared Codifix services, clusters, config/env files, repositories,
custom Codex/Claude settings and user installations remain untouched. This native
provisioning step alone does not clone repositories or provision agent identities.
The joined journey and explicit agent stage below reuse it; neither grants company
membership or marks the full onboarding plan complete.

## Joined login and workspace journey

`onboard.py journey --input /ABS/PRIVATE/journey-input.json --journal
/ABS/PRIVATE/journey.json` joins login, project selection, cloning/approved Codifix
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
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
  journey-agents --journal /ABS/PRIVATE/journey.json
/ABS/PYTHON3 -I -S -B /usr/local/share/codifix/onboarding/operator/onboard.py \
  journey-status --journal /ABS/PRIVATE/journey.json
```

Prepare opens the company browser with a public approval request. The actual
owning OIDC human must confirm separate Codex/OpenAI and Claude/Anthropic project
disclosure grants. If signed out, sign in and import the displayed public JSON
into Access → Devices → Agent access. Tokens remain in Keychain; the Python
operator and public approval URL never receive them. The project manifest must
permit each destination. Only then confirm the generated Codifix MCP settings
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
browser consent; no automatic consent extension occurs. Remove expired Codifix
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
