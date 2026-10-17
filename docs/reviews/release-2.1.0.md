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
- [x] Stable Linux manifest acceptance and RC/stable ordering with signed-fixture tests.
- [x] Packaging/workflow preparation and actual test/Clippy repairs.
- [x] Freeze one clean source commit for all builds.
- [x] Consolidated local required-database Rust and frontend verification.
- [x] Linux Bookworm-compatible packages and ABI/install verification.
- [x] Both Mac architectures, standard and runtime-preserving update packages.
- [x] Windows native ZIP/user MSI/server-tools MSI and disposable package checks.
- [x] Astra final review, exact artifact inventory and checksums.
- [x] Publish stable/latest release and verify downloaded assets.
- [x] Update both Homebrew formula entrypoints to verified published source hash.
- [x] Commit/push final evidence, verify approved plan and remember result.

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

## Preparation evidence

- Required-database `cargo test --workspace --all-targets --locked --quiet`
  passed against the isolated `memory-release-210-postgres` container on loopback
  port 55491; no production connections. Existing explicitly ignored tests remain
  ignored; this is not Windows-native acceptance.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`, formatting,
  boundary checks and the lean `tui,project-sync` CLI build pass. Test-only repairs
  use meaningful expectations and move one test module; no lint was disabled.
- Web 293 tests and production build pass; docs content, links, assets and
  89-page production build pass after stable-version metadata alignment.
- Linux operator tests include actual disposable signed RC-to-stable upgrade and
  unchanged RC compatibility. The isolated console CLI rehearsal passes 10 tests.
- Mac Python suite passes 147 tests, including 23 package contracts. These are
  software-contract tests, not native installer acceptance. Its native preservation
  rehearsal uses synthetic sentinel files, not a complete bundled-runtime restore.
- Release inventory tests reject incomplete matrices, changed source metadata
  and substituted package bytes; positive checksum generation passes.
- Web/docs audits remain zero. The raw Rust audit retains only the documented
  RSA advisory plus the known yanked-spin warning. Publication rejects new findings.

## Native candidate rehearsal

First run: [39025110663](https://github.com/3vilM33pl3/memory/actions/runs/39025110663),
source `49be5bb58b000f7bf69278a0dc8ff180d848021c`. This is **not a release pass**:

- Windows setup exposed PowerShell 7 command-line quoting of the MSVC environment
  script. Raw `ProcessStartInfo` arguments preserve the intended quoting; the
  corrected command was separately exercised in the existing Windows lab.
- Hosted Rust tests exposed a desktop-discovery fixture's dependency on a real
  `~/.codex/sessions` directory. The test now supplies its own directory without
  modifying `HOME` or relaxing production discovery. All 33 focused tests pass
  with the actual Codex directory hidden. The normal required-database workspace
  rerun passes **1,513 tests across 99 groups**, with 14 existing ignored tests.
- Both Linux architectures compiled and passed static ABI inspection. AMD64's
  runtime probe initially used Docker's non-executable `/tmp`; validation now
  uses a dedicated executable, bounded `/candidate` tmpfs. ARM64 then exposed a
  harness assumption that the positional gateway accepts `--help`. It now checks
  the exact no-config rejection, with a timeout; loader errors remain failures.
- Build caches are retained after failed, non-cancelled candidate checks. No
  failed package is promoted or relabelled; the replacement run rebuilds every
  platform from one new commit.
- Windows packages require the separately installed Visual C++ v14 x64 runtime.
  The builder records actual direct PE imports for every shipped executable;
  it does not silently bundle a runtime or claim clean-machine acceptance.

A supplemental full-suite user-namespace run is not acceptance evidence: mapping
root-owned ancestors to an unmapped UID correctly triggers protected-file checks.
The successful full-suite result above uses the normal required-database runner.

## Frozen candidate evidence

Candidate run [39027168771](https://github.com/3vilM33pl3/memory/actions/runs/39027168771)
builds every platform from `a500aa77709941385ead9c1c5e7407c957aabb71`.
Subsequent formula/evidence commits do not change that artifact source commit.

- Consolidated source/security job `117380577801` passes required-database Rust
  tests, formatting, Clippy, boundaries, lean CLI, packaging/operator regressions,
  Web/docs tests and builds, and the PDS security overlay. Raw Rust audit evidence
  retains the documented RSA advisory and yanked-spin warning.
- Linux build jobs `117380577739` (amd64) and `117380577756` (arm64) pass complete
  Bookworm ABI checks and packaged launch probes. Native installation jobs
  `117387529030` and `117387528876` pass fresh installation, synthetic 2.0.1 upgrade,
  repair and data-preserving removal on disposable hosted Ubuntu runners.
  No running application/database or production deployment is implied by those
  installer checks. Downloaded package/build/ABI checksums independently pass.
- Apple Silicon job `117380577723` passes both native package installations,
  architecture/dependency checks and sentinel-only runtime preservation. Both
  downloaded package checksums and source-bound metadata independently pass.
- Windows job `117380577747` passes native PostgreSQL 17/pgvector preparation,
  installer identity checks, packaged capture/curation/search/restart, and both
  MSI install/repair/data-preserving removal checks. Downloaded ZIP/MSI/manifest
  checksums independently pass. The manifest records build 1, MSI version 2.1.1,
  ten executable DLL inventories, unsigned status and `native_stack_bundled: false`.

- Intel Mac job `117380577821` passes native architecture/dependency checks,
  actual installation of both variants and sentinel-only runtime preservation.
  Downloaded package checksums and source-bound metadata independently pass.
- Astra's bounded final review finds no publication blocker. It independently
  confirms the tag, successful run, latest/non-prerelease status and published
  limitations; it does not substitute for native acceptance or checksum checks.

## Published result

[Memory Layer v2.1.0](https://github.com/3vilM33pl3/memory/releases/tag/v2.1.0)
was published at **2026-10-17 11:56:39 UTC**, with `draft: false` and
`prerelease: false`; the GitHub latest-release endpoint returns `v2.1.0`.
The immutable tag resolves to the frozen commit above. Every job in run
`39027168771`, including publication, succeeds.

All **43 published assets** were downloaded independently to
`/data/tmp/memory-release-210-verification.yDdFH4Ss/published`.
Aggregate `SHA256SUMS`, every individual `.sha256`, both Linux per-platform sums,
and all inventory sizes/digests pass. The inventory contains 19 payloads plus its
own checksummed JSON; ten payloads are primary packages. The source archive's
embedded Git commit matches the frozen source. This is content verification,
not a claim of publisher signing.

| Primary artifact | SHA-256 |
| --- | --- |
| `memory-layer_2.1.0_amd64.deb` | `a44c86b11e237c38c9deccd4303f5ce4fda75d462dca1eb88472ab1f4cc349b4` |
| `memory-layer_2.1.0_arm64.deb` | `fc33018d81488499e56dc53ca5b164ec1d3ca901450581e6de7dec4610e97193` |
| `memory-layer-2.1.0-macos-x86_64.pkg` | `0d9954a17e77510b82a1845f5921d12a7aec6784f924e8341dc365b1f8abb7ae` |
| `memory-layer-2.1.0-macos-x86_64-application-update.pkg` | `9b2b579cab24eee995c80096b268944214309b982a98299a6a0bf34147d8dc3d` |
| `memory-layer-2.1.0-macos-aarch64.pkg` | `cb6f1d4deb1c5f922a8fba3c8a53a8f212b86c5c58f1ba5467b6713ffee0cfdf` |
| `memory-layer-2.1.0-macos-aarch64-application-update.pkg` | `d3217f48b8fc917cdcb842900ee2a7a0de0968d527cbfea1bc5d35e7e035ce0a` |
| `memory-layer-2.1.0-windows-x86_64-build.1.zip` | `efedd5340e489b594de8516981a0cae71bf32d8a4146303f21e2e56b2a16d232` |
| `memory-layer-2.1.0-windows-x86_64-build.1.msi` | `832adbc609b096988efbe10fa274619ca1caabcf29e89388eb78092cd4f3dc04` |
| `memory-layer-2.1.0-windows-x86_64-build.1-server-tools.msi` | `cf0b2bb57a2aa0cc97088cbd8c6f25a80b2d66fc0fa00bd7ed75b62a25b014f6` |
| `memory-2.1.0.tar.gz` | `f003ddd96c6b957e6b524e4035582939a7511a14261307bf814462921bd44dd6` |

Homebrew commit `b4562ef9` updates `Formula/memory-layer.rb` with that verified
published source URL/hash. `packaging/macos/homebrew/memory-layer.rb` is a
`require_relative` forwarding entrypoint, not an independently maintained copy;
both resolve the same updated formula. Both pass Ruby syntax checks and a
stubbed, non-installing metadata-load assertion. No `brew install`, live service
restart or existing installation upgrade was performed.

The published [release instructions](../releases/2.1.0.md) retain package
prerequisites, migration/recovery guidance, unsigned/notarization status, the RSA
limitation and Windows-preview/unbundled-stack boundaries. Full Windows parity,
clean-PC runtime acceptance, paid R3 evaluation and live upgrades remain separate.

Workshop `finish-plan-execution` verifies **22/22** approved checklist items,
records and curates implementation memory `c764caaf-fe3f-416d-8493-7d6dd59cd1b0`
(capture `8468ddf7-bde7-469a-a2d8-524597f6272b`), and archives the completed plan.
The final local source/evidence tree is committed and pushed to `main`; the
release tag remains pinned to the original frozen artifact commit.
