---
name: codifix-project-init
version: 3.0.0
description: Initialise or refresh Codifix in a project directory after Codifix is already installed; use for repo-local setup with .mem, .agents/codifix.toml, and repo-local memory skills, previewing writes before applying them
---

# Codifix Project Init Skill

Use this skill when:
- the user asks to initialise, configure, bootstrap, or set up Codifix for a project
- Codifix is already installed on the machine
- the target work is repo-local `.codifix/` and `.agents/` setup

Do not use this skill for:
- installing Codifix packages, PostgreSQL, pgvector, or system services
- creating or reinitialising git history
- broad Codifix behavior questions
- completed-work remembering after setup is done

## Workflow

1. Identify the target project directory; if the user did not specify one, use the current working directory.
2. Run `codifix health` and `codifix doctor` to check the shared backend and current repo state.
3. Inspect existing `.codifix/`, `.agents/codifix.toml`, and `.agents/skills/` if present.
4. Preserve existing local customisations; do not delete or overwrite them without an explicit user request.
5. Run `codifix wizard --dry-run` in the target project and review the planned repo-local writes.
6. If the preview is safe, run `codifix wizard` in the target project.
7. Run `codifix doctor` again.
8. Report the project slug, files created or preserved, backend health, and next commands.

## Optional Evidence Setup

Only run these if the user explicitly wants initial evidence imported:

```bash
codifix commits sync --project <project-slug> --dry-run
codifix scan --project <project-slug> --dry-run
```

Ask before running the non-dry-run versions. Use `commits sync` for commit history evidence and `scan` for LLM-assisted initial memory candidates.

## Safety Rules

- Preview write-capable setup commands first with `--dry-run`.
- Do not run package managers or service installers from this skill.
- Do not create, delete, or reinitialise git repositories.
- If `.codifix/config.toml`, `.codifix/project.toml`, `.agents/codifix.toml`, or any skill directory exists, treat it as user/project state.

## Runtime Requirement

The repo-local Codifix skill bundle uses the shared Go helper under `.agents/skills/codifix/scripts/`.
`go` must be available on `PATH` for those helper commands to run after setup.
