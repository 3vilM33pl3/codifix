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

Status: provisioning in progress; passphrase entry is operator-owned.

- Confirm exactly one certification key and one signing subkey, public
  fingerprints, algorithm, usage and expiration.
- Verify protected storage without emitting private material.
- Export only the public key to `public/`.
- Sign and verify an explicitly non-release self-test, not `manifest.json`.
- Encrypt a bounded recovery archive containing the protected private-key files,
  public keyring, revocation certificate and dedicated agent configuration.
- Decrypt only into a checking pipeline and verify archive contents against the
  source identities; do not extract decrypted secrets into a temporary directory.
- Retain a separate offline/off-device copy after Olivier chooses its destination.
  A second directory on Monolith is not protection from loss of Monolith.

## Operation after setup

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
