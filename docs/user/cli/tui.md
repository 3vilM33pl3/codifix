# `codifix tui`

`codifix tui` opens the terminal UI for browsing, querying, operating, and diagnosing Codifix.

Use it for human inspection. Agents and scripts should prefer CLI commands with `--json` when they need parseable output.

## Common Usage

```bash
codifix tui
codifix tui --project memory
```

`--project <slug>` opens a specific project first. Without it, Codifix tries to infer the current repository project.

## What You Can Do

- browse and filter memories
- ask cited questions and inspect returned memories
- review activities, errors, watchers, agents, and embeddings
- switch embedding backends
- inspect project health and get resume briefings

## Related Docs

The focused federation workspace is `codifix federation --project <slug> tui`.
It supplies membership, exact publication review, proposals and synchronization
controls through the same local API. See [Federation](federation.md).

- [TUI Guide](../tui/README.md)
- [Memories Tab](../tui/memories.md)
- [Query Tab](../tui/query.md)
- [Errors Tab](../tui/errors.md)
