# TUI Guide

The terminal UI is the fastest way to inspect what Codifix knows before you act on it. It combines cited query results, stored memory, project health, agent activity, watcher state, proposals, and diagnostics in one local surface.

![Codifix terminal UI overview](https://codifix.dev/images/tui-frontpage.png)

## Start the TUI

```bash
codifix tui
codifix tui --project <project-slug>
```

The header identifies the active project and, for routed installations, the area,
durable instance and executable release. Check it before writing. The footer
reports TUI, service, watcher, and skill health. A source checkout uses an
isolated dev profile; its header is marked `[dev]` so it is not confused with a
packaged installation. See [Areas and projects](https://codifix.dev/docs/areas).

## Shared navigation

| Key | Action |
| --- | --- |
| `Tab`, `Right`, or `l` | Next tab |
| `Shift+Tab` or `Left` | Previous tab |
| `h` | Open or close help for the active tab |
| `?` | Switch to Query and start a question |
| `/` | Edit the global text filter |
| `r` | Refresh current project state |
| `Ctrl+C` | Exit |

## Choose a tab

| Need | Tab |
| --- | --- |
| Read or filter known memory | [Memories](https://codifix.dev/docs/tui/memories) |
| Ask a cited project question | [Query](https://codifix.dev/docs/tui/query) |
| Review replacement proposals | [Review](https://codifix.dev/docs/tui/review) |
| See active coding sessions | [Agents](https://codifix.dev/docs/tui/agents) |
| Check watcher liveness | [Watchers](https://codifix.dev/docs/tui/watchers) |
| Inspect the managed skill bundle | [Skills](https://codifix.dev/docs/tui/skills) |
| See loop state | [Automations](https://codifix.dev/docs/tui/automations) |
| Generate a handoff briefing | [Activity](https://codifix.dev/docs/tui/activity) or [Resume](https://codifix.dev/docs/tui/resume) |
| Maintain semantic retrieval | [Embeddings](https://codifix.dev/docs/tui/embeddings) |
| Diagnose a failure | [Errors](https://codifix.dev/docs/tui/errors) |
| See the whole project at a glance | [Project](https://codifix.dev/docs/tui/project) |

The TUI is for human inspection. Agents should prefer structured CLI output such as `codifix query --json` when another tool will parse the result.

See also: [Daily workflow](https://codifix.dev/docs/daily-workflow) and the [Web UI](https://codifix.dev/docs/web-ui).

## Additional workspaces

The **Offline** tab shows read-only synchronized knowledge from explicitly
configured laptop subscriptions. `r` loads/refreshes inventory; `j/k` or arrows
select the exact subscription UUID; `/` enters a question and `Enter` performs
a bounded eight-hit local deterministic search. Questions are limited to 4096
UTF-8 bytes. `Esc` ends entry; `d/u` or PageDown/PageUp scroll provenance and
reviewed evidence. While editing, letters including `q` are question text, not
quit/navigation shortcuts; Ctrl+C still exits.

Area, owner instance, project, company, device and space UUIDs identify the copy.
Last observed sync/cursor is not guaranteed freshness or current remote access.
Shared text/locators are untrusted plain text, never executed or forwarded to a
model. There are no write, approval or synchronization controls. Selection,
refresh and query edits invalidate old excerpts; leaving the tab cancels reads.
The configured local loopback owner must authenticate. A disabled/unsupported
server or failed read is reported as unavailable, not an empty healthy cache.

The **Jobs** tab inspects durable background stages and provides confirmed retry
with stable request IDs. See [Background jobs](https://codifix.dev/docs/reference/cli/jobs) for keys and policy.

The **Supervisor** tab provides instance-wide metadata-only inspection, separate
from local coding sessions in Agents. See [Supervisor inspection](https://codifix.dev/docs/reference/cli/supervisor)
for permissions, cursor controls and explicit inactive-worker limitations.

For private federation, launch `codifix federation --project <slug> tui`. This
focused workspace supports membership, exact publication review, shared citations,
proposals, approval/rejection and synchronization through the same authenticated
local API as Web and CLI. See [Federation commands and keys](https://codifix.dev/docs/reference/cli/federation).
