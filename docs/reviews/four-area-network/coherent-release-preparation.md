# Coherent offline release preparation — 2026-09-22

Direct continuation checkpoint: task `4f34448a-5750-4075-a07c-839e68ad54ea`,
memory `1746ffa3-7278-49ef-9853-f9358ea00d63`, starting at `99684ba4`.

Prepare a single-source AMD64/ARM64 internal Memory release containing the new
inactive console tools. Reuse the captured Bookworm image and public dependency
caches, but preserve all previous artifacts and verify every new input/output.
Build and test offline. Assemble exact binary/Web archives, Debian packages,
source and provenance for **operator signing review**, without selecting or using
a real signing key. Do not sign the candidate with test credentials. Pi is not
rebuilt, admitted or redistributed by this slice; existing pins remain intact.

No CA installation, credentials, service restart, database provisioning/import,
source-writer cutover, provider calls or R3 runs. The approved topology remains a
Trust database **local to the CA**, not a remote Monolith database; the previous
console guide's remote-DB discussion described an unapproved alternative.

Required evidence: both packages from one clean commit, all packaged executable
and Web digests, source verification, dependency/ABI checks, isolated native AMD64
and explicitly emulated ARM64 smoke checks, offline schema-plan agreement,
deterministic archive/manifest validation and fail-closed unsigned verification.
Full application/data recovery and native CA sandbox checks remain independent.

Pre-build review found an integration defect: the legacy package owns
`/etc/memory-layer` and `/var/lib/memory-layer` as its service user, while the
privileged console correctly requires trusted root-owned ancestors. New named
instances now need independent `/etc/memory-layer-instances` and
`/var/{lib,cache}/memory-layer-instances`, `/run/memory-layer-instances`, plus the
root-private `/var/lib/memory-layer-installations` journal. Keep `/opt` release
paths unchanged. Do not chown legacy directories or adopt existing older-layout
instances. Add a real-root container regression using service-owned legacy roots
before accepting this corrected candidate.

The correction passes 26 signed console/unit tests and 10 real-root CLI tests.
The latter uses service-owned uid/gid 65534, mode-0750 legacy directory fixtures,
no capabilities and only read-traversal supplementary group access. All legacy
bytes, modes, owners and inode identities remain unchanged. Five older-layout
instance/journal locations refuse install/inspection/recovery without adoption.
This is fixture evidence, not execution of package hooks against the real host.
Evidence: `/data/tmp/memory-console-cli-f0i1j1x9/`; 35 exact fake-systemctl read-only
queries, no activation markers. Corrected console SHA-256:
`3e5ed1ad489951fd91fd0047d2c4441d33fe233df9a6f3ddd90ad8e79c526675`.
Reviewed unit SHA-256:
`418e629337592d207f11115226395015c11aa01223841692771b9e12f4db1dca`.
The 20 prepared-instance tests, three launcher tests, Debian lifecycle fixtures,
seven build-command tests, formatting and boundaries also pass.

Frozen package source: `b35faf2983d20c69bc086fb9bcb7e95c51bd8f47` (2,203 tracked
files verified), archive SHA-256
`c6ce8327a1dea7f6988750cdc327ab40573938b696cf42ca358a068e9428d42e`.
Work/evidence root: `/data/tmp/memory-coherent-bookworm-20260922.I23H901g`.
The subsequent ABI-verifier extension is a separately identified verification
tool, not an unrecorded edit to that frozen build source. Its 57 tests all pass
in the pinned Bookworm image, including two real cross-compiled C/C++ fixtures.
Verifier SHA-256: `fe0ba65120d54cf42762d57b0ddf5fc1dcfeaa7afb81e525efa636ba171854e1`.

## Completed builds and runtime checks

Both packages completed offline from that frozen source using builder image
`sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`:
Debian 12/glibc 2.36, Rust 1.98.1, GCC 12.2, Node 22.19 and npm 10.9.3.
The private build source still matches all 2,203 archive files after compilation.
Generated TypeScript build metadata was retained separately, not mistaken for
source. Previously verified artifacts remain unchanged.

| Package | Bytes | SHA-256 |
| --- | ---: | --- |
| `memory-layer_2.1.0-rc.1_amd64.deb` | 28,314,764 | `d59c1c570769d1299b5e726b61a70aee8c3cc1e85d310a130205ea7c06566b30` |
| `memory-layer_2.1.0-rc.1_arm64.deb` | 24,710,960 | `34894b000cd1d9e71890d1415e3ab7ac5e6c47f06f7494b48b9e1283a17ab71a` |

Native AMD64 and explicitly QEMU-emulated ARM64 checks each pass six bounded
commands: CLI version, federation/jobs/TUI help, area-planner help and the offline
import schema plan. No service, database or provider was used. Both schema plans
report 60 migrations, head 60, digest
`2ce8b07f9c8fc252b0162d158ccb8e57082c2186306cdc489c9343ec91a115dc`.
They bind all four packaged executable hashes, although the gateway executable
itself is statically audited rather than started by the smoke test.

Web verification passes **145 tests across 30 files** in a separate offline
Bookworm container; both package Web builds pass. The existing large-JavaScript-
chunk warning remains an optimization issue. Cached npm audit output is not a
fresh vulnerability scan. Formatting, boundary checks and `git diff --check`
also pass; full Rust/database/Lean suites were not rerun for this packaging slice.

