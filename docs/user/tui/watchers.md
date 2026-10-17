# Watchers Tab

**Watchers** shows live watcher processes for the active project.

![Watcher health and ownership details in the TUI](https://codifix.dev/images/tui/watchers-tab.png)

It reports watcher mode, repository root, owning agent session, heartbeat, restart attempts, and health such as `healthy`, `stale`, `restarting`, or `failed`. This is the liveness view; the Project tab is the broader summary.

Watchers carry an explicit **Desktop session** or **CLI session** monitoring
label; older watchers without that metadata show **Session type unknown**.

## Controls

- `j` / `k` choose a watcher.
- `PgUp` / `PgDn` and `Home` navigate details.
- `r` forces a fresh project snapshot.
- `h` opens help.

For normal Codex-linked work, prefer the watcher manager. Legacy service-managed and foreground watchers remain available for advanced cases.

See also: [Watcher guide](https://codifix.dev/docs/watchers) and [watcher commands](https://codifix.dev/docs/reference/cli/integrations-evals).
