# rc.2 coherent Bookworm build evidence

Observed 2026-09-29. This records **unsigned build verification**, not deployment
completion or native CA execution.

## Frozen inputs

- Source: `bf78efa20478881b5eff15713a0daa5f2b6d7cb4`.
- Source archive SHA-256:
  `93dbd4c1a8a7bfbd194e01a0ef56085b8432c8d6cc30df4f64bc1f37ca092bab`.
- Immutable Bookworm builder:
  `sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`.
- Workspace:
  `/home/olivier/.cache/memory-coherent-rc2-bookworm-final-20260929.lO3yRhc7`.
- Unsigned bundle: `release-review/` within that private workspace.

AMD64 and ARM64 packages were built serially, network-disabled, non-root, with
read-only container roots and only a dedicated build workspace mounted. Public
Cargo/npm caches and compiled intermediates were copied after the earlier build
stopped; no credentials or user configuration were copied. Earlier superseded
source archives, artifacts and logs remain preserved. Pi was not rebuilt or
changed. Existing PDS/Atrium pins were retained.

## Outputs

| Artifact | SHA-256 |
| --- | --- |
| `memory-layer_2.1.0-rc.2_amd64.deb` | `6abf83bcbe78ffc417896a659c289ec091ad93392135634514665662a7814aad` |
| `memory-layer_2.1.0-rc.2_arm64.deb` | `5661028a832c87d447a075864932be54684b8aa70be609af99a8b70552afa03f` |
| `memory-layer_2.1.0-rc.2_amd64.tree.tar` | `c014e32c1fe89e86c6ea1798cbff6467c27b6037382545d452d024103010a078` |
| `memory-layer_2.1.0-rc.2_arm64.tree.tar` | `c337b34aa54c3e16f355c73ad7972642b83b6aef1e98ae6b918b5f11ab03ca27` |

The four-artifact `manifest.preview.json` has SHA-256
`ddf565ada67a88cba17b28394137761ac22574871f89c30fd3fc3fbb925efb51`.
Its compatibility interval is schema60..61, `approval-required`, with migration,
trust and permission changes declared. It is **not the signing input**: the final
deployment manifest must also bind the final encrypted Trust reconciliation
archive and procedure. No final `manifest.json`, signature or signing key was
created by this build task. No older executable may start against schema61.

## Verified

- Both Debian packages completed with matching rc.2 version/architecture.
- Frozen tracked source bytes and executable modes match the Git archive after
  building. Generated TypeScript build metadata was preserved separately under
  `evidence/`; only `web/node_modules` and `web/dist` remain build directories.
- All four packaged executables per architecture passed static Debian12/glibc2.36
  ABI closure checks against the preserved reviewed sysroots.
- Credential-free, network-disabled smoke passed for version, federation/jobs/
  TUI help, area help, and schema-plan (head61 and exact migration digest).
  ARM64 smoke uses QEMU; it is not a native CA test.
- ABI parser tests: **67 passed** including the synthetic ARM64 compiler fixture.
- Frozen Web tests: **149 passed** across31 files; both production builds passed.
  Vite reports the existing large GraphTab chunk warning.
- Bundled tools, units, docs and skills match frozen source. All shared payload
  bytes, including complete Web assets and completions, match across architectures.
- Both five-member USTAR binary/Web trees passed the frozen instance parser and
  rebuilt byte-identically. This establishes deterministic assembly, not an
  independent reproducible-compilation claim.
- Canonical manifest generation is invariant to artifact order. Missing
  signatures are rejected without staging. All46 preliminary checksum entries
  match and verification leaves the unsigned review unchanged.

The detailed reports, exact commands/recipes and build logs are in `evidence/`
and the bundle. `evidence/unsigned-review-verified.json` is the final review check;
`release-review/SHA256SUMS` covers the preliminary unsigned bundle only. Final
deployment additions require their own complete checksum/manifest verification.

## Remaining boundaries

This evidence does not establish operator signing, native CA sandbox/runtime,
database reconciliation, installed launcher routing, live federation, writer
fencing, or cutover completion. The source license/notice files are preserved in
the bundle; standalone Debian license packaging remains a redistribution caveat.
Existing Pi runtime hashes/grants remain independently managed and are not
replaced by the source-pin-only manifest entry. No paid calls, service changes,
database connections or live deployment occurred in the build/smoke task.
