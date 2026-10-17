# Automations Tab

**Automations** is the read-oriented terminal view of the loop automation control plane.

![Automation loop state in the TUI](https://codifix.dev/images/tui/automations-tab.png)

It shows registered loops, effective mode and scope, latest run state, risk level, pending approvals, load warnings, and the global kill-switch state when available. Use the Web UI or `codifix loops` when you need to change settings, run a loop, or make an approval decision.

## Controls

- `j` / `k` select an automation.
- `PgUp` / `PgDn` and `Home` navigate the detail.
- `r` refreshes the project snapshot.
- `h` opens help.

See also: [Automations](https://codifix.dev/docs/automations) and [loop commands](https://codifix.dev/docs/reference/cli/integrations-evals).
