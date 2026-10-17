# Codifix Skill Bundle

This page explains the current repo-local Codifix skill bundle. The canonical live umbrella skill is:

- `.agents/skills/codifix/SKILL.md`

The focused skills that sit beside it are:

- `.agents/skills/codifix-project-init/SKILL.md`
- `.agents/skills/codifix-github-init/SKILL.md`
- `.agents/skills/codifix-query-resume/SKILL.md`
- `.agents/skills/codifix-review-proposals/SKILL.md`
- `.agents/skills/codifix-plan-execution/SKILL.md`
- `.agents/skills/codifix-direct-task-start/SKILL.md`
- `.agents/skills/codifix-remember/SKILL.md`
- `.agents/skills/codifix-federation-expertise/SKILL.md`

Use this page to understand how the bundle is intended to behave without reading every raw skill file line by line.

## Table of Contents

- [Canonical Source](#canonical-source)
- [What The Skill Is For](#what-the-skill-is-for)
- [Current Workflow](#current-workflow)
- [Helper Scripts](#helper-scripts)
- [What Changed From The Older Flow](#what-changed-from-the-older-flow)
- [Related Docs](#related-docs)

## Canonical Source

The repo-local Codifix skill bundle in `.agents/skills/` is the source of truth.

That matters because the skill evolves with the product. The packaged `skill-template` and the example skill are derived from this workflow, but the live repo-local bundle is what the agent actually uses in this repository.

## What The Skill Is For

The `codifix` umbrella skill is the main driver for coding agent interaction with Codifix.

The bundle as a whole tells the agent when to:

- query project memory before answering repo-specific questions
- use `resume` to get back into flow after an interruption
- initialise or refresh repo-local Codifix setup for a project
- guide first-time GitHub repository onboarding with discovery, questions, dry-run review, and verification
- save the approved plan and checkpoint when a planning phase turns into approved execution
- save a task memory when direct implementation starts without an approved plan
- remember meaningful completed work automatically
- remember distilled code and codebase explanations after answering explanation requests
- review pending replacement proposals with evidence gathering before approve/reject

The umbrella skill handles broad or mixed memory turns. The focused skills handle the narrow workflow phases. The shared Go helper under `.agents/skills/codifix/scripts/` remains the shared execution path that actually calls `memory`.

## Current Workflow

The current live bundle workflow is:

1. `codifix-project-init` handles repo-local setup and refreshes for target projects.
2. `codifix-github-init` handles first-time GitHub repository onboarding and GitHub setup audits.
3. `codifix-query-resume` handles query-first answers and interruption recovery.
4. `codifix-review-proposals` handles pending replacement proposal review.
5. `codifix-plan-execution` handles execution start and plan-completion verification.
6. `codifix-direct-task-start` records direct no-plan implementation starts.
7. `codifix-remember` handles post-task remembering once work is actually complete.
8. Code explanation turns query memory first, inspect relevant code or docs, answer, then remember the distilled reusable explanation when it is durable.
9. The umbrella `codifix` skill keeps the shared invariants and covers mixed or ambiguous memory turns.
10. Prefer insufficient evidence over unsupported conclusions.
11. Never invent provenance.

The important change is that the bundle is no longer centered on one broad skill or on a manual `codifix capture -> curate-memory` sequence for normal work. The preferred path is now a focused workflow plus `remember-task`, which captures and curates in one step.

## Shared Go Helper

The repo-local skill bundle now uses one shared Go helper:

- `go run ./.agents/skills/codifix/scripts/main.go <command> ...`

The supported helper commands are:

- `query-memory`
  - query existing curated project memory before answering
- `resume-project`
  - build a resume briefing from the saved checkpoint and project timeline
- `review-proposals`
  - list, inspect, approve, or reject pending replacement proposals after review
- `checkpoint-project`
  - save a checkpoint explicitly when you want to mark a point in time without storing a plan
- `start-plan-execution`
  - save the checkpoint and store the full approved execution plan as `plan` memory before implementation starts
- `finish-plan-execution`
  - verify that every checkbox item in the active approved plan is complete before the agent can claim the task is finished, and record the verified outcome as `implementation` or `refactor` memory
- `remember-task`
  - capture completed work and curate it immediately into durable memory, with `implementation` as the normal completed-work outcome type and `refactor` for behavior-preserving code restructuring
- `capture-task`
- `curate-memory`

`go` must be available on `PATH` anywhere the repo-local skill bundle is expected to run. That requirement is specific to the skill helper runtime; the `memory` CLI itself is still the underlying command surface.

Command resolution is explicit and stops after one selected destination:

1. `CODIFIX_BIN`, when supplied, overrides the command for that invocation.
2. `.agents/codifix-command.json`, when present, contains the project's
   command as a JSON array of arguments. Invalid configuration fails closed.
3. Without either selection, a Memory source checkout uses `cargo run`; other
   repositories use the installed `memory` command.

This repository selects `["memory", "--area", "workshop"]` for durable notes,
including queries, task starts, plans and completed work. That selection uses
the existing Workshop API and operator-owned credentials/routing. The helper
does not read database credentials or automatically switch services on failure.
Other repositories do not inherit this project's command file when installing
the skill bundle.

For a backend test, explicitly select the source CLI with
`CODIFIX_BIN='cargo run --quiet --bin memory --'`. Source commands without an
explicit `--area` run with the dev profile and omit inherited area-routing,
config-path and client-token environment overrides. Explicit `--config` command
arguments remain available for intentional dev test configurations.
Dev transport, authentication and degraded-database
failures include recovery instructions in both JSON and text output, retaining
the original error and exit code. See [dev recovery](../dev-stack.md#project-memory-routing-and-recovery).

## What Changed From The Older Flow

Older documentation in this repo focused on:

- query memory
- build a capture payload manually
- run `codifix capture`
- run `curate-memory`

The current skill bundle has moved beyond that. It now also covers:

- interruption recovery with `resume`
- repo-local project setup with preview-first `codifix wizard`
- plan-to-execution checkpointing with approved-plan capture
- direct no-plan task-start recording
- strict plan-completion verification before the agent may conclude plan-backed work
- automatic post-task remembering with `remember-task`
- explanation memory capture for durable code, module, architecture, and codebase explanations

If you are updating other documentation or examples, align them to the live skill instead of treating the older manual capture/curate flow as the default.

## Related Docs

- [How Skills Work](how-skills-work.md)
- [Skills And Agent Material](README.md)
- [Architecture Overview](../architecture/overview.md)
- [How Codifix Works](../architecture/how-it-works.md)
- [Agent Example Skill](../examples/agent-example/skills/codifix/SKILL.md)
