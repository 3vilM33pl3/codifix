# Dedicated release-signing key setup — 2026-09-22

Authorization: Olivier asked to set up the dedicated release key after confirming
he did not have one. This authorizes key provisioning and a harmless key self-test,
not signing the candidate manifest, installing on the CA, changing trust/enrollment,
publishing a key or enabling automatic release signing.

Dev Memory task checkpoint: `2e468d92-3794-4281-b94a-19b20f369216`, task memory
`6f59c15b-2c09-4b22-880e-1a5cc728b96d`, starting at `69bd5e41`.

## Local boundaries

- Dedicated directory: `/home/olivier/.local/share/memory-release-signing`, mode
  `0700`, outside the repository, existing `.gnupg`, CA and ATProto identities.
- `gnupg/`: protected private keyring, public keyring and automatic revocation
  certificate. Never transfer this directory as an installation input.
- `public/`: public-key export and public fingerprints only. Public-key possession
  does not establish trust; independently confirm the exact signing fingerprint.
- `backups/`: encrypted local recovery copy, not an off-device backup guarantee.
- `evidence/`: sanitized creation/verification receipts and harmless self-test
  material. Never log passphrases, decrypted backup contents or private packets.

GnuPG 2.4.8 and graphical pinentry are already installed. Create an Ed25519
certification-only primary key with a two-year expiry and a separate Ed25519
signing-only subkey with a one-year expiry. No encryption/authentication subkey,
external account, public keyserver registration or CA certificate is needed.
User ID: `Memory Layer Release Signing` (no invented email address).

Passphrases are entered directly through the desktop pinentry, never chat,
arguments, environment variables, shell input, loopback mode or assistant tools.
The dedicated agent enforces at least 16 characters, disallows bypass and external
credential caching, and ignores its cache when signing. Normal cache lifetime is
60 seconds, capped at 120; smartcard probing is disabled. The existing GPG agent
and its configuration are not changed.

Use absolute `/usr/bin/env`, `/usr/bin/gpg` and `/usr/bin/gpgconf` paths. A local
`env` executable precedes the system utility in this session and silently did
not execute the initial checks/creation attempt. No key was created by those
attempts; the system executable was verified before proceeding. Do not modify
that unrelated user executable.

## Completion checks

Status: **local key setup, encrypted-backup verification and USB copy complete**.
The USB copy is verified and flushed but still mounted; safely eject it and keep
it disconnected. All passphrases were supplied through GPG pinentry, not through
the assistant or chat.

| Purpose | Public fingerprint | Expiry (UTC) |
| --- | --- | --- |
| Certification primary | `42ABF2BE5894AE6D5F0E2C4EB4E578E7F4606EB5` | 2028-09-21 15:27:30 |
| Release-signing subkey | `4EB55288032C60EF65C73A0352EE81AD6D98F325` | 2027-09-22 15:29:19 |

Public export: `public/release-signing-public-key.asc`, SHA-256
`a00d7f6f5e1ac2664da6e956237c871440df7932e31ed29c0af19b071a94f76c`.
The public listing confirms one Ed25519 certification-only primary and exactly
one Ed25519 signing-only subkey. The dedicated agent's `KEYINFO` reports `P`
(passphrase-protected) for both; private files are owned by Olivier with mode
`0600`, inside mode-`0700` directories. No private-key packets were exported.

A harmless, deliberately non-release message was signed with the exact subkey.
The existing frozen release verifier accepts it cryptographically in the pinned
Debian 12 image, with no network, host private keys, agent sockets, capabilities
or writable host mounts. Altered text and another fingerprint are rejected;
the message itself is also rejected as a release manifest. Evidence is
`evidence/selftest-bookworm.json`, with the reproduction script beside it.
The tested image is
`sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`.
This is algorithm/verifier compatibility evidence, not native CA execution or
an independently established trust relationship with an installation target.

Encrypted backup: `backups/memory-release-signing-20260922.tar.gpg`, 20,559 bytes,
SHA-256 `b6506b8b7ebce39afda1556f2f6d03eb7bc9a7e0c6848ac82f95fac32d12bb3d`.
The AES-256 encrypted tar contains only the dedicated agent configuration,
public/trust keyrings, the two protected private-key files, automatic revocation
certificate and public export/fingerprint record. The keyring was quiescent
during capture. Encryption and a fresh-passphrase decryption both succeeded;
`GOODMDC` and `DECRYPTION_OKAY` are retained in
`evidence/backup-decryption.status`. Decrypted bytes flowed directly into
`tar --compare`, which returned success against the source files; no decrypted
archive or restored private files were written to disk. This checks recovery
bytes, not restoration of a lost machine or import into another GPG version.

The original unsigned release manifest and `SHA256SUMS` remain unchanged:
`5d06127ec4db38e0d56eb0942a41f26bee32987645f1bf81391fb929e19ee1d4`
and `1d17cee510459632d010270603a6e30de8af995f692e204da64e8aa47ccc0367`.
At completion of this key-setup slice no release signature existed, no public
key was distributed or installed, and existing GPG, CA, ATProto and service
identities were left untouched. The subsequently approved
[release-signing handoff](signed-release-handoff.md) records the first real
manifest signature separately; it does not install trust on the CA.

After verification the dedicated agent was stopped via its explicit `--homedir`;
its socket was confirmed absent. No existing/default agent was stopped. Verification
receipt: `evidence/setup-verification.json`, SHA-256
`1d2c557e1988d5a89c099a1f2e0f0c30b0405c6e8958886c7f8d7c2501babc2b`.
Dev Memory completion capture `f25598af-7ae5-4ecf-aa23-17a94d3e3d4d`, curation
operation `247de3b5-ee7c-4b1f-bab8-dcae99f9f7e5`, stores public metadata only.
It returned three memories, three unapproved proposals and four queued jobs;
background completion or proposal approval is not claimed.

