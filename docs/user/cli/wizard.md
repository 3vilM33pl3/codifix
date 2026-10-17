# `codifix wizard`

Use `wizard` to bootstrap Codifix configuration either globally for the machine or locally for the current repository.

## Table of Contents

- [Global vs Repo-Local](#global-vs-repo-local)
- [What It Creates](#what-it-creates)
- [Typical Usage](#typical-usage)
- [How It Differs From `init`](#how-it-differs-from-init)

## Global vs Repo-Local

`codifix wizard --global` configures shared machine-level settings such as:

- `database.url`
- optional shared `writer.id`
- optional LLM settings, including local Ollama via `provider = "ollama"`

`database.url` must point at a reachable PostgreSQL database before the backend can become healthy. The target database should already have pgvector enabled:

```bash
psql "$DATABASE_URL" -c "SELECT 1;"
psql "$DATABASE_URL" -c "CREATE EXTENSION IF NOT EXISTS vector;"
psql "$DATABASE_URL" -c "SELECT extversion FROM pg_extension WHERE extname = 'vector';"
```

For local and hosted database setup examples, see [Getting Started: PostgreSQL Requirement](../getting-started.md#postgresql-requirement).

The shared service API token is usually generated automatically into the adjacent `codifix.env` file. The wizard only needs an explicit token value if you want to override that generated token.
If you leave `writer.id` unset, Codifix derives a stable advisory writer
label automatically at runtime. In v2, the authenticated principal supplies
durable authorship and permissions.

`codifix wizard` inside a repository is project-first and bootstraps user-local project config plus repo-local agent files such as:

- user-local project `config.toml`
- user-local project `codifix.env` when needed
- `.codifix/project.toml`
- `.agents/codifix.toml`
- `.agents/skills/`

Inside a repository, the wizard defaults to repo-local scope unless you explicitly choose shared/global setup.

## What It Creates

At repo scope, the wizard creates:

- user-local project config/state/cache directories
- `.codifix/project.toml` as the repo-local project marker
- `.agents/codifix.toml`
- a repo-local copy of the Codifix skill bundle

Older `.codifix/config.toml`, `.codifix/codifix.env`, and `.codifix/runtime/` layouts remain readable as legacy fallback. Use `codifix doctor --fix` to copy missing legacy config/env files into the current user-local project layout.

The bundled skills are created from the packaged `skill-template`, or from the repo-local template during source/dev usage.
That repo-local bundle now uses a shared Go helper under `.agents/skills/codifix/scripts/`, so `go` must be available on `PATH` anywhere you expect the repo-local skills to run.
The bundle includes these Memory-owned skills:

- `codifix`
- `codifix-project-init`
- `codifix-github-init`
- `codifix-query-resume`
- `codifix-review-proposals`
- `codifix-plan-execution`
- `codifix-direct-task-start`
- `codifix-remember`

The installed template lives under `/usr/share/codifix/skill-template/` for Debian packages, `~/.local/share/codifix/skill-template/` for the local Linux install script, `/usr/local/share/codifix/skill-template/` for the macOS `.pkg`, `$(brew --prefix)/share/codifix/skill-template/` for Homebrew, and `.agents/skills/` during source/dev usage.

## Typical Usage

First machine setup:

```bash
codifix wizard --global
```

Then inside each repo:

```bash
cd /path/to/your-project
codifix wizard
```

Preview the resulting file, token, and service actions without writing anything:

```bash
codifix wizard --dry-run
codifix wizard --global --dry-run
```

## How It Differs From `init`

`init` is the lower-level repo bootstrap command.

For normal interactive setup, prefer `wizard`.

Use `init` when you want:

- a more scriptable bootstrap path
- a non-interactive setup flow
- direct control over the generated repo-local files

Both `codifix wizard` and `codifix init` support `--dry-run` for preview-only setup.

For the full onboarding flow, see [Getting Started](../getting-started.md).
