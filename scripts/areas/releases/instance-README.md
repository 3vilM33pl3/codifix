# Inactive selected-instance preparation and filesystem rehearsal

`instance.py` prepares a **concrete, inactive release tree** for exactly one named
instance. It reuses [release.py](README.md)'s signed manifest verification and
private staging, then checks and unpacks one signed release-tree archive. It
also rehearses a forward filesystem switch and rollback between two such
preparations in a new disposable directory.

This is **not a privileged installer**. It does not install a Debian package,
run package hooks or artifacts, access configuration/credentials, stop a service,
contact a database, apply migrations, switch the live `/opt/...` release or
`/etc/...` credentials, or activate Pi. Writes are confined to new private
children of the explicitly selected output parent; the operator chooses that
parent, and privileged callers are not barred from choosing one under `/opt`
or `/etc`.
It does not prove host ABI compatibility, systemd enforcement, CA isolation or
database rollback. Keep existing CA, OpenBao and Memory services untouched.

## Release input contract

The separately authenticated `memory-release-v1` manifest must list an `archive`
for the selected architecture. Its tar or gzip-tar contents are narrowly scoped:

```text
bin/memory
web/index.html
web/assets/...
```

Both the Memory binary and index must be nonempty. All remaining files belong
under `web/`. Directory entries are optional. Absolute paths, traversal,
noncanonical paths, duplicate/case-colliding names, links, devices, sparse files,
special permission bits and installer hooks are rejected. The tool imposes
8,192-member and 2-GiB expanded limits. It never calls general-purpose archive
extraction. Each regular file is streamed into an exclusively created file
through directory descriptors, without following symlinks.
PAX/GNU extension records are rejected before their metadata bodies are read;
use ordinary ustar members (gzip compression is supported). The helper targets
Linux with Python 3.11 or later, including Debian 12.

The release archive and its manifest must be prepared by the release owner.
An unsigned build candidate is insufficient. The full signing fingerprint is
supplied independently, never trusted merely because it accompanies the bundle.
Existing operator keys are not generated, loaded or used for signing here.

## Prepare one instance

Use explicit absolute paths, a caller-owned `0700` staging parent, an observed
schema from the confirmed target binding, and an independently trusted signer.
These variable values are operator inputs, not defaults or deployment approval:

```bash
python3 -I -S scripts/areas/releases/instance.py prepare \
  --manifest "$MEMORY_RELEASE_INPUT/manifest.json" \
  --signature "$MEMORY_RELEASE_INPUT/manifest.sig" \
  --public-key "$MEMORY_RELEASE_INPUT/signing-public-key.asc" \
  --trusted-signing-fingerprint "$MEMORY_RELEASE_SIGNER" \
  --architecture arm64 \
  --schema-current "$MEMORY_OBSERVED_SCHEMA" \
  --artifacts-dir "$MEMORY_RELEASE_INPUT" \
  --staging-parent "$MEMORY_PRIVATE_STAGING_PARENT" \
  --instance trust \
  --release-archive "$MEMORY_SIGNED_RELEASE_ARCHIVE_NAME"
```

The existing release verifier authenticates the manifest and copies all signed
artifacts for the selected architecture. The additional tool materializes only
the explicitly selected release-tree archive; Debian packages remain opaque and
unexecuted. `instance-preparation.json` binds the exact instance, manifest hash,
signer, architecture, schema interval and every binary/web file's size and hash.
The schema input is labeled `schema_current_asserted`, with
`database_schema_verified: false`: this offline tool does not establish the
actual database schema, even when the caller supplies a previously observed value.
It contains an **unexecuted review plan** naming only
`memory-layer@trust.service`, its concrete release path and digest-credential
path, with explicit stop/fence/switch/health/rollback gates.

Success is `inactive_preparation_only`, with `installation_authorized: false`.
Repeated preparation creates another random private output; it never edits a
previous output. Executable bytes become owner-read/execute (`0500`); web files
become owner-read (`0400`). Directories stay caller-private (`0700`). These are
create-only tool outputs with verification, **not filesystem immutable flags**
or protection from the owning user. A later privileged installer must authenticate
again under its own trust boundary. Failure may retain a partial private stage;
without a successfully reverified preparation record it is not a usable release.

## Rehearse a reversible filesystem switch

Prepare two independently authenticated, increasing RC versions for the **same
instance and architecture**. Both must declare the same single schema equal to
the observed schema, with no migrations. This stricter rehearsal gate deliberately
does not claim that moving an old binary back reverses a database migration.
Trust/permission classifications remain visible and still need separate review.

```bash
python3 -I -S scripts/areas/releases/instance.py rehearse \
  --previous "$MEMORY_PREVIOUS_PREPARATION" \
  --prepared "$MEMORY_CANDIDATE_PREPARATION" \
  --previous-trusted-signing-fingerprint "$MEMORY_PREVIOUS_SIGNER" \
  --trusted-signing-fingerprint "$MEMORY_RELEASE_SIGNER" \
  --schema-current "$MEMORY_OBSERVED_SCHEMA" \
  --instance trust \
  --output-parent "$MEMORY_PRIVATE_REHEARSAL_PARENT"
```

The tool reauthenticates both manifests, derives the expected inventory from
their signed archives, and verifies complete prepared trees. Changed metadata,
extra files/directories, symlink or hardlink substitution, changed permissions,
wrong instance, wrong schema or version downgrade fail closed. It then creates a
new random private root, copies both signed trees and performs:

1. `trust/release` → `trust/previous`; `trust/incoming` → `trust/release`.
2. Verify candidate and retained prior bytes.
3. `trust/release` → `trust/retained-candidate`;
   `trust/previous` → `trust/release`.
4. Verify both trees and that prepared sources are unchanged.

No prepared source is moved or replaced. Only the new disposable root changes.
A caught interruption between the two renames restores the prior directory when
its identity and the absent destination can still be verified; unexpected path
replacement is preserved and rejected, never overwritten. **Power-loss, SIGKILL
recovery, host writer fencing and transactional credential switching are not
implemented or established by this rehearsal.** A real installer needs a durable
recovery journal and independent schema revalidation before activation/rollback.
Directory moves use Linux `renameat2(RENAME_NOREPLACE)`; missing support fails
closed rather than falling back to check-then-rename overwrite behavior.

Success is `filesystem_switch_and_rollback_verified`, still with
`installation_authorized: false`. The new root contains a scope record and final
`result.json`; both retained releases remain available. No successful result is
written after a failed or interrupted check. Preserve failures for inspection;
the tool intentionally performs no broad recursive cleanup.

## Offline verification

```bash
python3 scripts/areas/releases/test_instance.py
python3 scripts/areas/releases/test_release.py
```

Tests sign only synthetic, nonexecutable fixtures using disposable GPG keys in
an isolated keyring. They cover signature/hash failures, full binary/web binding,
archive traversal and special files, concrete path requirements, immutable-output
behavior, changed input refusal, schema/version gates, preserved prior releases,
reversible switching and an injected interruption. No paid provider, real data,
operator key, systemd action or CA access is needed. If GPG is missing the crypto
tests explicitly skip; a skip is not authentication evidence.
