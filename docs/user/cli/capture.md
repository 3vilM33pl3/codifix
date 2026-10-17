# `codifix capture`

`codifix capture` sends structured task evidence to Codifix without relying on the higher-level `remember` wrapper.

Right now the public capture surface is `codifix capture`.

## Common Usage

```bash
codifix capture --file /tmp/task.json
codifix capture --file /tmp/task.json --dry-run
```

## When To Use It

- when another tool already produced a structured task payload
- when you want lower-level control than `codifix remember`
- when you need to validate a capture payload before sending it

## Related Docs

- [Remember Command](remember.md)
- [Curate Command](curate.md)
