# Watchers & Companions Tab

**Watchers & Companions** shows watcher processes and companion observations for the active project.

![Watcher health and ownership details in the TUI](https://codifix.dev/images/tui/watchers-tab.png)

It reports watcher mode, repository root, owning agent session, heartbeat, restart attempts, and health such as `healthy`, `stale`, `restarting`, or `failed`. This is the liveness view; the Project tab is the broader summary.

Watchers carry an explicit **Desktop session** or **CLI session** monitoring
label; older watchers without that metadata show **Session type unknown**.

## Controls

- `j` / `k` scroll watcher details.
- `PgUp` / `PgDn` and `Home` navigate details.
- `r` refreshes project, companion and exchange observations.
- `c` switches focus between watchers and companions.
- `h` opens help.

For normal Codex-linked work, prefer the watcher manager. Legacy service-managed and foreground watchers remain available for advanced cases.

See also: [Watcher guide](https://codifix.dev/docs/watchers) and [watcher commands](https://codifix.dev/docs/reference/cli/integrations-evals).

## Companion observations

The companion panel shows whether the companion is enabled, its reported
freshness, heartbeat and version, and whether your project credential can inspect
exchanges. Disabled, unknown, fresh, stale and failed reads are distinct. A
heartbeat alone does not prove peer reachability or task success.

Press `c` to focus the companion panel. Select a peer with `j/k`, then press
`Enter` to open that peer's Agent exchanges in
[Conversations](https://codifix.dev/docs/tui/conversations). Peer labels describe
enrollment or observed exchanges; they are not a live connectivity check.
The companion panel uses the full content area when focused, so peer selection
remains accessible in smaller terminals. Press `c` again to return to the watcher
view and its existing scroll position.
