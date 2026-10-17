# `codifix doctor`

Use `doctor` when Codifix is installed but something is not working the way you expect.

`codifix status` is the recommended first diagnostic command because it aggregates the service, watcher, MCP, and doctor views. Use `doctor` when you want the deeper setup checklist or when `status` points at a fix.

## Table of Contents

- [What It Checks](#what-it-checks)
- [Typical Usage](#typical-usage)
- [Common Failures](#common-failures)

## What It Checks

`codifix doctor` checks the current setup for common problems such as:

- missing or placeholder database URL
- missing or placeholder service API token
- unexpected auto-derived or overridden advisory writer label
- backend connectivity issues
- Ollama reachability and missing local LLM models when `[llm].provider = "ollama"`
- missing `pgvector`
- project bootstrap problems such as missing user-local project config, missing `.codifix/project.toml`, or legacy `.codifix` config that has not been migrated
- repo-local Memory skill-bundle version and missing/outdated skill files
- repo-local Memory skill freshness compared with the GitHub skill bundle
- repo-local service or watcher configuration issues

The exact output is meant to be actionable, not just descriptive.

## Typical Usage

Run it any time setup looks suspicious:

```bash
codifix doctor
```

It is especially useful after:

- first install
- upgrades
- changing database config
- enabling embeddings
- bootstrapping a new repo

## Common Failures

Typical remediation paths are:

- unexpected writer label
  - set `[writer].id` or `CODIFIX_WRITER_ID` if you want a custom stable
    activity label instead of the auto-derived default; authentication still
    determines durable authorship
- missing `pgvector`
  - install pgvector on the PostgreSQL server, then enable `vector` in the specific Codifix database with `CREATE EXTENSION IF NOT EXISTS vector;`
  - verify the same database URL Codifix uses with `psql "$DATABASE_URL" -c "SELECT extversion FROM pg_extension WHERE extname = 'vector';"`
  - see [Getting Started: PostgreSQL Requirement](../getting-started.md#postgresql-requirement) for local Debian, local macOS, and hosted PostgreSQL examples
- repo not initialized
  - run `codifix wizard` or `codifix init`
- outdated or unversioned repo-local skills
  - run `codifix upgrade --dry-run`, then `codifix upgrade`
  - or run `codifix doctor --fix` to download the current Memory skill bundle from GitHub and repair repo-local skills
  - or open the TUI/Web Skills tab to inspect the affected skill files and run the same repair path interactively
- backend unreachable
  - start the shared backend service or the local development backend
- Ollama unreachable or model missing
  - start Ollama with `ollama serve` and pull the configured model with `ollama pull <model>`
- placeholder service API token
  - run `codifix service ensure-api-token --rotate-placeholder` or `codifix wizard --global`

Use `doctor` first before assuming the memory database or watcher system is broken.

## Skill Repairs

`codifix doctor` includes two skill checks:

- `workflow.project_skills` compares `.agents/skills/` with the best local template available to the installed binary.
- `workflow.project_skills_github` compares `.agents/skills/` with the current GitHub skill bundle.

`codifix doctor --fix` downloads the current skill bundle archive from GitHub, caches it under the user-local Codifix state directory, and uses it to install missing skills or replace outdated, unversioned, or invalid-version Memory-owned skills. Replaced skills are backed up under the user-local project runtime directory.

If GitHub is unavailable, the repair path falls back to the installed local `skill-template` so existing package-based repairs still work offline.
