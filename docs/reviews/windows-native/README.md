# Native Windows execution evidence

Target: **2.1.0-rc.5**, Windows 11 x64, no WSL or Linux containers.
The approved [execution contract](execution.md) is the completion checklist.

## Starting point — 2026-10-04

- Branch `codex/windows-native-port`, based on federation commit `94e5ee05`.
- Approved-plan checkpoint succeeded through the working Workshop connection:
  capture `2e8830d0-5041-4794-a877-3ff29736df9a`, plan memory
  `81d56105-6267-4229-96ab-852cd07c69ea`, thread `windows-native-port`.
- No production services, customer data, CA state or paid evaluation are in scope.
- Existing Windows packaging builds only `memory.exe`; existing service helpers
  use per-user scheduled tasks. These do not establish native server parity.
- PDS/Atrium baselines remain in `fixtures/federation/upstream-lock.json`.
- Pi remains v0.5.1 at `7c018475d54d4e09c907691e47a520f1f09e3288`
  with the existing no-tools correction. Windows patches must be recorded separately.
- Host has QEMU/libvirt/OVMF/swtpm. `kvm_amd` is available but not loaded;
  `/dev/kvm` is absent. Temporary sudo must be restored for the VM host operation.
- Existing Microsoft consumer ISO SHA-256 matches Microsoft's download table:
  `66b7b4b71763ed6f9b2ce29326ed9284544da6f5283d00329921540c01aaaeea`.
  A consumer licence has not been established, so use Enterprise evaluation media.

## Shipped functionality inventory

All rows start **pending observed native execution**, not implicitly passed
because the Linux workflow or Windows compilation works. Acceptance evidence
must identify the frozen commit, executable hashes, OS/toolchain and test command.

| Component | Required native evidence |
| --- | --- |
| `memory.exe` | CLI inventory below, real TUI and MCP stdio/HTTP |
| `memory-area.exe` | snapshot-bound review/materialization, full source accounting |
| `memory-area-import.exe` | fresh import and existing-target reconciliation, replay/conflict fencing |
| `memory-gateway-schema.exe` | isolated gateway schema initialization |
| `memory-federation-enroll.exe` | enrollment, refresh, identity preservation/recovery |
| `memory-federation-gateway.exe` | native service, PDS pipe ACL, signed publications/expertise |
| `memory-pi-sandbox.exe` | AppContainer/Job Object denial probes, broker-only fake provider |
| `memory-workshop-worker.exe` | grants, budgets, cancellation, uncertainty/restart |
| Web assets | tests/build, browser workflows, area/release/federation indicators |
| Skills and plugins | release-bound installation, watcher discovery, operator-bound MCP destinations |
| Watchers/manager | interactive user context, every supported session source, area identity/offline replay |
| Operators | release verification, installation, repair, upgrade, recovery, backup/restore, reconciliation |
| Native prerequisites | PostgreSQL 17/pgvector, pinned Node/PDS, patched Pi; hashes and versions |
| Federation fleet | four independent databases, access denials, Windows↔disposable Linux exchange |
| Evaluation | fake-provider native coding isolation; old Linux artifacts unchanged, paid R3 paused |
| CI/release | hosted Windows checks, VM builds, ZIP/user MSI/server MSI, RC upgrade ordering |

### CLI top-level inventory at base commit

`wizard`, `init`, `demo`, `tour`, `remember`, `query`, `resume`, `tui`,
`status`, `health`, `doctor`, `capture`, `curate`, `jobs`, `supervisor`,
`scan`, `ingest`, `proposals`, `consolidate`, `structure`, `review`,
`archive`, `scores`, `validate`, `history`, `prune-history`,
`verify-provenance`, `bundle`, `checkpoint`, `activities`, `up-to-speed`,
`service`, `auth`, `federation`, `watcher`, `agent`, `loops`, `mcp`,
`embeddings`, `commits`, `repo`, `graph`, `eval`, `upgrade`.

Subcommand checks must cover administrative writes as well as happy-path reads.
Background coverage includes durable jobs, validation/consolidation, federation
publication/sync/retention, expertise answering, supervisor attempts, watcher
management/offline replay and credential maintenance. No schema migration is
authorized solely to compensate for platform differences.

## Reporting rules

Keep `execution.md` unchecked until its entire requirement has observed evidence.
Separate host-script unit tests, cross-target compilation, native VM tests and
clean-install release acceptance. Never label a blocked or unexecuted feature as
working. Signing/public publication and real-data activation remain separate.
