# `codifix status`

`codifix status` is the recommended first diagnostic command for humans and agents.

```bash
codifix status --project memory
codifix status --project memory --json
```

It aggregates:

- packaged service status
- HTTP health and stats API reachability
- watcher manager status
- project watcher service status
- MCP service/project status and exposed surface counts
- doctor checks, including config and skill-bundle checks
- runtime component status; the default skill row is filtered to the
  `codifix` umbrella skill so headers stay focused

## Output Contract

- Machine-readable JSON output goes to stdout with `--json`.
- Human-readable status output goes to stdout without `--json`.
- Warnings, progress, and diagnostics that are not part of parsed output should go to stderr.
- Current exit behavior is simple: commands exit non-zero on failure. Stable category-specific exit codes are not implemented yet.

Use the narrower commands when you already know which subsystem you need:

- `codifix doctor --project memory` for the full setup checklist and repair hints
- `codifix upgrade --dry-run` for the full repo-local focused-skill bundle view
- `codifix health` for only `/healthz`
- `codifix service status` for the packaged service manager
- `codifix watcher manager status` for the Codex-linked watcher manager
- `codifix mcp status --project memory` for MCP-only status

## Agent Guidance

For automation, use:

```bash
codifix status --project memory --json
```

Then inspect `summary.overall`, `summary.doctor_failures`, `health.ok`, `stats.ok`, and `mcp.service_reachable` before deciding whether to run a narrower command.
