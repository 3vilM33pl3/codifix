# `codifix dev`

`codifix dev` manages the isolated development-profile overlay used when running Codifix from a source checkout.

Use it only when developing Codifix itself. Normal users should use `codifix wizard --global`, `codifix wizard`, and the packaged service commands.

## Common Usage

```bash
codifix dev --copy-from-global
codifix dev --dry-run
codifix dev --no-copy-from-global
```

## What It Creates

`codifix dev` creates the user-local project `config.dev.toml` and dev runtime directory. Dev-profile binaries use separate defaults from installed packages:

| Stack | HTTP |
|---|---|
| installed package | `127.0.0.1:4040` |
| dev/cargo run | `127.0.0.1:4250` |

`--copy-from-global` copies database, LLM, and embedding endpoints from the global config into the dev overlay so a checkout can run without duplicating secrets by hand.

## Related Docs

- [Dev Stack vs Installed Stack](../../developer/dev-stack.md)
- [Running From Source](../getting-started.md#running-from-source)
- [Service Commands](service.md)
