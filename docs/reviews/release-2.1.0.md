# Memory Layer 2.1.0 release execution

Approved 17 October 2026: publish stable/latest `v2.1.0` for Linux amd64/arm64,
macOS Intel/Apple Silicon and Windows x64 **preview**, plus source. Existing
Monolith, Mac and CA installations are not upgraded by this task.

Workshop checkpoint succeeded before edits: plan thread
`release-memory-layer-2-1-0-across-all-platforms`, 22 checklist items;
capture `3acbd25f-7ad2-4354-81e3-d02912e611a7`, baseline `0f5fd703`.

## Progress

- [x] Checkpoint approved scope and inspect clean pushed main.
- [x] Align application, workspace, Web, docs and bundled skill metadata to 2.1.0.
- [ ] Stable Linux manifest acceptance and RC/stable ordering with signed-fixture tests.
- [ ] Packaging/workflow preparation and actual test/Clippy repairs.
- [ ] Freeze one clean source commit for all builds.
- [ ] Consolidated required-database Rust and frontend verification.
- [ ] Linux Bookworm-compatible packages and ABI/install verification.
- [ ] Both Mac architectures, standard and runtime-preserving update packages.
- [ ] Windows native ZIP/user MSI/server-tools MSI and disposable package checks.
- [ ] Astra final review, exact artifact inventory and checksums.
- [ ] Publish stable/latest release and verify downloaded assets.
- [ ] Update both Homebrew formulae to verified published source hash.
- [ ] Commit/push evidence, verify approved plan and remember result.

## Boundaries

- Windows installer build 1 is the public sequence, distinct from private lab
  builds 60000/60001. Windows remains `native_stack_bundled: false`; the
  server-tools MSI is not a complete PostgreSQL/PDS/Pi installer.
- Standard Mac packages document prerequisites. Application-update packages
  retain existing bundled runtime data and credentials. No new runtime bundle
  or live-Mac installation is part of release validation.
- Artifacts use locked security-fixed sources; previous package/runtime receipts
  cannot be reused as evidence for rebuilt artifacts.
- Disclose unsigned/notarized status accurately. Preserve trust verification.
- [Dependency evidence](security-dependencies-20261017.md) records the remaining
  unpatched RSA advisory and public-key-only OIDC reachability assessment.
- No paid models, R3 campaign, customer data, live service or OS changes.

Build/test results and publication hashes will be recorded below as observed.
