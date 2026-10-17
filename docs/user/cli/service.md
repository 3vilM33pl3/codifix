# `codifix service`

Use the `service` commands to enable, start, and inspect the shared backend service installed on your machine.

## Table of Contents

- [What It Controls](#what-it-controls)
- [Common Commands](#common-commands)
- [Platform Notes](#platform-notes)
- [Development And Source Use](#development-and-source-use)

## What It Controls

These commands manage the installed shared `codifix service` backend.

They are for the normal packaged setup, not for source-level development runs like `cargo run --bin codifix -- service run`, which activate a fully isolated dev profile on different ports. See [Dev Stack vs Installed Stack](../../developer/dev-stack.md) for the full contract.

## Common Commands

Enable or start the service:

```bash
codifix service enable
```

Preview the service action without changing the machine:

```bash
codifix service enable --dry-run
codifix service disable --dry-run
codifix service ensure-api-token --rotate-placeholder --dry-run
codifix service restart-all --dry-run --json
```

This command also provisions the shared service API token automatically if it is missing or still set to the old development placeholder.

Check service status:

```bash
codifix service status
```

Rotate an old placeholder token manually:

```bash
codifix service ensure-api-token --rotate-placeholder
```

Restart active services after an install or upgrade:

```bash
codifix service restart-all --mark-tui-restart --json
```

`restart-all` restarts only services that are already active or loaded. It does not start services that the user intentionally stopped. On Linux it checks the system backend service and active user watcher services. When run as root during a package install, it also checks logged-in user systemd sessions under `/run/user/*`. On macOS it checks loaded Codifix LaunchAgents. On Windows it checks the current user's `MemoryLayer-*` scheduled tasks.

Installers pass `--mark-tui-restart` so a TUI that is already open can show `restart` in red in the bottom status bar. Restarting the TUI is still a user action; the installer does not kill the running terminal UI.

Health checks from the client side:

```bash
codifix health
codifix doctor
```

## API Token Security

The HTTP API does not trust `Origin` or `Referer` headers as authentication, even when they point at localhost. API clients must send the configured service token with `x-api-token`. The bundled same-origin web UI bootstraps that token from the service and attaches it to its own API requests.

## Platform Notes

On macOS, `codifix service enable` manages the LaunchAgent.

On Windows, `codifix service enable` registers and starts the current-user, least-privilege `MemoryLayer-Backend` Task Scheduler task. It starts at sign-in and is configured to restart after failures. Use the same CLI commands to inspect, restart, or remove it; no administrator shell is required.

Native named server instances are separate from that legacy per-user task.
Select the exact installed configuration to inspect one:

```powershell
memory --config "C:\ProgramData\MemoryLayer\configurations\homelab\codifix.toml" service status
```

The actual installation root comes from Windows' native known folders. Status
checks the protected installation registration and completed operation receipts,
then compares the SCM command/account to that exact instance. It does not guess
a service name from an area label, adopt an unrelated registration, or fall back
to `MemoryLayer-Backend` when the selected instance is unavailable. Reading the
protected receipts requires the installing operator's existing access.

This is read-only SCM status—not an HTTP health check, configuration-content
verification or a fresh binary-integrity check. Named-instance enable, disable
and restart through these generic helpers remain unsupported and fail before
token writes or legacy-task changes. Use the journaled native-instance operator
for installation/upgrades; named start/stop integration is separate work.

On Linux, the packaged service is usually managed with:

```bash
sudo systemctl enable --now codifix.service
codifix service restart-all --mark-tui-restart
```

Use `systemctl` directly only when debugging a specific unit.

## Development And Source Use

When working from source, `cargo run --bin codifix -- service run` starts a **dev** backend that is fully isolated from any installed service on the same machine: a different HTTP port (`4250` vs `4040`) and its own user-local project runtime state. The dev profile ignores the installed global config entirely.

Bootstrap is one-time per checkout:

```bash
cargo run --bin codifix -- init
cargo run --bin codifix -- dev --copy-from-global
cargo run --bin codifix -- service run
```

`--copy-from-global` lifts the database URL and LLM/embedding endpoints out of the installed config into the dev overlay. Without it (and without a TTY) you will need to populate those sections in the user-local project `config.dev.toml` by hand.

For the full isolation contract, override flags, verification steps, and troubleshooting, see [Dev Stack vs Installed Stack](../../developer/dev-stack.md).

### PostgreSQL-only service build

An isolated instance that explicitly disables offline storage can build the
service and TUI without DuckDB:

```bash
cargo build -p codifix-cli --bin codifix --no-default-features --features tui,embedded-service-postgres --locked
```

Set `[offline] enabled = false` in its explicit configuration. Database outages
cannot fall back to local capture in this build. Default/full builds and the
existing `embedded-service` feature still include offline storage; `--features
tui --no-default-features` remains a client-only build and cannot run a service.
