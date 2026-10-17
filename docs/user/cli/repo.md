# `codifix repo`

`codifix repo` manages the local repository index used by `codifix scan` and related analysis flows.

## Subcommands

### Index

```bash
codifix repo index --project memory
codifix repo index --project memory --since 2026-04-01
codifix repo index --project memory --dry-run
```

Builds or refreshes the local index.

### Status

```bash
codifix repo status --project memory
codifix repo status --project memory --json
```

Shows analyzer coverage, fact counts, and local index status.

## Related Docs

- [Code Graph Extraction](graph.md)
- [Scan Command](scan.md)
