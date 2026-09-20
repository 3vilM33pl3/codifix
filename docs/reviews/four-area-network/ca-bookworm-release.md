# CA-compatible release preparation

Status: off-CA candidates built and verified, 2026-09-20. No CA installation,
writer cutover, signed release or live Pi activation has occurred.

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
- [x] Implement a pinned Debian 12 ARM64 build path for Memory and the existing
      `0.5.1+memory.no-tools.1` Pi patch, preserving upstream source/lock/license.
- [x] Build candidates, inspect complete native ABI dependencies and execute
      isolated compatibility tests; label emulation separately from native proof.
- [x] Implement and test selected-instance installation/rollback preparation in
      disposable environments without touching any host service or database.
- [x] Prepare versioned console instructions, artifact hashes, health/rollback
      checks and a redacted offline-data reconciliation checklist.
- [x] Record test evidence and remaining gates; checkpoint results in dev Memory.

The approved-plan checkpoint is task `d80052bc-9128-4e6e-9ec0-4c4fc0698ecc`,
plan `b1d6d75d-07b4-4d84-9acd-88d40180cee1`, thread key
`ca-compatible-release-preparation-and-rehearsal`, at source `312d7c77`.

Completion verification succeeded with **6/6 preparation items**. The completed
plan version `f06400cf-da15-496b-a8e5-e9fcde3398b4` was archived and implementation
memory `1ec74def-f0d4-41a5-8935-e3ccbf74573e` was curated from capture
`c31bcd3d-519a-4c6f-bfe4-5581c1f6ba62`. This closes only the scoped off-CA
preparation, not the outstanding deployment gates below.

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

## Preserved candidate and source identity

Private build/evidence root:
`/data/tmp/memory-ca-bookworm-20260920.5G9rywjc`.
The new transfer directory is `ca-candidate-handoff/` under that root, **not**
the older four-area snapshot's `ca-handoff/`. Start with its `README.md`,
`PROVENANCE.json`, `CA-BOOKWORM-CANDIDATE.md` and `SHA256SUMS`.

Memory binaries and Web assets use source
`95ec76a028bd6e77439ef5d9d6178abbbbfeb368`. All 2,185 tracked regular source files
were checked against its retained archive. The only build-tree difference is
`packaging/build-deb.sh` from `d8d390fa756cde35e9c2763ae5961ac8ce83801b`, whose
exact patch is included. No Rust source or lockfile differs. The package contains
the source commit's documentation; the adjacent handoff carries newer guidance.

Pi uses `7c018475d54d4e09c907691e47a520f1f09e3288` and the existing two-site
`no-tools-1.patch`, with unchanged Cargo.lock and complete license. All 21,584
tracked regular files were checked; only the approved `src/main.rs` differs.
This does not change the existing production or diagnostic artifact pins.

| Candidate | SHA-256 | Verification |
| --- | --- | --- |
| `memory-layer_2.1.0-rc.1_arm64.deb` | `08e2daf1a90c38d905ad04b39610a3c40038307669654b23af16cbb316ddd8cb` | Built, extracted without installation; four actual packaged ELF files audited |
| Packaged `memory` | `897c368547e608062fdcd050902fc0c8906c47da6b928989f115c617c35ab119` | Maximum required GLIBC symbol 2.34; emulated version/help passed |
| `memory-2.1.0-rc.1-bookworm-arm64-tree.tar` | `2365ef9901db0dca47775b5186dd9bf7ca62584bf75e0d338cff8cfff5552154` | USTAR archive; exact packaged Memory/Web bytes; unsigned format check passed |
| `pi-0.5.1-memory.no-tools.1-bookworm-arm64` | `9d4a45f620ea789529b4f441bba5270ab61aa8fc2dc52761f7fbef875e7e2eca` | Maximum required GLIBC symbol 2.35; three emulated fake-provider checks passed |

The area, import and gateway binaries also require at most GLIBC 2.34; their
individual hashes are in `evidence/package-abi.json`. The ABI audit resolves
the complete needed-library closure, strong imported symbols and version sets
against the captured Debian 12 sysroot. It deliberately rejects the older
glibc-2.43 Pi artifact.