## Final audit and unsigned assembly

The first AMD64 closure audit correctly stopped on an unrecognized
`GLIBC_ABI_DT_RELR` requirement in the captured Bookworm `libm`. This is an empty
libc capability node introduced in glibc 2.36, not a numeric symbol version.
The [upstream implementation](https://sourceware.org/pipermail/glibc-cvs/2022q2/078142.html)
and captured libc definitions establish its meaning. Commit `649bdf52` adds exact
feature/provider validation, with no exception to strong-symbol resolution or
unknown-feature rejection. All **67 verifier tests** pass in the pinned offline
builder, including both real ARM64 compiler fixtures. Actual dependency closure
audits of all four executables now pass for **both** architectures.
The failed audit and pre-fix checker copies remain under `evidence/pre-relr-*`.

Final verification-tool SHA-256:
`c24580ef902b047607ec1976f713ca528b236d90c99da1e0989f81d632e6e8e4`.
The checker is separately versioned; it did not change the frozen source or
finished binaries. Exact test invocation/evidence is retained at
`/data/tmp/memory-abi-relr-20260922.dyzqfN/verification.md`.

The complete private review directory is:

```text
/data/tmp/memory-coherent-bookworm-20260922.I23H901g/release-review
```

It contains both packages, both deterministic binary/Web tree archives, the
canonical unsigned manifest, full source archive and notices, frozen installer
tools, per-file inventories, build/audit/smoke/Web/console-CLI evidence and
`SHA256SUMS`. Pi binaries, credentials, datasets and signatures are absent.

| Review identity | SHA-256 |
| --- | --- |
| `manifest.json` | `5d06127ec4db38e0d56eb0942a41f26bee32987645f1bf81391fb929e19ee1d4` |
| `SHA256SUMS` | `1d17cee510459632d010270603a6e30de8af995f692e204da64e8aa47ccc0367` |
| AMD64 tree archive | `a7e23ed0c3c03bdce8a7d09fd1a5bda76c5b6375f1abc2cfb0ea51f93e7a5cd3` |
| ARM64 tree archive | `489ec3b5456b2a28464d5456cf2068867d1107cd019475e2783298486c575d1a` |

Verification checks all **53 files in each Debian payload**, including all four
ELF headers and 42 exact source-bound tools/unit/docs/skill files; Web assets and
all other architecture-independent payload files match between packages. Each
tree contains exactly the Memory binary and four Web files. Reassembling both
trees gives identical bytes. Reversing manifest artifact order gives identical
canonical bytes. Both architectures reject absent authentication without creating
a staging directory. All **53 review checksum entries** pass; the complete review
directory is unchanged after verification.

The independent final result is
`../evidence/review-verification.json` relative to the review directory, SHA-256
`0c8bf79927da4afccaa68a294688d7534d19bc94b76795fc39007890635727be`.
Its reproduction script is included at `evidence/verify-review.py`; the one-off
scripts intentionally bind this exact private work root. Reproduction requires
the retained inputs and does not fetch anything. Deterministic assembly is not
a claim of independently reproducible compilation.

## Next operator action and remaining gates

**Status: this unsigned, off-CA release-preparation slice is complete.** No
publication, signature, native CA test, installation or activation has occurred.

1. Review `REVIEW-ONLY.txt`, `provenance.json`, the exact manifest and checksums.
   Identify an approved release-signing key using only its full public signing
   fingerprint. Do not send private keys, passphrases or unrelated configuration.
2. Separately approve signing this exact manifest with that exact key. This work
   neither selected a key nor accessed a real signing keyring. Never substitute
   test keys. Transfer the trusted signing-subkey fingerprint independently of
   the bundle, then authenticate the signature before preparing installation.
3. Before any CA change, complete native compatibility and service-sandbox
   checks, **CA-local** independent Trust database/schema preparation, offline
   inventory reconciliation, refreshed backup/restore and delta review. This
   candidate admits only already-prepared schema **60**; it does not authorize an
   upgrade of the existing shared database. Preserve all original writers until
   their exact fencing/cutover window is approved.
4. Use the [inactive first-install guide](../../../scripts/areas/releases/console-README.md)
   only after its host-change approval. Installation still does not start Memory;
   activation, federation enrollment and Pi admission have separate gates.

The Debian packages do not install the repository-root license/notice files.
Those complete texts and the full source are preserved beside them in this
internal review bundle; standalone/public redistribution review remains open.
No license clearance, production readiness or customer-release gate is claimed.
The September 20 candidate/handoff remains preserved and checksum-verified;
existing previews, CA services, credentials and source writers were not changed.
R3 remains paused.

Dev Memory completion capture: `b6e37328-a0fd-47ea-a3bb-1ab35cf1c3b2`, curation
operation `cf8527db-c170-4af3-950f-d0d263fc96a7`, implementation memories
`bb0d1069-c852-47a9-8286-711265d568f0` and
`07105b24-5a03-4989-bff8-d32882ff1933`. The workflow returned four queued
background jobs and three proposals; neither job completion nor proposal approval
is claimed. These are repository-workflow records, not source-dataset cutover.
