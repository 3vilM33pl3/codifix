# Main Mac Memories tab — observed update

## Scope and behavior

The Mac TUI now combines local work with the selected project's synchronized
curated owner knowledge in its main Memories tab. Owner copies are labelled
`Synced/read-only`; detail and retained history preserve author, version and
digest. Delete and validation cannot mutate these copies. Accepted local
contributions are suppressed only after their authoritative owner copies arrive;
pending/conflicting work remains visible. No database migration was needed.

The default API/repository listing remains local-only. Only the authenticated,
loopback local-human browse view opts in, using bounded, no-store pages and a
content-bound revision. Shared task/plan selection and evaluation are unchanged.
Web Memories has not been unified by this task; Web Query/Offline remains the
existing way to read downloaded knowledge.

## Native artifact and deployment

Built natively on Olivier's ARM64 Mac from frozen commit `7bd1e36d`:

- Version: `2.1.0-rc.5`, internal ad-hoc signed candidate.
- SHA-256: `bf6115fd035348378ebedc07e74996f41459c72a2d2eb029df4edd2120b2d299`.
- `codesign --verify --strict` passed; loader inspection found system libraries
  and frameworks only, without build-directory runtime dependencies.
- Immutable directory:
  `/Users/olivier/.local/share/memory-layer/releases/main-memories-7bd1e36d`.
- User launcher: `/Users/olivier/.local/bin/memory`. Quit/reopen the old TUI;
  use `~/.local/bin/memory tui` from the chosen repository if an old shell has
  cached the previous command.

Both named area service LaunchAgents were switched sequentially using an adapter
that verifies original helper hashes, original runtime/journal/configuration,
selected area and candidate executable hash/architecture before execution.
The original `/usr/local/bin/memory`, Keychain login, sync workers and watcher
launch configuration remain unchanged. This avoids another installer/login or
replacement of their executable-bound authority.

Private intent, before/after plists and results:
`/Users/olivier/Library/Application Support/memory-layer/onboarding-runtime-v3/memories-view-update-20261015`.
Recovery restores the selected area's `before.plist`, then bootout/bootstrap of
that exact LaunchAgent. Preserve accepted writes; no database rollback is needed.

## Verification

Observed against the running Mac after deployment:

| Area | Default local-only total | Main browse total | Pages | Duplicate rows |
| --- | ---: | ---: | ---: | ---: |
| Homelab | 2 | 434 | 5 | 0 |
| Workshop | 4 | 1,678 | 17 | 0 |

Both views returned no-store headers, a consistent revision across pages and
correct project/read-only bindings. The real Homelab SSH PTY TUI displayed 434
main-tab rows, owner content/attribution and the read-only delete/validation
guard. Offline projections retained 484/1,819 versions and 434/1,678 states;
existing query checks and sync status returned no sync error. These counts are
observations, not promises against future updates.

Software validation passed: five required-database laptop-cache tests on
disposable PostgreSQL; two focused CLI regressions; 99 TUI tests; full-feature
CLI/service check; reduced-feature CLI check; targeted strict Clippy; formatting,
boundaries and diff checks. Reduced-feature check retains existing dead-code
warnings. Astra's bounded review findings were addressed before deployment.

The native frozen commit precedes final guarded-unwrap lint cleanups and the
synthetic fixture's required search-document correction; runtime feature logic
is unchanged. This is not a new packaged/public release or a claim that all
broader onboarding/reboot/security acceptance is complete. No paid calls,
new disclosure grants, Trust/General activation or R3 execution occurred.
