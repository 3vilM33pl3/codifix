# Release Compatibility And Known Limitations

Memory Layer v2.1.0 is the current stable release. It adds private federation,
project/device onboarding, curated project synchronization and durable jobs.
Windows packages remain a preview of client/operator functionality, not a
complete native PostgreSQL/PDS/Pi installation. Existing PostgreSQL databases
migrate in place; back them up before starting the new application.

Version 2.1.0 embeds migrations through 0084. Already-applied migrations are
unchanged. Do not run an older executable against a newer database. Named
instances require an explicit selected-instance upgrade; downloading or
installing a package does not replace independently pinned instance releases.
For a bundled Mac installation, use the architecture-matched
`application-update.pkg` to preserve its runtime; standard packages require
separately provisioned dependencies and refuse to replace bundled runtimes.

See the [2.1.0 changelog](../../CHANGELOG.md) and
[dependency security evidence](../reviews/security-dependencies-20261017.md).
The unpatched upstream RSA advisory is retained, not suppressed; the inspected
OIDC path uses public-key verification rather than private-key operations.

Read the [v2.0.0 release](https://github.com/3vilM33pl3/memory/releases/tag/v2.0.0)
and [changelog](../../CHANGELOG.md) before updating a v1 production installation.

## v2.0.1 schema compatibility repair

Release `v2.0.1` restores migrations 33 through 52 to every release artifact.
Some `v2.0.0` artifacts only embedded migrations through version 32, so they
refused to start when pointed at a database that had already applied the newer
migrations. The `v2.0.1` application code is otherwise the `v2.0.0` release.

Upgrade the application package to `v2.0.1`; do not downgrade the database,
delete rows from `_sqlx_migrations`, or edit an applied migration. Existing
databases at migration 52 are accepted without changing their migration ledger,
while databases below migration 52 are upgraded normally.

After installing `v2.0.1`, restart the service and verify both the binary and
database health:

```bash
memory --version
memory service restart-all
memory doctor
memory health
```

## The v2 compatibility boundary

The v2 line preserves these documented boundaries:

- the current core CLI workflows and their documented `--json` response shapes
- append-only database migrations; already-applied migrations are never edited
- the OpenAPI operations marked `x-stability: core`; control-plane operations
  marked `internal` may evolve between minor releases
- read-only MCP query, search, resume, resource, and prompt tools
- deterministic bundle schema v2 exports and backward-compatible bundle v1 imports
- packaged operation on Debian amd64/arm64, Homebrew, macOS Intel/Apple Silicon,
  and Windows x86_64
- source/dev isolation from the installed service profile

Advanced surfaces remain deliberately conservative. Loop automations are
approval-gated, graph quality depends on repository/extractor coverage, and
evaluation claims require reviewed suites and passing gates.

## Upgrading from v1

Review these breaking changes before restarting the service:

- Remove `capnp_unix_socket` and `capnp_tcp_addr`; clients now receive live
  updates from the `/ws` WebSocket stream.
- Replace `memory automation flush` with `memory watcher flush`,
  `memory capture task` with `memory capture`, and `memory dev init` with
  `memory dev`. The duplicate `memory setup` command is gone; use
  `memory wizard`.
- Update direct API integrations against the running service's
  `GET /v1/openapi.yaml`. v2 removed `/v1/stats`, `/v1/offline/pending`, and the
  browser auth-token handoff, and consolidated several loop/activity routes.
- Treat `--writer-id` and `[writer]` as advisory labels. Durable authorship is
  derived from the authenticated principal.
- Reissue least-privilege service tokens where appropriate. Role names remain
  convenient presets, but authorization is enforced as explicit permission
  sets rather than an ordinal role ladder.
- Re-export shared bundles when practical. New exports use deterministic,
  content-addressed schema v2; existing schema v1 bundles still import.

## Upgrade guidance

Before upgrading:

```bash
memory status --project <project-slug>
memory doctor
pg_dump "$DATABASE_URL" > memory-layer-before-upgrade.sql
```

Debian installation does not start/restart services or rotate credentials. After
reviewing schema compatibility, restart only the selected installation. For the
legacy system singleton (not a user service or named instance):

```bash
sudo systemctl restart memory-layer.service
memory doctor
memory health
memory status --project <project-slug>
memory upgrade --dry-run
```

The v2.0.1 service embeds migrations through version 52 and applies any pending
migrations when it starts. Do not downgrade the binary against a migrated
database; restore the pre-upgrade database backup if you must roll back. Run
`memory upgrade` only after reviewing the dry run because it can refresh
repo-local `.agents/` skills and instructions.

For independently pinned named instances, package installation does not change
their executable. Follow the [named-instance staging contract](../../deploy/areas/README.md)
and the selected-instance release procedure; never use `restart-all` as an
implicit package-upgrade step. A database migration can require a coordinated
writer stop and cannot be undone by blindly selecting an older executable.

## Release artifacts

GitHub Releases publish the supported native installer set:

- Debian x86_64: `memory-layer_<version>_amd64.deb`
- Debian arm64: `memory-layer_<version>_arm64.deb`
- macOS Intel: `memory-layer-<version>-macos-x86_64.pkg`
- macOS Apple Silicon: `memory-layer-<version>-macos-aarch64.pkg`
- Windows x86_64: `memory-layer-<version>-windows-x86_64.msi`
- Windows x86_64 portable archive: `memory-layer-<version>-windows-x86_64.zip`
- Homebrew source archive: `memory-<version>.tar.gz`

For v2.1.0, replace `<version>` with `2.1.0`. Mac application-update installers
have the additional `-application-update.pkg` suffix. Windows preview filenames
include `-build.1`; the server-tools MSI is tools-only, not a bundled server.
Use the release's exact asset inventory and `SHA256SUMS` rather than guessing
filenames. Every published package has a matching `.sha256` checksum file.

The Debian arm64 package targets 64-bit ARM Linux, including Raspberry Pi 4/5
systems running a 64-bit Debian-family OS. Windows ARM64 and 32-bit Raspberry Pi
OS are not release targets yet.

## Current limitations

- Run `memory doctor` and `memory health` after every package upgrade.
- Review `memory upgrade --dry-run` before refreshing repo-local skills or
  instructions.
- Verify the installer architecture before installing: Debian publishes `amd64`
  and `arm64`, macOS publishes Intel and Apple Silicon packages, and Windows
  publishes x86_64 packages.
- The code graph UI depends on browser WebGL support and extractor coverage.
- The interactive browser demo uses static sample data; it does not prove that
  your local backend, database, or authentication is healthy.
- Loop automations remain approval-gated by design, and risky actions must stop
  for human review.

## Next

Read [Getting Started](getting-started.md), [Doctor Diagnostics](cli/doctor.md), or [Skill Upgrade](cli/upgrade.md).
