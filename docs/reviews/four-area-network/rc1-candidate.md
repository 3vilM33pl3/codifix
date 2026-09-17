# 2.1.0-rc.1 local candidate

Status: **build candidate only**. Not signed, published, installed or activated.
This is not completion of the four-area foundation or permission to cut over.

## Local build status, 2026-09-17 14:08 UTC

- Source `5827aeb`: native and ARM64 CLI/operator/gateway compilation completed;
  both Debian assemblies failed under the private umask. The ARM64 CLI was
  statically inspected, not executed on ARM64 or the CA.
- Source `a1c5e4ee`: an unsigned AMD64 package assembled successfully in
  `/data/tmp/memory-four-area-rc1-artifacts-a1c5e4ee/`. It predates the runtime
  dependency and Web-tooling fixes and is **superseded; do not install/sign it**.
- Source `57dd6a77c218bc641bc41258b22aad29154074b0`: corrected builds are in progress.
  Outputs are `/data/tmp/memory-four-area-rc1-amd64-artifacts-57dd6a77/` and
  `/data/tmp/memory-four-area-rc1-arm64-artifacts-57dd6a77/`. Completion, package
  metadata, checksums and extracted-artifact verification remain to be recorded.
  Their logs are `/data/tmp/memory-four-area-rc1-57dd6a77-amd64-build.log` and
  `/data/tmp/memory-four-area-rc1-57dd6a77-arm64-build.log`.

The corrected build checkouts retain their original directory names
`/data/tmp/memory-four-area-rc1-a1c5e4ee` (AMD64) and
`/data/tmp/memory-four-area-rc1-5827aeb` (ARM64); both were cleanly fast-forwarded
to the exact `57dd6a77` revision only after their previous commands exited.
Directory names are not source identity. No original artifact was overwritten.

The real native `memory --version` and `memory-area-import schema-plan` smoke
checks passed in a non-root, network-none, read-only container. The latter binds
head 0057 to schema digest
`a67df68d9fd458173172b7a314b0478d00b8abd8e7d956e8a6011ec5710f696d`,
without connecting to a database. Three clean-launcher tests and 17 signed
staging-helper tests also pass. Their fixture signing keys are not release trust.
No image was installed on the CA; no real release was signed or published.

## Included software

- Strict, lossless area inventory and reviewed-partition tooling; protected local
  importer with immutable reconciliation receipts and retired runtime authority.
- Explicit schema-only preparation for fresh local targets, separate from
  service bootstrap and confirmed by a reviewed compiled migration digest.
- Opt-in named-instance packaging with fixed executables, clean environments,
  disabled staging automation and separately scoped private configuration.
- Private federation gateway binary, existing reviewed knowledge/expertise
  workflows and CA console preflight/authenticated staging tools.
- Supervisor policy/budget persistence, deterministic metadata-only observations,
  and audited Web/CLI/TUI pause/resume. Full Pi agency remains disabled.

The package does not include a private dataset, ownership approval, provider key,
Pi executable, configured gateway/PDS, target database or TLS/enrollment material.
No package hook starts a service, runs an import or rotates credentials.

## Compatibility and validation boundaries

Workspace and client versions identify this candidate as `2.1.0-rc.1`.
Debian's internal version is `2.1.0~rc.1`; artifact names retain SemVer. Additive
migrations extend the source snapshot's head 0052 through 0057. Normal app startup
runs migrations and bootstraps administration; do not start against an existing
database as a package smoke test. No automatic database downgrade is supported.

Focused software evidence is in the [execution ledger](README.md). Full release
validation, actual AMD64/ARM64 artifacts, independent signing trust, CA runtime
compatibility, reviewed real-data partitioning, complete restore/writer fencing,
host isolation and four-instance enrollment are separate open gates. Existing
services remain unchanged. The upstream Pi artifacts require glibc 2.43; a
Memory package build does not prove Pi can run on the CA.

Build only from a pinned clean source revision and keep outputs on the build
volume. Record source revision, toolchain, hashes, package metadata and smoke-test
results before any signing/publishing decision. Do not present a synthetic or
host-only smoke test as deployment evidence.
