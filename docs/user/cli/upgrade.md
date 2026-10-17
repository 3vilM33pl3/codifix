# `codifix upgrade`

`codifix upgrade` refreshes the repo-local Codifix skill bundle in the current project.

Use it after installing a newer Codifix package or when `codifix doctor` reports that project skills are missing, unversioned, or older than the installed template.

## Common Usage

```bash
codifix upgrade --dry-run
codifix upgrade --dry-run --json
codifix upgrade
codifix upgrade --force
```

## What It Does

- compares `.agents/skills/` against the best available Memory skill template
- uses the installed local `skill-template` for both dry-run and execution
- never downloads `main` as a side effect of `codifix upgrade`
- reads the canonical shared bundle `version` from each `SKILL.md`
- installs missing Memory-owned skills
- replaces outdated, unversioned, or invalid-version Memory-owned skills
- backs up replaced skill directories under the user-local project runtime directory at `runtime/skill-backups/<timestamp>/`

By default, it does not replace a project-local skill that is newer than the selected template. Use `--force` only when you intentionally want to replace all known Memory-owned skills from the template.

All bundled Memory skills should report the same skill-bundle version. That
version is independent from the Codifix binary and Codex plugin versions;
for example, a v2.0.0 binary can correctly ship a `0.9.x` skill bundle. The JSON
output includes a top-level bundle version/status and per-skill details for
troubleshooting.

The Memory-owned skill set is:

- `codifix`
- `codifix-project-init`
- `codifix-github-init`
- `codifix-query-resume`
- `codifix-review-proposals`
- `codifix-plan-execution`
- `codifix-direct-task-start`
- `codifix-remember`

Template sources:

- Exact operator-selected release: `CODIFIX_SKILL_TEMPLATE` set to an absolute directory. A missing explicit directory fails closed rather than using another bundle.

- Debian package: `/usr/share/codifix/skill-template/`
- local Linux install script: `~/.local/share/codifix/skill-template/` unless `XDG_DATA_HOME` overrides it
- macOS `.pkg`: `/usr/local/share/codifix/skill-template/`
- Homebrew: `$(brew --prefix)/share/codifix/skill-template/`
- source/dev checkout: `.agents/skills/`

## Doctor Integration

`codifix doctor` includes a `workflow.project_skills` check. If the check warns, run:

```bash
codifix upgrade --dry-run
codifix upgrade
```

`codifix doctor` also includes a `workflow.project_skills_github` comparison. A
newer upstream version is informational for pinned deployments: update the
authenticated release first, then run `codifix upgrade` against its exact bundle.
For the four-area cutover, archive existing Memory-owned skills and set
`CODIFIX_SKILL_TEMPLATE` to the verified rc.2 bundle before repair. Unrelated
skills remain untouched.
