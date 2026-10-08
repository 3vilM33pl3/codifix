# Linux Codex Desktop watcher discovery

Implemented at Olivier's request after Soundmind was not discovered on Monolith.
The Linux collector previously excluded `codex app-server`; the Desktop process
has a home-directory cwd and multiplexes multiple repository transcripts.

The lightweight collector now reads all rollout paths held open by each live
Desktop host through `/proc/<pid>/fd`, deduplicates paths, and reads session
metadata for repository identity. Host PID and start time retain existing
process-lifetime fencing. It does not scan historical session directories or
change Windows discovery. Repository grouping still selects one watcher owner.

## Checks and deployment

- `cargo test -p mem-agenttop --locked`: 29 passed.
- Targeted all-target Clippy with warnings denied, formatting, diff checks and
  boundary checks passed.
- Lean CLI/TUI build passed.
- Code commit: `f6bbbc1e`.
- Monolith watcher-only executable:
  `/home/olivier/.local/state/memory-desktop-watch-f6bbbc1e/memory`.
- SHA-256: `bec9244a278f02d23e009480e015af0b44efc4f677a3d89855673851b013def1`.
- A user-service override, `memory-watch-manager.service.d/desktop-discovery.conf`,
  selects this executable. Frozen rc.5 packages and service executables remain
  unchanged; this is a local watcher hotfix, not a newly packaged release.

After manager restart, three repositories were discovered with no warnings:
Soundmind → General, Memory → Workshop, and Homelab → Homelab. The General
service's project runtime reports one active Soundmind watcher, zero unhealthy,
and status `ok`. The old `watcher status` subcommand reports the legacy standalone
unit, not the manager-owned unit; use `memory status --project soundmind` from
the Soundmind repository or inspect the manager's units instead.

The dev checkpoint service and its existing isolated database container were
stopped after reboot. They were restarted unchanged; health and the task-start
checkpoint succeeded before implementation.

## Recovery

To revert only this discovery hotfix, move the user-service override aside,
reload user systemd, and restart the manager. Its base unit still points at the
preserved rc.5 client. Stop hotfix-owned transient watchers before reverting if
switching their executable too. No database migration or identity change occurred.
