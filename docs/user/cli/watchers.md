# Watcher Health

This page explains how watcher liveness and recovery show up in Memory Layer.

## Table of Contents

- [What The Watcher Does](#what-the-watcher-does)
- [Watcher Health States](#watcher-health-states)
- [Where To See Watcher Health](#where-to-see-watcher-health)
- [Recovery Signals](#recovery-signals)
- [Common Commands](#common-commands)

## What The Watcher Does

`memory watcher` is the background process that watches a project and sends useful work context to `memory service`.

On Linux, macOS, and Windows, the recommended path is the Codex-linked watcher manager:

```bash
memory watcher manager enable
```

That installs a persistent user manager service:

- on Linux it uses `systemd --user`
- on macOS it uses a `launchd` LaunchAgent
- on Windows it uses current-user, least-privilege Task Scheduler tasks

The manager detects live Codex and Claude sessions, bootstraps git repos if needed, and starts one watcher per repository. When several agent sessions use the same repository, the manager selects one stable live session as the watcher owner instead of duplicating filesystem work.

The manager uses a lightweight process scan every 10 seconds. It reads only process IDs and command lines until it identifies live Codex or Claude processes; it does not subscribe to agent transcript directories or collect TUI metrics such as per-process CPU and memory. Every sixth scan also verifies the managed service units. A singleton lock prevents accidentally running two managers at the same time.

A manager started from a Cargo build is scoped to the repository whose dev config it loaded. It does not try to launch dev watchers for every other active agent repository on the machine; the installed production manager remains responsible for those repositories.

Project watchers are also event-driven by default. They watch the repository root and non-ignored top-level directories, debounce file changes, and then run one `git status --porcelain` pass for the burst. Configured ignored trees such as `.git/` and `target/` are not recursively subscribed, and event bursts are coalesced into a single pending signal. `poll_interval` is the fallback safety scan interval, not the primary loop. New configs use:

```toml
[automation]
file_events = true
poll_interval = "60s"
```

If filesystem watching is unavailable, the watcher logs a warning and falls back to polling.

Watcher state is replaced atomically. If an older or interrupted watcher left invalid JSON, the next watcher preserves it beside the state file with a `.corrupt-*` suffix and rebuilds current state from the repository instead of entering a permanent restart loop.

Legacy per-project service-managed watchers can still be installed with:

```bash
memory watcher enable --project my-project
```

Manual foreground watchers can still be run with:

```bash
memory watcher run --project my-project
```

Treat the legacy service-managed and foreground watcher commands as advanced flows. For normal Codex-linked operation, prefer the watcher manager and use `memory status --project my-project` for a combined service, watcher, MCP, and doctor snapshot.

## Watcher Health States

Memory Layer tracks watcher health with a heartbeat and watchdog.

The main states are:

- `healthy`: the watcher is heartbeating normally
- `stale`: the watcher stopped heartbeating in time
- `restarting`: the backend requested a restart for a service-managed watcher
- `failed`: the watcher did not recover or the restart request failed

Manual watchers can become `stale`, but they are not restarted automatically.

## Where To See Watcher Health

In the TUI:

- `Watchers` tab:
  - shows the current watcher list for the project
  - shows health state, restart attempts, last restart attempt, and last heartbeat
  - shows the owning agent session when the watcher is manager-started
- `Project` tab:
  - shows the compact watcher summary
  - shows recent watcher-health transitions such as stale, restarting, failed, and recovered in the recent activity section

## Recovery Signals

When a watcher recovers after being `stale`, `restarting`, or `failed`, the TUI now makes that explicit:

- the status line shows a recovery message immediately
- the `Project` tab recent activity section shows a watcher transition row for the recovery
- the watcher and activity details include:
  - the current health
  - the previous health
  - whether the watcher is service-managed
  - the restart attempt count
  - how many restart attempts happened before recovery, when relevant

That makes it easier to tell the difference between:

- a watcher that is still unhealthy
- a watcher that restarted and recovered
- a manual watcher that went stale and needs user action

## Common Commands

Enable the watcher manager:

```bash
memory watcher manager enable
```

Check watcher manager status:

```bash
memory watcher manager status
```

The status output includes the manager mode, last reconcile reason and duration, process scan count, and lock owner PID. If another manager is already running, `memory watcher manager run` exits with the active PID instead of starting a duplicate.

Enable a legacy service-managed watcher:

```bash
memory watcher enable --project my-project
```

Check legacy watcher service status:

```bash
memory watcher status --project my-project
```

Disable the service-managed watcher:

```bash
memory watcher disable --project my-project
```

Run a watcher manually:

```bash
memory watcher run --project my-project
```

## `memory watcher flush`

Flushes pending automation work into a capture (and optional curation) run
immediately, instead of waiting for the idle threshold. Formerly
`memory automation flush`; `memory automation status` is covered by
`memory watcher status`.

```bash
memory watcher flush --project memory --dry-run
memory watcher flush --project memory --curate
```
