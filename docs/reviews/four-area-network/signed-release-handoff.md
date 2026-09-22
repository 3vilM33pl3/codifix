# Authenticated internal release handoff — 2026-09-22

Status: signing and offline verification in progress. Olivier explicitly approved
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
