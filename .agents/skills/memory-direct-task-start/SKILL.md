---
name: memory-direct-task-start
version: 2.1.0-rc.2
description: Record a task memory before starting direct no-plan implementation work when the user asks to implement, fix, change, add, update, release, install, or otherwise mutate the repository without first approving a plan
---

# Memory Direct Task Start Skill

Use this skill when:
- the user gives an actionable implementation instruction
- there is no approved checklist plan for the work
- repository files, packaging, release metadata, services, or local install state may change

Do not use this skill for:
- pure questions or explanations
- planning-only turns
- work already covered by `memory-plan-execution`
- trivial read-only checks

## Memory Layer Outage Exception

When the explicit task is to repair Memory Layer itself and its unavailable or
degraded service prevents `start-task-execution` from being recorded, do not
let that failed checkpoint block the repair.

1. Confirm that the failure is caused by Memory Layer's own unavailable or
   degraded backend, using read-only evidence such as `memory doctor`, service
   status, or logs.
2. Make only the minimum repair required to restore Memory Layer. Do not use
   this exception for unrelated repository or infrastructure work.
3. Verify that the service is healthy and the database-backed API is available.
4. Record the original task start and implementation outcome after recovery.

This exception is deliberately limited to breaking the circular dependency of
using Memory Layer to repair Memory Layer.

## Script

Before any implementation work or file edits, record the task start:

```bash
go run ./.agents/skills/memory-layer/scripts/main.go start-task-execution \
  --project <project-slug> \
  --title "<short task title>" \
  --prompt "<original user request>"
```

## Workflow

1. Run the script before mutating files or services.
2. Use the original user request as `--prompt`; do not summarize away the ask.
3. Keep `--title` short and action-oriented.
4. If the helper fails, stop and surface the failure before implementation,
   except for the Memory Layer Outage Exception above.
5. After the work is complete, use `memory-remember` to record the implementation outcome.

## Runtime Requirement

This focused skill uses the shared Go helper under `.agents/skills/memory-layer/scripts/`.
`go` must be available on `PATH` for these helper commands to run.
