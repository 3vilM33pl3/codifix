# Getting Started

This guide is written for someone who just wants Codifix working with as little setup friction as possible.

The current distribution is the [Codifix v3.0.0 stable release](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0).
These instructions follow current `main`; source fixes after the candidate are
not included in its packages. If you are joining an existing installation,
start with [Team setup](https://codifix.dev/docs/team-setup) instead of creating
another database. Moving from Memory Layer requires the
[release-specific update procedure](https://codifix.dev/docs/install/update).

When developing Codifix itself, use the repository's shared memory helper
for durable project notes: it explicitly selects the Workshop API. The separate
dev backend/database is for testing. See [project memory routing and dev recovery](../developer/dev-stack.md#project-memory-routing-and-recovery).

## Table of Contents

- [Prerequisites](#prerequisites)
- [PostgreSQL Requirement](#postgresql-requirement)
- [Agent Install Prompt](#agent-install-prompt)
- [Agent Repo Memory Init Prompt](#agent-repo-memory-init-prompt)
- [Fast Install: Debian](#fast-install-debian)
- [Fast Install: macOS](#fast-install-macos)
- [Fast Install: Windows](#fast-install-windows)
- [What The Wizard Will Ask For](#what-the-wizard-will-ask-for)
- [File Locations](#file-locations)
- [What To Put Where](#what-to-put-where)
- [Writer ID](#writer-id)
- [Primary And Relay Services](#primary-and-relay-services)
- [Daily Use](#daily-use)
- [Shell Completion](#shell-completion)
- [Optional Background Watcher](#optional-background-watcher)
- [Upgrading An Existing Install](#upgrading-an-existing-install)
- [Using `scan`](#using-scan)
- [Web UI Notes](#web-ui-notes)
- [Importing Commit History](#importing-commit-history)
- [Running From Source](#running-from-source)

## Prerequisites

Before you install or run the wizard, have these ready:

- a PostgreSQL database that already exists and that Codifix can own
- a PostgreSQL connection string for that database
- a project directory or repository you want Codifix to manage
- optional: an OpenAI-compatible API key if you want `codifix scan`
- PostgreSQL with `pgvector` installed and `CREATE EXTENSION vector` enabled in the Codifix database
- `go` on `PATH` if you plan to use the repo-local Codifix skills through `go run`

You do not need to invent a Codifix service token yourself for normal installs. The setup wizard provisions a machine-local token in `codifix.env`, and local write-capable tools use that token to authenticate to `codifix-service`. Debian package installation only stages files: it does not provision or rotate credentials, or start/restart services. Review configuration and explicitly start only the intended instance after setup.

## PostgreSQL Requirement

Codifix stores durable memories in PostgreSQL. The backend cannot become healthy until the database URL points at a reachable PostgreSQL database that already exists. `codifix wizard --global` writes the URL into config; it does not create the PostgreSQL database for you. Semantic retrieval and current embedding migrations also require pgvector.

There are two pgvector steps, and both matter:

1. Install pgvector on the PostgreSQL server.
2. Enable the `vector` extension inside the specific database Codifix uses.

The extension is per database. Enabling it in `postgres` does not enable it in `codifix`.

Example database URL:

```text
postgres://codifix:<password>@127.0.0.1:5432/codifix
```

### Existing Or Hosted PostgreSQL

Use this path when you already have PostgreSQL, including a hosted provider:

1. Confirm the provider or server supports pgvector.
2. Create a dedicated database and user, or ask your database admin for a URL to an existing database.
3. From the machine that will run Codifix, verify connectivity:

```bash
export DATABASE_URL='postgres://codifix:<password>@db-host:5432/codifix'
psql "$DATABASE_URL" -c "SELECT 1;"
```

4. Enable pgvector in the target database:

```bash
psql "$DATABASE_URL" -c "CREATE EXTENSION IF NOT EXISTS vector;"
psql "$DATABASE_URL" -c "SELECT extversion FROM pg_extension WHERE extname = 'vector';"
```

If the `CREATE EXTENSION` command fails with a permissions error, ask the database admin to run it or grant extension privileges for that database.

### Local Debian Or Ubuntu PostgreSQL

Use this when you want PostgreSQL on the same machine as Codifix:

```bash
sudo apt-get update
sudo apt-get install -y postgresql postgresql-contrib
pg_config --version
```

Install the pgvector package that matches your PostgreSQL server major version. For PostgreSQL 16, the package is commonly:

```bash
sudo apt-get install -y postgresql-16-pgvector
```

If your server is a different major version, replace `16` with that version. If the package is unavailable from your OS repositories, install it from the PostgreSQL Global Development Group packages or follow the upstream pgvector installation instructions.

Create a dedicated user and database before running `codifix wizard --global`:

```bash
sudo -u postgres createuser --pwprompt codifix
sudo -u postgres createdb --owner=codifix codifix
export DATABASE_URL='postgres://codifix:<password>@127.0.0.1:5432/codifix'
```

Enable and verify pgvector:

```bash
psql "$DATABASE_URL" -c "CREATE EXTENSION IF NOT EXISTS vector;"
psql "$DATABASE_URL" -c "SELECT 1;"
psql "$DATABASE_URL" -c "SELECT extversion FROM pg_extension WHERE extname = 'vector';"
```

### Local macOS PostgreSQL

Use this when you want Homebrew PostgreSQL on the same Mac as Codifix:

```bash
brew install postgresql@16 pgvector
brew services start postgresql@16
```

Create a dedicated user and database before running `codifix wizard --global`:

```bash
createuser --pwprompt codifix
createdb --owner=codifix codifix
export DATABASE_URL='postgres://codifix:<password>@127.0.0.1:5432/codifix'
```

Enable and verify pgvector:

```bash
psql "$DATABASE_URL" -c "CREATE EXTENSION IF NOT EXISTS vector;"
psql "$DATABASE_URL" -c "SELECT 1;"
psql "$DATABASE_URL" -c "SELECT extversion FROM pg_extension WHERE extname = 'vector';"
```

### Local Windows PostgreSQL

The Windows MSI includes `share\codifix\postgres.compose.yaml`. With Docker Desktop running, copy that file and create a private `.env` beside it:

```powershell
$postgres = Join-Path $env:LOCALAPPDATA 'codifix\postgres'
New-Item -ItemType Directory -Force $postgres | Out-Null
Copy-Item "$env:LOCALAPPDATA\Programs\Codifix\share\codifix\postgres.compose.yaml" "$postgres\compose.yaml"
# Generate and store a strong CODIFIX_POSTGRES_PASSWORD in $postgres\.env.
docker compose --project-directory $postgres -f "$postgres\compose.yaml" up -d
```

The template runs PostgreSQL 16 with pgvector, binds only to `127.0.0.1:5432`, and keeps data in a named Docker volume. Do not commit the local `.env` file.

After Codifix is configured, run:

```bash
codifix doctor
codifix health
```

`codifix doctor` should not report a missing database URL or missing pgvector before you treat the install as complete.

## Agent Install Prompt

Use the [agent setup prompts](https://codifix.dev/docs/install/agent-setup) for
a standalone installation. They reference the current package guides, checksums
and prerequisites rather than duplicating platform commands here. For an
existing organization, follow [Team setup](https://codifix.dev/docs/team-setup).

## Agent Repo Memory Init Prompt

If Codifix is already installed, use the repository-initialization prompt on the
same [agent setup page](https://codifix.dev/docs/install/agent-setup#prompt-for-initializing-your-repository).
Inspect existing `.codifix/` and `.agents/` files, verify the selected area,
preview with `codifix wizard --dry-run`, and preserve customizations.

## Fast Install: Debian

1. Download the architecture-matched Codifix release and verify its checksum
   using the [Linux install guide](https://codifix.dev/docs/install/linux-debian).
   Debian release filenames use the stable `3.0.0` version; follow the guide's
   exact package name and checksum instructions.
2. Install it:

```bash
# x86_64
sudo dpkg -i codifix_<version>_amd64.deb

# arm64, including Raspberry Pi 4/5 with a 64-bit OS
sudo dpkg -i codifix_<version>_arm64.deb
```

3. Create the PostgreSQL database and prepare pgvector using the [PostgreSQL Requirement](#postgresql-requirement) section. Do not continue until these commands work:

```bash
psql "$DATABASE_URL" -c "SELECT 1;"
psql "$DATABASE_URL" -c "SELECT extversion FROM pg_extension WHERE extname = 'vector';"
```

4. Configure the shared/global settings once on this machine:

```bash
codifix wizard --global
```

This is where you set the shared database URL and explicitly provision the shared service API token. The Debian installation hook does not generate or rotate a token. For a manually configured system service, use `sudo codifix --config /etc/codifix/codifix.toml service ensure-api-token --json` after reviewing that configuration; replacing an existing credential is a separate operator action. A writer ID is optional; if you do not set one, Codifix derives a stable writer identity automatically.

5. Go to the project you want to use:

```bash
cd /path/to/your-project
```

6. Preview and run the repo-local Codifix setup wizard:

```bash
codifix wizard --dry-run
codifix wizard
```

The repo-local skill bundle that `codifix wizard` installs uses a shared Go helper under `.agents/skills/codifix/scripts/`, so agent-driven skill usage in that repository requires `go` to be available on `PATH`.

Most mutating `codifix` commands also support `--dry-run`, so you can preview setup, write, indexing, bundle, and checkpoint operations before they touch local files, services, or backend state.

7. Start the backend service:

```bash
sudo systemctl enable --now codifix.service
```

8. Verify the setup and open the UI you prefer:

```bash
codifix doctor
codifix health
codifix tui
```

## Fast Install: macOS

1. Prefer the architecture-matched unsigned release package described in the
   [macOS install guide](https://codifix.dev/docs/install/macos-homebrew).
   Homebrew builds the checksum-pinned v3.0.0 release source:

```bash
brew tap 3vilM33pl3/codifix https://github.com/3vilM33pl3/codifix
brew install 3vilM33pl3/codifix/codifix
```

This compiles the stable release locally. Use `--HEAD` only to opt into unreleased changes.

2. Create the PostgreSQL database and prepare pgvector using the [PostgreSQL Requirement](#postgresql-requirement) section. Do not continue until these commands work:

```bash
psql "$DATABASE_URL" -c "SELECT 1;"
psql "$DATABASE_URL" -c "SELECT extversion FROM pg_extension WHERE extname = 'vector';"
```

3. Configure the shared/global settings once on this machine:

```bash
codifix wizard --global
```

4. Go to the project you want to use:

```bash
cd /path/to/your-project
```

5. Preview and run the repo-local Codifix setup wizard:

```bash
codifix wizard --dry-run
codifix wizard
```

The repo-local skill bundle that `codifix wizard` installs uses a shared Go helper under `.agents/skills/codifix/scripts/`, so agent-driven skill usage in that repository requires `go` to be available on `PATH`.

6. Start the backend LaunchAgent:

```bash
codifix service enable
```

7. Verify the setup and open the TUI:

```bash
codifix doctor
codifix health
codifix tui
```

or in a browser:

```text
http://127.0.0.1:4040/
```

## Fast Install: Windows

1. Follow the [Windows preview guide](https://codifix.dev/docs/install/windows)
   to verify the unsigned x86_64 MSI and install the separate VC++ x64 runtime.
   It installs per user under `%LOCALAPPDATA%\Programs\Codifix` and adds its
   `bin` directory to the user `PATH`. It does not bundle a complete native
   PostgreSQL/PDS/Pi stack.
2. Start the bundled PostgreSQL Compose template using the [Local Windows PostgreSQL](#local-windows-postgresql) instructions.
3. Open a new PowerShell window and configure Codifix:

```powershell
codifix wizard --global
Set-Location C:\path\to\your-project
codifix wizard --dry-run
codifix wizard
codifix service enable
codifix doctor
codifix health
codifix tui
```

The backend is a current-user, least-privilege Task Scheduler task named
`Codifix-Backend`. If you want Codex-linked background watcher automation,
enable the separate optional task:

```powershell
codifix watcher manager enable
```

Its task is named `Codifix-WatcherManager`. Backend and watcher-manager
config, state, cache, and task metadata live under
`%LOCALAPPDATA%\codifix`.

## What The Wizard Will Ask For

The wizard can set up:

- shared/global settings when that scope is enabled:
  - the PostgreSQL database URL
  - the shared service API token override, if you want to replace the auto-generated one
  - an optional shared `writer.id` advisory label
- optional LLM settings for `scan`
- user-local project config plus the repo-local `.codifix/project.toml` marker
- optional watcher setup
- the repo-local memory skill bundle, which uses a shared Go helper under `.agents/skills/codifix/scripts/`

The repo-local skill bundle contains:

- `codifix`
- `codifix-project-init`
- `codifix-github-init`
- `codifix-query-resume`
- `codifix-review-proposals`
- `codifix-plan-execution`
- `codifix-direct-task-start`
- `codifix-remember`

Important detail:

- inside a repository, `codifix wizard` is local-first by default
- use `codifix wizard --global` when you want to edit the shared/global config
- or enable `shared/global setup` in the first wizard step

## File Locations

### Shared configuration

Debian install:

- `/etc/codifix/codifix.toml`
- `/etc/codifix/codifix.env`

macOS install:

- `~/Library/Application Support/codifix/codifix.toml`
- `~/Library/Application Support/codifix/codifix.env`

Local install:

- `~/.config/codifix/codifix.toml`
- `~/.config/codifix/codifix.env`

### Per-project configuration

Current installs keep operational project files in your home directory, not next to `.git`:

- Linux local config: `~/.config/codifix/projects/<project-key>/config.toml`
- Linux local env: `~/.config/codifix/projects/<project-key>/codifix.env`
- Linux local state/runtime: `~/.local/state/codifix/projects/<project-key>/runtime/`
- Linux local cache/indexes: `~/.cache/codifix/projects/<project-key>/`

Inside each project repository:

- `.codifix/project.toml`
- `.agents/codifix.toml`
- `.agents/skills/`

Older repositories may still have `.codifix/config.toml`, `.codifix/codifix.env`, or `.codifix/runtime/`. Codifix reads those as legacy fallback. Run `codifix doctor --fix` to copy missing legacy config/env files to the user-local project layout; it does not delete the old files.

### Installed skill template

`codifix wizard`, `codifix init`, and `codifix upgrade` copy Memory-owned skills from the installed `skill-template` into the project-local `.agents/skills/` directory.

Template locations:

- Debian package: `/usr/share/codifix/skill-template/`
- local Linux install script: `~/.local/share/codifix/skill-template/` unless `XDG_DATA_HOME` overrides it
- macOS `.pkg`: `/usr/local/share/codifix/skill-template/`
- Homebrew: `$(brew --prefix)/share/codifix/skill-template/`
- source/dev checkout: the repo's `.agents/skills/`

## What To Put Where

### Shared/global config

Use this for values shared by many repos:

- `database.url`
- `service.api_token`
- `[cluster]` settings for backend relay discovery on a local network
- `[llm]` settings
- `[[embeddings.backends]]` — one block per embedding backend you want available (OpenAI, Voyage, Cohere, Gemini, Ollama). See [Embedding Operations](cli/embeddings.md#configuring-multiple-backends) for the full shape and the "enable two backends from day one" workflow.

The shared service API token normally lives in the adjacent `codifix.env` file and is provisioned automatically during setup.

### Repo-local config

Use this for project-specific overrides:

- watcher settings
- local backend ports
- project-specific DB override if needed
- repo-specific `writer.id` override if one project needs a different activity label

### Project memory behavior

Use `.agents/codifix.toml` for project-owned behavior that should be easy to adapt without digging through service config:

- include and ignore path hints for repository scans
- enabled analyzers
- curation replacement policy for memory updates
- future graph and plugin controls

Example:

```toml
[curation]
replacement_policy = "balanced"
```

Available policies are `conservative`, `balanced`, and `aggressive`. `balanced` is the default.

### Env files

Use these for secrets such as:

```bash
CODIFIX__SERVICE__API_TOKEN=auto-generated-or-manually-overridden
OPENAI_API_KEY=your-api-key-here
VOYAGE_API_KEY=your-voyage-key-here
```

When you declare multiple `[[embeddings.backends]]` blocks, each one's `api_key_env` field names the variable the service will look up here. The name is arbitrary — whatever you put in `api_key_env` in the TOML, put the same key in `codifix.env`.

For local Ollama, use the first-class `ollama` provider and leave `api_key_env`
empty unless you are running behind a proxy that requires auth:

```toml
[llm]
provider = "ollama"
base_url = "http://127.0.0.1:11434/v1"
api_key_env = ""
model = "llama3.2"

[[embeddings.backends]]
name = "ollama-nomic"
provider = "ollama"
base_url = "http://127.0.0.1:11434/v1"
api_key_env = ""
model = "nomic-embed-text"
```

Run `ollama serve` and pull the models first, for example `ollama pull
llama3.2` and `ollama pull nomic-embed-text`. `codifix doctor` checks the
local `/v1/models` endpoint and warns when the configured LLM model is missing.

### LLM audit/debug mode

LLM audit mode records what the service sends to configured LLMs for query
answers, resume summaries, and get-up-to-speed summaries. It is disabled by
default because prompts can contain memory content:

```toml
[llm_audit]
enabled = true
redact = true
max_message_chars = 8000
max_total_chars = 32000
```

With `redact = true`, Codifix redacts common API key, bearer token,
password, secret, and database URL credential patterns before persisting the
audit activity.

## Writer ID

Codifix derives durable authorship from the authenticated principal.
The writer ID is an advisory runtime label for activity and diagnostics; it
does not grant access and cannot replace principal identity.

If you do nothing, Codifix derives the label automatically from:

- the writing tool
- the local user
- the local host name

That gives stable defaults such as:

- `memory-olivier-monolith`
- `memory-watcher-olivier-monolith`

For most setups, that automatic label is enough.

You can configure it in TOML:

```toml
[writer]
id = "codex-cli-main"
name = "Codex CLI"
```

or with an environment variable:

```bash
export CODIFIX_WRITER_ID=codex-cli-main
```

Use an explicit writer ID only when you want a custom stable label shared
across tools or machines. It changes display/runtime attribution, not the
authenticated author.

## Primary And Relay Services

If a machine can reach PostgreSQL, `codifix-service` runs as a `primary`.

If a machine cannot reach PostgreSQL but can see another Codifix service on the local network, `codifix-service` can run as a `relay`. In relay mode it discovers a primary over UDP multicast and forwards the normal HTTP API and browser WebSocket traffic to it.

Relay discovery is controlled from shared config:

```toml
[cluster]
enabled = true
```

The wizard exposes this as a shared setup option, and `codifix service enable` can offer to turn it on after a database-connect failure.

## Daily Use

Use [Daily workflow](https://codifix.dev/docs/daily-workflow) for capture,
retrieval, resume, jobs and sync. Start by verifying the destination:

```bash
codifix urls
codifix status --project my-project --json
codifix tui --project my-project
```

A project slug scopes knowledge inside an installation; it does not select an
area. See [Areas and projects](https://codifix.dev/docs/areas). For focused
commands, use [Remember](cli/remember.md), [Query](cli/query.md),
[Resume](cli/resume.md), [Jobs](cli/jobs.md), or [Bundles](cli/bundles.md).

## Shell Completion

Package installs generate completion scripts for bash, zsh, and fish automatically. Open a new shell after installing or upgrading.

For development binaries or manual setup:

```bash
codifix completion bash
codifix completion zsh
codifix completion fish
```

See [Shell Completion](cli/completion.md) for install paths and shell-specific setup.

## Optional Background Watcher

The manager is the preferred way to attach to configured coding-agent sessions:

```bash
codifix watcher manager enable --dry-run
codifix watcher manager enable
codifix watcher manager status
codifix watcher status --project my-project
```

For area deployments, the manager pins each child's exact area, instance and
configuration. Legacy per-project `watcher enable` is rejected there. Follow
[Watchers](https://codifix.dev/docs/watchers) and
[Area routing](https://codifix.dev/docs/how-it-works/area-routing) for manual
processes, ownership and recovery.

## Upgrading An Existing Install

Follow the [Update guide](https://codifix.dev/docs/install/update) and the
selected release's procedure. The Memory Layer-to-Codifix rename requires an
explicit cutover with a matched backup; it is not an in-place package upgrade.
Named areas have independently pinned executables, so neither package
installation nor `service restart-all` upgrades those instances.

After the selected upgrade and service procedure, verify health and review
repo-local skill changes:

```bash
codifix doctor
codifix health
codifix status --project my-project
codifix upgrade --dry-run
```

Refresh skills only after reviewing that preview. Rebuilding or pruning
embedding spaces is separate maintenance, not a required step for every
upgrade; see [Embedding operations](cli/embeddings.md). Application migrations
are append-only; never edit a migration ledger to force compatibility.

## Using `scan`

`scan` reads a repository, sends a structured summary to the configured LLM, and writes useful durable memories back into Codifix.

Full command documentation:

- [Memory Bundles](cli/bundles.md)
- [Scan Command](cli/scan.md)

Try it safely first:

```bash
codifix scan --project my-project --dry-run
```

Then write the results:

```bash
codifix scan --project my-project
```

If `scan` fails, the two most common causes are:

- missing `[llm].model` in config
- missing `OPENAI_API_KEY`

## Web UI Notes

The web UI is served by `codifix-service` itself. In a normal install it should work automatically once the service is running.

It mirrors the main TUI workflows in a browser: memories, agents, query, activity, errors, project health, review, watchers, embeddings, and resume. It also keeps web-only bundle import/export tooling under More.

See the [Web UI Guide](web-ui.md) for tab-by-tab behavior, query diagnostics, runtime status, get-up-to-speed briefings, LLM audit/debug logging, and contextual help.

If you build from source, build the frontend first:

```bash
npm --prefix web ci
npm --prefix web run build
```

Then start the backend:

```bash
cargo run --bin codifix -- service run
```

## Importing Commit History

Codifix can also store git commits as project evidence without turning every commit into canonical memory.

Import recent or full history:

```bash
codifix commits sync --project my-project
```

Browse imported commits:

```bash
codifix commits list --project my-project
codifix commits show --project my-project <commit-hash>
```

If `codifix doctor` reports that no commit history has been imported yet, the fix is:

```bash
codifix commits sync --project my-project
```

## Running From Source

If you are developing Codifix itself, a `cargo` checkout runs as a **dev** stack that is fully isolated from any packaged install on the same machine: a separate HTTP port (`4250` instead of `4040`) and runtime directory. The TUI shows `[dev]` in its header.

```bash
cargo run --bin codifix -- init
cargo run --bin codifix -- dev --copy-from-global
cargo run --bin codifix -- service run            # in its own shell
cargo run --bin codifix -- tui                    # header reads [dev]
```

Optional watcher manager:

```bash
cargo run --bin codifix -- watcher manager run
```

`codifix dev` without `--copy-from-global` leaves the user-local dev overlay without shared credentials — fine if you want the dev stack on a different database or LLM endpoint, otherwise rerun with the flag or copy `[database]`, `[llm]`, `[embeddings]` into the project `config.dev.toml` by hand.

The full isolation contract, default endpoint table, and troubleshooting live in [Dev Stack vs Installed Stack](../developer/dev-stack.md).

## Related Docs

- [User Documentation](README.md)
- [Memory Bundles](cli/bundles.md)
- [Scan Command](cli/scan.md)
- [Commit History](cli/commits.md)
- [Developer Documentation](../developer/README.md)
