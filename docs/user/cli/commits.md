# `codifix commits`

`codifix commits` manages stored git history for a project.

It does **not** turn every commit into canonical memory.
Instead, it stores commits as project-scoped evidence so Codifix can keep:

- full commit hashes
- commit messages
- authors
- timestamps
- changed paths

while still keeping `memory_entries` selective and curated.

## Table of Contents

- [Commands](#commands)
- [What Gets Stored](#what-gets-stored)
- [Why This Exists](#why-this-exists)

## Commands

Sync commit history into Codifix:

```bash
codifix commits sync --project my-project
```

Limit the import:

```bash
codifix commits sync --project my-project --limit 100
codifix commits sync --project my-project --since "2 weeks ago"
```

Preview the import counts without writing stored commit evidence:

```bash
codifix commits sync --project my-project --since "2 weeks ago" --dry-run
```

List stored commits:

```bash
codifix commits list --project my-project
```

Show one stored commit:

```bash
codifix commits show --project my-project <commit-hash>
```

## What Gets Stored

For each imported commit, Codifix stores:

- full hash
- short hash
- subject
- body
- author name and email when available
- commit timestamp
- parent hashes
- changed paths
- import timestamp

## Why This Exists

This feature is meant to preserve git history as evidence without degrading normal memory search.

Default query behavior remains memory-first:

- normal `codifix query` returns curated memories
- commit history is separate evidence you can inspect directly
- curated memories can still cite commits as provenance

## Related Docs

- [Getting Started](../getting-started.md)
- [Scan Command](scan.md)
- [How Codifix Works](../../developer/architecture/how-it-works.md)
