# Dev Stack vs Installed Stack

Codifix is designed so you can run a development copy from a `cargo` checkout on the same machine that already has the packaged service installed (Debian, Homebrew, or a user-systemd unit) without any conflict between them.

This page is the canonical reference for that split: how the dev profile activates, what gets isolated, and how to bootstrap a fresh checkout.

## Table of Contents

- [Quickstart](#quickstart)
- [How The Profile Is Detected](#how-the-profile-is-detected)
- [What Is Isolated](#what-is-isolated)
- [What Is Shared](#what-is-shared)
- [Default Endpoints](#default-endpoints)
- [Verifying Isolation](#verifying-isolation)
- [Project Memory Routing And Recovery](#project-memory-routing-and-recovery)
- [Common Pitfalls](#common-pitfalls)

## Quickstart

From a fresh clone, with the packaged service already installed and configured:

```bash
# 1. Clone and build the frontend (web UI is served by the dev backend too).
git clone https://github.com/3vilM33pl3/codifix
cd codifix
npm --prefix web ci && npm --prefix web run build

# 2. Bootstrap the user-local project base config and the dev overlay.
cargo run --bin codifix -- init
cargo run --bin codifix -- dev --no-copy-from-global
# Configure [database].url in the user-local project config.dev.toml with
# a dedicated development database/login before starting the backend.
# PostgreSQL must have the vector extension available.

# 3. Run each piece in its own shell. All three target the dev stack.
cargo run --bin codifix -- service run            # backend (4250 HTTP)
cargo run --bin codifix -- watcher manager run    # optional: project watchers
cargo run --bin codifix -- tui                    # TUI shows [dev] in the header
```

The default source build includes both the TUI and embedded service so every
command above works without feature flags. For an explicitly lean TUI build
that cannot host the backend, use `cargo build -p codifix-cli --no-default-features
--features tui`.

Use a dedicated development database and login for backend tests. The doctor
command reports the resolved user-local configuration paths. On this checkout,
the existing isolated setup is described under [routing and recovery](#project-memory-routing-and-recovery).

`codifix dev --copy-from-global` remains an explicit opt-in that copies the
installed database URL and model endpoints. It can give development code access
to live installed data; it does not create an isolated database. Do not use it
for this project's normal backend-test setup.

If you skip `codifix init`, `codifix dev` will refuse with a clear message — the overlay layers on top of the user-local project `config.toml` and needs that file to exist first.

When the dev stack is no longer needed, just stop the three processes. There is nothing to clean up on the installed side.

## How The Profile Is Detected

Every `codifix` binary picks its profile at startup:

| Condition | Profile |
| --- | --- |
| Binary path is under `target/{debug,release}/` next to a `Cargo.toml` | `dev` |
| Anything else (packaged install, `~/.cargo/bin/`, systemd unit) | `prod` |
| `CODIFIX_PROFILE=dev` or `CODIFIX_PROFILE=prod` set | overrides both |

Practical consequences:

- `cargo run --bin codifix -- ...` is always dev.
- `~/.cargo/bin/codifix ...` is prod even though it lives in your home dir.
- Set `CODIFIX_PROFILE=prod` to force a `target/debug/codifix` invocation onto the installed stack (rare; mostly useful when reproducing prod-only bugs against a debug build).

The TUI shows `[dev]` in its header when the profile is dev so you cannot mistake one for the other.

## What Is Isolated

The dev profile reads the user-local project `config.toml` and then layers the user-local project `config.dev.toml` on top of it. The overlay deliberately diverges on the values that would otherwise collide with the installed stack:

| Setting | Installed default | Dev default |
| --- | --- | --- |
| `service.bind_addr` | `127.0.0.1:4040` | `127.0.0.1:4250` |
| `automation.state_file_path` | system path | user-local project `runtime/dev/automation-state.json` |
| `automation.audit_log_path` | system path | user-local project `runtime/dev/automation.log` |
| `cluster.service_id` | derived from bind addr | `codifix-dev` |

Importantly, **the dev profile ignores the global config entirely**. That is what keeps a cargo-run service from silently picking up packaged machine-wide settings. Anything the dev stack needs that's normally global (database URL, LLM/embedding endpoints) must live in the dev overlay, which is why `codifix dev --copy-from-global` exists.

Dev binaries also **advertise themselves with a `-dev` version suffix**. `codifix --version`, `codifix-service --version`, `codifix-watch --version`, the `/healthz` JSON `version` field, the cluster discovery packet, and the TUI version panel all report `0.6.0-dev` rather than `0.6.0` when the profile is dev. That way logs, peer lists, and health checks cannot silently conflate a dev service with an installed one.

## What Is Shared

The configured database URL determines whether storage is shared. This
project's backend-test setup uses a separate database. If an operator explicitly
copies the installed URL into the dev overlay, both services write the same live
database; separate ports do not provide data isolation. There is no automatic
synchronization between separate databases. Durable development notes instead
go directly through the Workshop API, as described below.

Tables that are willingly copied from the global config when you opt in:

- `[database]`
- `[llm]`
- `[embeddings]`
- `[features]`
- `[writer]`

Service endpoint, automation paths, and cluster id are intentionally excluded — those are the values that *must* diverge for isolation to hold.

## Default Endpoints

After `codifix dev` with defaults, ports look like this:

| Stack | HTTP |
| --- | --- |
| Installed (Debian/Homebrew package) | `127.0.0.1:4040` |
| Dev (cargo-run from repo) | `127.0.0.1:4250` |

Override the dev ports at bootstrap time:

```bash
cargo run --bin codifix -- dev \
  --bind-addr 127.0.0.1:4260 \
```

## Verifying Isolation

Four quick checks:

```bash
# 1. The TUI header.
cargo run --bin codifix -- tui                 # header reads [dev]

# 2. Version string of each binary.
cargo run --bin codifix -- --version           # codifix 0.6.0-dev
/usr/bin/codifix --version                     # codifix 0.6.0

# 3. Health endpoint of each stack (reports "version" with or without -dev).
curl -s http://127.0.0.1:4040/healthz         # installed
curl -s http://127.0.0.1:4250/healthz         # dev

# 4. Doctor reports the active profile and resolved overlay path.
cargo run --bin codifix -- doctor
```

`codifix doctor` will surface the resolved config and overlay paths under the `config.*` checks.

## Project Memory Routing And Recovery

For development of Codifix itself, keep **durable project knowledge** in
Workshop's `codifix` project. This repository's shared Go helper explicitly reads
`.agents/codifix-command.json` (`["codifix", "--area", "workshop"]`). Task
starts, plans, completed work and normal memory queries therefore use Workshop's
API, including while a development backend is stopped or being restarted.

```bash
go run ./.agents/skills/codifix/scripts/main.go \
  query-memory 'What changed in this project?' codifix
```

The **dev service and database** are for backend tests. Test records do not
automatically synchronize to Workshop. Select dev explicitly when testing the
helper:

```bash
CODIFIX_BIN='cargo run --quiet --bin codifix --' \
  go run ./.agents/skills/codifix/scripts/main.go \
  query-memory 'A synthetic test question' codifix
```

Source CLI commands without an explicit area force the dev profile and omit
inherited `CODIFIX_AREA`, `CODIFIX_AREA_ROUTING`, `CODIFIX_CONFIG`
and `CODIFIX_CLIENT_TOKEN`. Explicit `--config` command arguments remain
available for intentional test configurations. `CODIFIX_BIN` takes
precedence over the project command file. Neither a dev failure nor a Workshop
failure switches to another destination or retries a write automatically.

If dev reports connection refused, an authentication failure, or a degraded
database, recover from the checkout in a shell without installed-area routing:

```bash
unset CODIFIX_AREA CODIFIX_AREA_ROUTING
unset CODIFIX_CONFIG CODIFIX_CLIENT_TOKEN
export CODIFIX_PROFILE=dev
cargo run --bin codifix -- doctor
# In a separate terminal with the same dev environment:
cargo run --bin codifix -- service run
# Then check both HTTP and PostgreSQL health:
cargo run --bin codifix -- health
```

Require `status=ok` and `database=up`; a listening HTTP port may be serving only
degraded offline responses. Check the configured endpoint rather than assuming
4250 if the dev bind address was customized. For failed write requests, inspect
the checkpoint and activities before replaying them: a local file or remote
write may have completed before the failure was reported. For authentication
errors, inspect the dev overlay and client identity rather than starting another
service. Do not use packaged `codifix service enable` to recover a source backend.

The restored Monolith dev instance uses a dedicated local PostgreSQL database
and login named `codifix_project_dev`. Its protected user-local dev overlay holds
the credentials; the previous overlay is preserved privately in the rename
recovery directory. The old dev database is retained, and the old
`memory-layer-dev.service` is disabled. The retired shared `memory` login remains
disabled. Cluster discovery, offline replay,
automatic background work, embeddings and model curation are disabled for this
test instance. Workshop credentials and database settings are unchanged.

For a persistent user-managed dev process on Linux, the repo includes
`scripts/systemd/codifix-dev.service`, with the checkout defaulting to
`%h/Projects/codifix`. Review its paths and build the source executable before
installing it:

```bash
cargo build --bin codifix
install -m 0644 scripts/systemd/codifix-dev.service \
  ~/.config/systemd/user/codifix-dev.service
systemctl --user daemon-reload
systemctl --user enable --now codifix-dev.service
# After rebuilding the backend:
systemctl --user restart codifix-dev.service
journalctl --user -u codifix-dev.service -n 30 --no-pager
```

Stop any foreground or transient dev process on the same port before starting
the persistent unit. This unit starts only the source backend, using the dev
overlay; it does not alter packaged or area services.

## Common Pitfalls

**`dev profile active but <project config>/config.dev.toml is missing`**
You ran a `target/debug/codifix` binary in a checkout that has not been bootstrapped yet. Run `codifix init && codifix dev` (or `CODIFIX_PROFILE=prod` to force the installed stack instead).

**Dev stack reads no LLM key even though the installed stack works**
The dev profile does not fall back to the global config. Either rerun `codifix dev --copy-from-global --force` or add `[llm]`/`[embeddings]` blocks directly to the user-local project `config.dev.toml`.

**Two installed services on one machine fighting for `127.0.0.1:4041`**
This happens when both a system-wide (`/etc/codifix/`) and a user-level (`~/.config/codifix/`) install are enabled. Pick one or give the user-level install its own `bind_addr`. This is unrelated to the dev stack.

## Related Docs

- [Getting Started](../user/getting-started.md)
- [Service Commands](../user/cli/service.md)
- [Doctor Diagnostics](../user/cli/doctor.md)
- [Architecture Overview](architecture/overview.md)
