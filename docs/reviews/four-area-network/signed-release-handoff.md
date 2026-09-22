# Authenticated internal release handoff — 2026-09-22

Status: **signed; both architectures authenticated and staged offline**. Olivier explicitly approved
signing the exact reviewed manifest, not installation, public publication or CA
changes. Dev Memory task checkpoint `305d6378-1be6-4138-b1d6-99b5e66b9bdb`,
task memory `e2d5cfcf-7b47-441e-822f-22bca2290628`, starting at `f9152a4c`.

The preserved [unsigned review](coherent-release-preparation.md) remains unchanged.
Its exact manifest SHA-256 is
`5d06127ec4db38e0d56eb0942a41f26bee32987645f1bf81391fb929e19ee1d4`;
the complete 53-entry inventory verified before signing. A new private work
directory is `/data/tmp/memory-signed-release-20260922.0Ov3K6`.

Use only the [dedicated key](release-signing-setup.md), signing-subkey fingerprint
`4EB55288032C60EF65C73A0352EE81AD6D98F325`, with direct desktop pinentry.
The public-key export is bound to SHA-256
`a00d7f6f5e1ac2664da6e956237c871440df7932e31ed29c0af19b071a94f76c`.
No private key, backup or signing-agent socket belongs in the handoff or verifier.

Verification uses the frozen release verifier in the pinned, network-disabled
Debian 12 container and exercises both architectures separately. Schema 60 is a
declared offline compatibility test input, not an observation of the CA database.
Tampered manifests, changed artifact bytes and incorrect fingerprints must fail.

The signature authenticates the manifest and its four artifact hashes; it does
not independently authenticate adjacent source, notices, instructions or test
reports, nor prove their assertions. Installation stays explicitly unauthorized.
Native CA checks, independently confirmed trust, CA-local Trust database,
offline-data reconciliation and approved writer cutover remain separate gates.
No CA, existing service, USB recovery copy, public release, paid evaluation or Pi
activation is changed by this signing-only operation.

## Signing and verification results

GPG created one 119-byte binary detached Ed25519/SHA-256 signature with the exact
approved signing subkey. Signature SHA-256:
`8f1efa484c661533b1f271690dcfa95d83a3de263835accfcaabf5d22692d215`.
The manifest remains byte-for-byte identical to the reviewed file. The passphrase
was entered directly in desktop pinentry, not supplied to the assistant. The
dedicated agent was stopped after signing and its socket confirmed absent;
the default keyring and agent were not changed.

The frozen verifier `669eb1a491286a1296e5f10d543771497371744005f0257f79d441e846319af7`
passes **10 exact-input checks** in image
`sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`:

- AMD64 and ARM64 each authenticate the real signature and stage their two
  artifacts. Staged bytes, sizes, identities and authentication files rehash
  correctly. Both reports retain `installation_authorized: false`.
- An unrelated fingerprint and the certification-primary fingerprint are both
  rejected; the exact signing subkey is required.
- Altered canonical manifest and detached-signature bytes fail authentication.
- Schema assertions 59 and 61 are rejected against this release's interval 60–60.
- A same-size altered package for each architecture is rejected. Every negative
  case leaves its staging parent empty, including failures after a partial copy.

The container is non-root, network-none, read-only-root, capability-free and
resource-bounded. Only public handoff inputs are mounted read-only; a new scratch
directory is writable. No private keys, GPG agent sockets, host configuration,
live database or service socket is mounted. No packaged binary is executed by
these checks. Inputs and all four artifact hashes still match afterward.
This supplements the earlier native AMD64 and emulated ARM64 smoke evidence;
it is not a native CA test or an independent third-party review.

## Handoff and next action

New private handoff:

```text
/data/tmp/memory-signed-release-20260922.0Ov3K6/handoff
```

Start with `START-HERE.md`. It supplies a verification-only console sequence,
the public signing fingerprint, explicit schema-test semantics and stop points.
The handoff contains the real manifest/signature/public key, both Debian packages
and tree archives, full source and notices, frozen tools and prior/current
verification evidence. `signing-evidence/` contains the current reproduction
scripts, copy receipt, public signing receipt and ten-case verification report.
All **55 handoff checksum entries pass**, covering 270,658,563 total bytes
including the inventory itself. Top-level `SHA256SUMS` SHA-256:
`de6339d324bff0a2abd9ff4f1aa697692b7e10aed69d2417baad21b7662517fb`.
Verification-report SHA-256:
`305eadd59dacef489642627d52fa83902728a9e035e7e30d42f00934d1358e92`.
Finalization receipt: `../evidence/final-inventory.json`. All original review
files and the original inventory were rechecked and remain unchanged.
The prior unsigned status and inventory are retained under `historical-review/`;
those checksums describe the original directory, not this rearranged handoff.
Duplicate unpacked binary/Web trees are omitted; their bytes remain inside the
signed tree archives. All 53 original review entries were verified and accounted
for, with 43 copied or matched and ten redundant unpacked files omitted.

The original unsigned bundle is not overwritten. No transfer onto the CA or USB
has occurred. Keep the signing recovery USB disconnected and use a separately
approved release-transfer method. First obtain read-only native console facts
and authenticate/stage the selected architecture; do not install, start services
or query a live database as an incidental verification step.

The package payload does not install repository-root licence/notice files; full
notices/source are adjacent here. Keep the internal handoff complete. This is
not public or standalone redistribution clearance. Pi binaries remain absent.
R3, native CA sandbox checks, full data recovery and all activation/cutover gates
remain open. No Rust behavior changed; full workspace/DB/Lean/Web suites were not
rerun for this signing-only operation.
