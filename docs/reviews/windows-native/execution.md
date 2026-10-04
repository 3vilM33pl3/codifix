# Full native Windows port and VM validation

## Summary

Port all currently implemented Memory Layer functionality to Windows 11 x64, including independent instances, federation gateway/PDS, supervisors, watchers, MCP, CLI/TUI, Web, evaluation tooling and installation/recovery operations.

Windows will run native executables and services. PostgreSQL, pgvector, PDS and Pi will also run natively, without WSL or Linux containers. Deliver a tested 2.1.0-rc.5 Windows candidate.

Use a disposable VM on Monolith and synthetic data. Existing installations remain operational; R3’s paid campaign remains paused.

## 1. Establish the VM and prove native dependencies

- [ ] Checkpoint the approved plan through working Memory and create `codex/windows-native-port` from the current federation work. Maintain small commits and a functionality checklist covering every shipped command, background worker and operator tool.
- [ ] Create a Windows 11 x64 VM under `/data`, initially with 8 vCPUs, 16 GiB RAM and a 200 GiB thin-provisioned disk, UEFI and virtual TPM. QEMU, libvirt and TPM tooling already exist. Enable and verify KVM acceleration first; the AMD module is available but `/dev/kvm` is currently missing.
- [ ] Use verified Microsoft installation media. Reuse the existing ISO if its edition and licensing are suitable; otherwise use Microsoft’s evaluation media. Create a clean snapshot, then install the build tools in a second snapshot.
- [ ] Configure guest-agent access, key-authenticated SSH and a graphical console on a private test network. Automate builds, log collection, reboots and snapshot restoration so routine testing requires no commands from Olivier.
- [ ] Prove the difficult dependencies immediately: native PostgreSQL 17 with pgvector, the pinned PDS under Node, and the pinned patched Pi executable with a fake provider. Record exact toolchain versions and artifact hashes. Fix concrete portability failures before progressing to the full application.

Retain the current PDS, Atrium and Pi protocol/source baselines; record necessary Windows portability patches separately.

## 2. Port shared operating-system behavior

- [ ] Extend the existing platform layer with Windows implementations for protected files, ownership/access checks, file identity, locking, atomic replacement, process-tree termination and local IPC. Replace unconditional Unix imports and Windows rejection stubs with equivalent behavior.
- [ ] Use Windows ACLs and handle-based validation for credentials, grants, release manifests and journals. Cover junctions/reparse points, hard links, replacement during reads, case-insensitive collisions, reserved filenames and alternate data streams.
- [ ] Support named instances through native Windows services with separate service identities and protected state directories. Keep the watcher manager in the interactive user’s context so it can observe that user’s agent sessions. Preserve existing per-user installations and provide an explicit migration path to named instances.
- [ ] Complete area routing, duplicate-watcher prevention, offline DuckDB ownership, transactional replay and process discovery on Windows. Preserve IDs and stored provenance; distinguish original source paths from current Windows repository locations.
- [ ] Port release installation, reconciliation, backup, restore and upgrade operations. Preserve their preview, journal and recovery semantics. Use immutable release directories and switch service configuration between versions, avoiding replacement of running executables.

Existing HTTP, MCP, record and federation wire contracts remain compatible. No database schema change is expected solely for this port.

## 3. Native federation, supervisors and evaluation

- [ ] Run the gateway and PDS as native Windows services. Keep PDS storage and credentials private. Provide an ACL-protected named-pipe transport between the PDS and its authorized gateway/operator processes; retain the existing HTTPS transport for other deployments. The Windows transport changes must preserve signed repository bytes, CIDs and ATProto behavior.
- [ ] Port enrollment, credential refresh, publication recovery, collection review, addressed expertise exchange and retention. Exercise all four project instances with separate databases and identities.
- [ ] Build the pinned Pi source for Windows with the existing no-tools correction. Implement its supervisor sandbox using Windows AppContainer isolation and Job Objects, with explicit filesystem grants, bounded resources and broker-only IPC. Provider credentials remain with the trusted broker. Use narrowly scoped Windows patches for Pi’s broker transport where required.
- [ ] Preserve supervisor grants, daily budgets, uncertain-attempt accounting, cancellation, proposal review and restart recovery.
- [ ] Port evaluation execution and coding-workspace isolation to native Windows processes. Replace shell and Docker assumptions with explicit platform adapters, keep hidden scoring material outside agent access, and record the Windows isolation profile in campaign artifacts. Verify with deterministic agents and fake providers; existing Linux campaign artifacts retain their original meaning.
- [ ] Verify the remaining product workflows: capture, curation, search, graph/retrieval, history, corrections, durable jobs, imports/exports, authentication administration, loops, skills and all user interfaces. A feature cannot be marked complete merely because its Windows code compiles.

## 4. Packaging and repeatable builds

- [ ] Extend Windows packaging to include every required first-party executable, Web assets, skills, operator tools and the native supervisor components. Supply pinned prerequisite installation for native PostgreSQL/pgvector and Node/PDS, with third-party notices.
- [ ] Retain the portable ZIP and per-user MSI. Add a machine-level server installer for named services and the complete local stack. Preserve user data during repair, upgrade and uninstall.
- [ ] Introduce a versioned release-manifest format supporting Windows artifacts while preserving existing Linux manifest verification. Give MSI packages an explicit, increasing installer build number so RC upgrades work; retain the full application version in product output.
- [ ] Use the Windows VM for complete builds and integration/release tests. Add a hosted Windows CI job for early compilation and targeted tests, sharing the same build scripts. Trusted release runs use the VM’s existing self-hosted runner labels.
- [ ] Produce checksummed candidate packages, dependency/build metadata and installation instructions. Record signing status accurately; public publication is separate from creating and testing the candidate.

## 5. Acceptance and delivery

- [ ] Fresh install, repair, RC-to-RC upgrade, uninstall, reboot and recovery from an interrupted upgrade.
- [ ] CLI/TUI, browser and MCP workflows; watcher capture; Unicode/space-containing paths; area routing; offline capture and replay.
- [ ] Required-database Rust tests on native PostgreSQL/pgvector, Web tests/build, formatting, Clippy, boundary checks and existing Rust/Lean conformance.
- [ ] Four native Windows instances exchanging reviewed knowledge and cited answers through the native gateway/PDS.
- [ ] Windows↔Linux interoperability using a separate disposable Linux peer, including duplicate delivery, reconnect, revocation and signed-repository verification.
- [ ] Native Pi execution with fake providers, disclosure restrictions, budget enforcement, cancellation and restart; denial of access to another area, credentials, hidden tests and unauthorized network destinations.
- [ ] Backup and restore of databases, PDS repositories, identities, jobs and receipts into a clean VM snapshot.
- [ ] Final installation and interactive checks on the clean snapshot without developer tools, to detect missing runtime dependencies.
- [ ] Deliver candidate installers/ZIP, repeatable VM setup and test scripts, a functionality checklist with evidence, and a short installation/upgrade runbook. Every current product capability needs a working native Windows path and an observed result.