The package declares `libc6:arm64 (>= 2.36)`, `libgcc-s1:arm64 (>= 4.2)`,
`libstdc++6:arm64 (>= 12.2.0-14)`, Python 3 and GPG. Actual CA availability and
native behavior still need verification; do not silently install dependencies.

## Build and test evidence

Builder image:
`sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`.
The [recipe](../../../packaging/ca-bookworm/README.md) pins Rust and Node image
digests. Recorded Debian packages provide glibc 2.36, cross GCC 12.2, Clang 14
and QEMU 7.2. Memory uses Rust 1.98.1; Pi uses its pinned 2026-08-31 nightly.
APT repositories are signed but not historical snapshot-pinned: retaining the
actual image is necessary for exact toolchain replay, and bit-identical rebuilds
are not claimed.

Compilation, packaging and checks ran without network access, non-root, with a
read-only container root, dropped capabilities, no new privileges and resource
limits. They had no production database, host config, provider key or Docker
socket. Downloads/toolchain preparation did not involve paid model calls.

| Check | Result and boundary |
| --- | --- |
| Memory full ARM64 build; area/import/gateway builds; Debian package | Passed; completion scripts generated by a host-native lean CLI |
| Static ABI tests in Bookworm | 30 passed, including two real cross-compiled C/C++ fixtures |
| Package byte checks | All four extracted executables passed the static audit |
| ARM64 userspace smoke | Packaged Memory version, federation/jobs/TUI help, area help and offline schema-plan passed; schema-plan reports head 60, not a database observation |
| Pi synthetic RPC execution | Success/default, success/explicit-off and HTTP 429 passed; zero tools, one fake request per case, children reaped; no real model |
| Python checks | Build helper 4; Pi smoke harness 21; release verifier 17; instance preparation/rehearsal 20; launcher 3; CA preflight 13 passed |
| Bookworm signed-fixture checks | Release verifier 17 and instance preparation 20 passed with disposable keys, no skips |
| Rust Pi adapter | 15 ordinary tests passed; 5 explicitly environment-bound diagnostics ignored; strict Clippy passed |
| Web | 145 tests in 30 files passed; production build passed; existing large-chunk warning remains |
| Repository checks | Formatting, boundary checks, shell syntax and diff whitespace checks passed |
| Debian lifecycle tests | Passed, including cross metadata fallback, rejection of missing/malformed metadata and preserved instance boundaries |

The host-only ABI suite skips its two optional cross fixtures; the captured
Bookworm run executes all 30. CI wiring was added, but a remote CI run is not
claimed. Full workspace/database, native CA, live federation and actual customer
workflows were not rerun for these packaging-only changes.

### Failures retained, not hidden

- Pi's first offline build raced the cache copy; after cache preparation finished,
  the same source/lock/toolchain built successfully with network still disabled.
- Initial Pi smoke rejected an upstream `provider_error` RPC diagnostic containing
  two identical top-level `provider` fields. Only that exact diagnostic is now
  allowed by the test harness; request/state identity and other duplicate keys
  still fail. Pi and the production adapter were not changed for this issue.
- Bookworm dpkg lacks `dpkg-shlibdeps --package`; the single-package build uses
  its unambiguous portable default. Bookworm's cross C++ metadata has no versioned
  symbols entry, so the package uses its exact GCC `Built-Using` version as a
  conservative runtime floor. Missing/non-exact metadata still fails closed.
- Pi's pre-existing duplicate-target and dependency future-compatibility warnings
  remain in the logs. No unrelated dependency update was made.

## What is ready, and what remains

The unsigned compatibility bundle, immutable-identity checks, inactive release
preparation and **filesystem-only** switch/rollback rehearsal are ready for
review. The rehearsal uses synthetic signed releases, not a fabricated older or
second-architecture build of this candidate. Its outputs are private create-only
trees, not protection against an owning user's edits or a host administrator.

Not delivered or authorized: the host-privileged selected-instance installer,
durable recovery after power loss/SIGKILL, transactional credential switching,
operator signing, native CA sandbox proof, actual database compatibility,
offline-data reconciliation, final snapshot/delta/import, original writer fencing,
Trust activation or Pi runtime admission. Those are explicit next gates in the
[candidate console guide](../../../deploy/areas/CA-BOOKWORM-CANDIDATE.md).
Do not run this package's hooks as a shortcut to them.
