# CA-compatible release preparation

Status: in progress; off-CA builds and disposable rehearsal only.

Olivier approved continuation on 2026-09-20 after the console inventory and
confirmed that the existing CA Memory service connects to the shared Monolith
database. The CA reports Debian 12, AArch64, glibc 2.36 and Memory 2.0.1.
This operator confirmation resolves the source topology, not pending offline
captures or local evidence coverage. No additional database-discovery request is
needed. Keep CA signing, OpenBao, the existing Memory service and previews running.

The earlier rc.1 Memory package and both upstream and patched Pi ARM64 artifacts
require glibc 2.43. They remain preserved evidence, not candidates for this CA.
Do not upgrade the CA libc/loader, relax its sandbox or substitute an unreviewed
runtime. Rebuild against Debian 12 and inspect the actual resulting artifacts.

## Approved preparation scope

- [x] Checkpoint this contract in dev Memory and inventory build prerequisites.
- [ ] Implement a pinned Debian 12 ARM64 build path for Memory and the existing
      `0.5.1+memory.no-tools.1` Pi patch, preserving upstream source/lock/license.
- [ ] Build candidates, inspect complete native ABI dependencies and execute
      isolated compatibility tests; label emulation separately from native proof.
- [ ] Implement and test selected-instance installation/rollback preparation in
      disposable environments without touching any host service or database.
- [ ] Prepare versioned console instructions, artifact hashes, health/rollback
      checks and a redacted offline-data reconciliation checklist.
- [ ] Record test evidence and remaining gates; checkpoint results in dev Memory.

The approved-plan checkpoint is task `d80052bc-9128-4e6e-9ec0-4c4fc0698ecc`,
plan `b1d6d75d-07b4-4d84-9acd-88d40180cee1`, thread key
`ca-compatible-release-preparation-and-rehearsal`, at source `312d7c77`.

## Boundaries

Preparation is on Monolith, with synthetic fixtures and no provider credentials.
It does not authorize installing on the CA, stopping original writers, activating
Trust/Pi, changing DNS/credentials, using an operator signing key or paid models.
Any Pi redistribution requires the separate license review already documented in
the [patch evidence](../../../crates/mem-supervisor-pi/patches/README.md).

Before actual cutover, reconcile offline work, take and independently restore a
fresh shared snapshot, review its delta, fence every identified original writer
(including the CA), and verify the new Trust database and publication authority.
Memory installation and Pi activation remain separate gates. A userspace ARM64
emulator does not prove the CA's kernel, cgroups, systemd or sandbox enforcement.
Use the [cutover runbook](staging-cutover.md) and
[console handoff](../../../deploy/areas/CA-CONSOLE-HANDOFF.md).
