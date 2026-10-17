# `codifix init`

`codifix init` bootstraps or refreshes Codifix for a repository.

Use it when you want the low-level bootstrap step without the interactive wizard.

## Common Usage

```bash
codifix init
codifix init --project memory
codifix init --force
codifix init --dry-run
codifix upgrade --dry-run
```

## What It Does

- creates or repairs the user-local project config/state directories
- creates or repairs the tiny repo-local `.codifix/project.toml` marker
- creates or repairs `.agents/codifix.toml`
- installs or refreshes the bundled repo-local Codifix skills
- can preview the planned file writes with `--dry-run`

The installed skill bundle includes:

- `codifix`
- `codifix-project-init`
- `codifix-github-init`
- `codifix-query-resume`
- `codifix-review-proposals`
- `codifix-plan-execution`
- `codifix-direct-task-start`
- `codifix-remember`

These are copied from the installed `skill-template` into `.agents/skills/`. The umbrella skill and shared Go helper live under `.agents/skills/codifix/`; focused workflow skills live beside it.

## Notes

- prefer [`codifix wizard`](wizard.md) for the normal guided setup flow
- use `--force` only when you intentionally want to replace managed bootstrap files
- existing `.codifix/config.toml` and `.codifix/codifix.env` files are treated as legacy inputs and can be migrated with `codifix doctor --fix`
- after package upgrades, use [`codifix upgrade`](upgrade.md) to refresh existing repo-local skill copies from the installed template
