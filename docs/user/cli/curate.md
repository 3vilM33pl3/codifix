# `codifix curate`

`codifix curate` turns raw captures into canonical memory entries.

Curation commits its run and automatic-replacement activity with the memory
changes. Success does not mean that asynchronous embedding, semantic dedup or
consolidation has finished. Version-pinned background job intents commit in the
same transaction. The service polls PostgreSQL and recovers unfinished leases
after restart. Inspect progress with `codifix jobs --project memory`; blocked or
failed stages require the documented operator retry workflow.

Use it when captures already exist and you want to process them into durable memory.

## Common Usage

```bash
codifix curate --project memory
codifix curate --project memory --batch-size 10
codifix curate --project memory --dry-run
```

## Notes

- `codifix remember` usually runs capture and curation together for completed work
- `--dry-run` previews the curation pass without writing memory entries

## Related Docs

- [Remember Command](remember.md)
- [Capture Command](capture.md)
