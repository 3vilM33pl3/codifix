---
name: codifix-codex
description: Use Codifix MCP tools and the installed memory CLI to retrieve project context, resume work safely, and preserve completed work in Codex.
---

# Codifix for Codex

Use this skill for project-specific questions, interruption recovery, Memory
Layer setup, or durable work capture in a repository that uses Codifix.

## Prerequisites

This plugin requires the `memory` CLI on `PATH` and a reachable Codifix
service. It does not install the CLI, change service configuration, or manage
credentials. If a tool or CLI command reports a missing binary, unavailable
service, or authentication error, explain the prerequisite and stop before
changing system configuration.

## Read and resume workflow

1. For a project-specific question, call `codifix_query` before answering.
2. For an interrupted task, call `codifix_resume`; do not replace it with a
   generic query.
3. For a newly assigned or unfamiliar repository, call `codifix_up_to_speed`.
4. For work that might belong to another repository, call
   `codifix_search_all` first. Use the returned `project` and `repo_root` to
   select the repository before making changes.
5. Treat Codifix results as evidence. Prefer insufficient evidence over
   unsupported conclusions.

## Project setup

Use `codifix wizard --dry-run` before initialising a repository, then review
the proposed changes. `codifix init` or `codifix wizard` installs the canonical
repo-local `.agents/skills/` bundle. Those repo-local skills remain
authoritative; this desktop plugin does not replace or update them.

## Write workflow

For direct implementation without an approved plan, record the start before
editing:

```bash
codifix checkpoint start-task --title "<short title>" --prompt "<user request>"
```

For approved plan execution, save the checked plan and checkpoint before
editing:

```bash
codifix checkpoint start-execution --plan-file /tmp/approved-plan.md
```

Before claiming an approved plan is complete, run:

```bash
codifix checkpoint finish-execution
```

After meaningful completed work, capture the verified outcome with
`codifix remember`, including a concise summary, durable note, changed files,
and relevant validation results. Use `--type refactor` only for an intentional
behavior-preserving restructuring.

## Safety

Use the existing MCP tools only. Do not add, bypass, or infer authorization.
Codifix loop operations remain subject to their existing approval and
role checks.
