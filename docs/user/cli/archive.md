# `codifix archive`

`codifix archive` archives low-confidence, low-importance memories in a project.

## Common Usage

```bash
codifix archive --project memory
codifix archive --project memory --max-confidence 0.2 --max-importance 1
codifix archive --project memory --dry-run
```

## Notes

- use `--dry-run` first to preview which memories would be archived
- the thresholds let you tune how aggressively low-signal memories are removed from the active set
