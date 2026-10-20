# Release compatibility and known limitations

These guides describe current `main`. Codifix v3.0.0 is the current stable release. See the
[v3.0.0 release](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0). The earlier
[v2.1.0](https://github.com/3vilM33pl3/codifix/releases/tag/v2.1.0) packages
belong to Memory Layer; there was no Codifix v2 release.

## Source and package versions

The workspace and plugin report `3.0.0`. Use the package's
`release-inventory.json` and checksums to match each artifact to its exact source
commit; a version string alone does not identify package contents. For a source build, record `git rev-parse HEAD` as well as
`codifix --version`. Later changes on `main` are not retroactively part of the
published release.

The release contains project memory, private federation, device/project
onboarding, synchronized knowledge and durable background jobs. Availability
still depends on build capabilities, explicit configuration, current grants
and the selected installation. A passing test or a controlled rehearsal does
not establish complete deployment or platform acceptance.

## Rename and recovery boundary

Codifix changes executable, package, service, configuration, plugin and
federation identities. Installing its package does not adopt Memory Layer's
state. Follow the [update guide](https://codifix.dev/docs/install/update) and
the [release-specific reinstall procedure](https://github.com/3vilM33pl3/codifix/blob/main/packaging/CODIFIX-3-REINSTALL.md)
for a reviewed cutover that preserves knowledge and recovery material.

Preserve a matched backup of the database, configuration, credentials,
attachments, state and original executables before a cutover. PostgreSQL
migrations are append-only and run at service startup; they do not perform the
product rename. The release embeds migrations through 0085 and adds no migrations beyond
0085. Never run an old
Memory Layer binary against a database after Codifix has accepted writes.

Named instances keep independently pinned executables. Installing a package or
running `codifix service restart-all` does not upgrade those instances. Follow
the selected-instance procedure; do not restart other areas to repair one.
After new writes, preserve accepted work and recover forward rather than
blindly choosing an older executable.

## Published package limits

| Platform | Published release and limitations |
| --- | --- |
| Debian/Ubuntu | amd64 and arm64 Debian packages are published. PostgreSQL with pgvector is a separate prerequisite. Debian packages are named `codifix_3.0.0_ARCH.deb`. Follow the exact Linux download instructions. |
| macOS | Intel and Apple Silicon standard/application-update packages are published. They are unsigned and not notarized; native pixel-level UI acceptance is not claimed. Standard packages need separately provisioned PostgreSQL. Application-update packages preserve an existing Codifix runtime; they do not migrate Memory Layer. Homebrew builds the checksum-pinned v3.0.0 release source. |
| Windows | Unsigned x86_64 ZIP, per-user MSI and server-tools MSI are preview artifacts; the MSI is build 2 with MSI version 3.0.2. The VC++ x64 runtime is a separate prerequisite. No complete bundled PostgreSQL/PDS/Pi stack or full native parity is claimed. Windows ARM64 and 32-bit systems are not release targets. |

Use the [release asset inventory](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0)
and its `SHA256SUMS` and individual `.sha256` files. Checksums establish content
consistency; they do not provide publisher identity verification. Signing a
Git commit is different from notarizing or signing an installer.

## Feature boundaries

| Surface | What to expect |
| --- | --- |
| Areas | Operator-configured routing chooses one exact instance; a failed selected service does not fall back into another area. See [Areas and projects](https://codifix.dev/docs/areas). |
| Private sharing | Reviewed publications and grants determine what is shared. Revocation cannot recall bytes already delivered. See [Federation](https://codifix.dev/docs/federation). |
| Expertise | Separate, explicitly authorized questions may leave the local instance. Optional model synthesis needs its own configured policy; ordinary queries do not automatically ask another area. |
| Offline knowledge | Only configured, synchronized subscriptions are available locally. Pending contributions are not accepted owner versions; observed sync time is not current remote authorization. See [Offline and synchronization](https://codifix.dev/docs/offline). |
| Background jobs | Curation commits before optional enrichment finishes. Inspect failures and confirm retries; uncertain provider attempts can be billed again. See [Jobs](https://codifix.dev/docs/reference/cli/jobs). |
| Supervisor | Policy, budget and action inspection exist. Optional execution depends on configured adapters and admission; a Pi control probe is not a deployed, isolated production agent. See [Supervisor](https://codifix.dev/docs/reference/cli/supervisor). |
| Code graph | The browser uses a 3D WebGL renderer where available and a flat 2D canvas fallback otherwise. Graph coverage depends on extraction and the selected repository. |
| Demo | The interactive website demo uses static sample data and does not establish local service, auth or database health. |
| Evaluations | Fixture, rehearsal and historical benchmark results are bounded evidence. They do not establish a new paid R3 result, ROI or universal coding improvement. |

The [dependency security evidence](https://github.com/3vilM33pl3/codifix/blob/main/docs/reviews/security-dependencies-20261017.md)
retains the unpatched upstream RSA advisory and the lockfile's yanked-dependency
warning. The inspected OIDC path verifies public keys rather than performing
the affected private-key operation; the Rust audit is not described as clean.

## Historical Memory Layer releases

Keep original release artifacts, manifests, signatures and migration history
unchanged. The [Memory Layer v2.0.1 schema repair](https://github.com/3vilM33pl3/codifix/releases/tag/v2.0.1)
restored migrations 33–52 missing from some v2.0.0 artifacts. Repair those
installations with the matching historical release procedure; never delete
migration-ledger rows or mix `memory` and `codifix` executables as an upgrade.

For current installation, start with [Install](https://codifix.dev/docs/install),
[Team setup](https://codifix.dev/docs/team-setup), or
[Operations](https://codifix.dev/docs/operations).