- Confirm exactly one certification key and one signing subkey, public
  fingerprints, algorithm, usage and expiration.
- Verify protected storage without emitting private material.
- Export only the public key to `public/`.
- Sign and verify an explicitly non-release self-test, not `manifest.json`.
- Encrypt a bounded recovery archive containing the protected private-key files,
  public keyring, revocation certificate and dedicated agent configuration.
- Decrypt only into a checking pipeline and verify archive contents against the
  source identities; do not extract decrypted secrets into a temporary directory.
- The separate USB copy is now verified; safely eject it and store it independently.
  A second directory on Monolith alone is not protection from loss of Monolith.

## USB recovery copy — 2026-09-22

Olivier connected a USB key and asked to copy the recovery materials there.
Task checkpoint `d2a660d5-1a0e-4c6c-a2f6-312a7472cb4c`, task memory
`3539b1c8-a2e9-4736-b092-9b29e2df2f39`, starting at `eb2cfa42`.
The only USB block device was a removable 58.2-GiB `STORAGE DEVICE`, containing
the mounted FAT filesystem **BIOS UPGRAD**, UUID `66F0-75AD` (`/dev/sdc1`, 8:33 at
copy time). Re-identify by live evidence for future work; device names may change.

New directory (no existing directory or file was overwritten):

```text
/run/media/olivier/BIOS UPGRAD/Memory-release-signing-backup-2026-09-22
```

Exactly five files were written: the encrypted recovery archive, public-key
export, public `fingerprints.txt`, portable `RECOVERY.md` and `SHA256SUMS`.
No plaintext private-key files, live keyring, standalone revocation certificate,
passphrases, Memory dataset or release artifacts were copied.

Source archive/public-key/fingerprint hashes were checked against the setup
record before copying. Mount source, UUID, filesystem and device identity were
checked before and after; destination operations used pinned directory handles
and exclusive creates. All files were fsynced, the filesystem was flushed, and
all five files read back byte-for-byte. The four `SHA256SUMS` entries pass an
additional on-USB check. Existing root-entry inode/mode/size/mtime metadata is
unchanged. FAT permissions are mount-wide: encryption, not POSIX file modes,
protects the private contents on this drive.

USB `SHA256SUMS` digest:
`db832205c22a3fe5a09e57f4a8125e7d87f59e056bfdfcb9ad43b3874d54ce56`.
The encrypted archive retains the original verified digest
`b6506b8b7ebce39afda1556f2f6d03eb7bc9a7e0c6848ac82f95fac32d12bb3d`.
Local transfer receipt: `evidence/usb-copy-20260922.json`, SHA-256
`0badfe426cda82a2027e77ab46d13e96aa4d3864e3f245e5d5c721be526defae`;
create-only reproduction recipe: `evidence/copy-usb-20260922.py`. Both are relative
to the dedicated local signing directory. The recipe intentionally refuses a
rerun into an existing target and must not be reused against a changed device.

The drive was not formatted, unmounted or powered off. This is verified off-device
storage, not yet disconnected/offline storage, nor a new restore exercise.
Safely eject it through the desktop and keep it physically secure. No release
was signed and no CA, service, credential or trust configuration was changed.

Dev Memory USB completion capture: `2e41a74d-6895-413b-a4c4-642326c6c9e7`,
curation operation `e2677403-8ea6-4954-9c06-e060d7164827`. The helper returned
five memories, no replacement proposals and four queued background jobs; job
completion is not implied by the successful transfer record.

## Operation after setup

The immediate operator action is to safely eject the verified USB copy and keep
it disconnected. Retain its full SHA-256 separately and compare after any future
copy. Do not upload it or choose an
unapproved cloud/sync destination automatically. Keep the key and backup
passphrases in the password manager, and ensure that manager is itself recoverable.

To recover later, first verify the encrypted archive hash. Work only in a fresh
mode-`0700` directory on a trusted offline machine, never extract over an existing
GPG home. Decrypt the authenticated archive into that protected recovery directory,
verify the recorded public fingerprints and restrictive permissions, review the
pinentry path for that machine, and run a non-release sign/verify check before
using the recovered key. Never disclose the restored private-key directory or
revocation certificate. Concrete restoration commands should be bound to that
actual recovery target; this task did not perform such a restoration.

Keep the passphrase in the operator's password manager, never in this repository.
Only the public key and independently verified signing-subkey fingerprint belong
on installation targets. One company release key can authenticate packages for
all areas without granting the areas permission to sign releases.

Signing a release requires approval of the exact manifest and signing fingerprint;
key setup is not standing signing approval. The manifest covers exact artifact
hashes, architecture, source and compatibility assertions. Signing does not prove
those assertions or authorize installation. Use the
[release signing guide](../../../scripts/areas/releases/README.md#operator-signing-separate-from-this-tool)
with an explicit `--homedir` pointing to this dedicated keyring, never an implicit
default key selection. Retain the operator's passphrase interaction.

Before expiry, renew deliberately and re-export the public key to verifiers.
After compromise, stop using the key, invoke the private revocation procedure and
distribute replacement trust explicitly; creating a revocation certificate alone
does not update disconnected verifiers. Keep the revocation certificate private:
anyone holding it can revoke the key. Do not publish it as part of normal setup.

References: [GnuPG key management](https://www.gnupg.org/documentation/manuals/gnupg/OpenPGP-Key-Management.html)
and [agent passphrase controls](https://www.gnupg.org/documentation/manuals/gnupg/Agent-Options.html).
