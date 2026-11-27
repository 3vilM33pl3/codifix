# Watchers Help

## Purpose
Show project watchers, heartbeat state, agent ownership, service identity, restart attempts, and recovery behavior.

## Layout
- Scrollable watcher report.
- Each watcher shows health, mode, repo root, watcher id, owner agent/session/pid, host service, heartbeat, and restart attempts.
- Managed watchers belong to agent sessions; manual watchers were started directly.

## Controls
- `j/k` or `Up/Down`: scroll.
- `PgUp/PgDn`: page. `Home`: jump to top.
- `r`: refresh project state outside help.

## Workflows
- Use this tab when captures are not appearing or watcher health is degraded.
- Check owner/session and stale heartbeat before restarting anything.
- Use restart attempts to distinguish transient restarts from repeated failures.

## Troubleshooting
- If a managed watcher stays stale, check Manager footer and Errors.
- If only manual watchers exist, start through the manager-integrated path.

## Companions

This tab is **Watchers & Companions**. `c` switches focus between watcher
inspection and the companion panel, preserving watcher scroll controls.
Companion state distinguishes disabled, unknown, fresh and stale observations.
A heartbeat does not prove peer reachability or task success. In the companion
panel, `j/k` selects a peer seen on the exchange page and `Enter` opens that
peer in Conversations. `r` refreshes status and the observed exchange page.
