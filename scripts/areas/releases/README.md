# Local release manifests and authenticated staging

`release.py` creates a deterministic, unsigned `memory-release-v1` JSON manifest
for `2.1.0-rc.<positive number>` releases. It can also authenticate a detached GPG
signature and stage the selected Linux architecture's local files. It never
downloads, extracts archives, executes artifacts, installs a package, uses sudo,
controls a service, reads a live private key, or grants installation permission.
Python 3 is required; signature verification also requires GnuPG.

## Manifest contract

All fields are required; unknown fields and duplicate JSON keys are rejected.
Canonical bytes use sorted JSON keys, compact separators, ASCII escaping and a
single trailing newline. Artifacts are sorted by basename; no timestamp or local
source path enters the signed manifest. Identical inputs produce identical bytes.

| Field | Meaning |
| --- | --- |
| `format` | Exactly `memory-release-v1` |
| `release_version` | Exactly a `2.1.0-rc.N` version, without a leading `v` |
| `source_revision` | Full lowercase 40- or 64-digit Git object ID |
| `platform` | Exactly `linux` |
| `schema` | Integer `floor` and `head`, defining the supported database interval |
| `upgrade` | `classification`, plus boolean `migrations`, `trust_changes`, `permission_changes` |
| `pi` | Pi Rust `version` exactly `0.5.1`, reviewed official `source_url` and exact `source_revision`, nullable `source_sha256`, and optional architecture-to-digest entries in `artifact_sha256` |
| `artifacts` | 2–64 entries covering both `amd64` and `arm64`; each has `name`, `architecture`, `kind`, `size_bytes`, `sha256` |

Artifact kinds are `binary`, `debian-package` and `archive`. Names are safe ASCII
basenames, with case-insensitive duplicates, traversal, separators and reserved
evidence filenames rejected. Each file must be nonempty and at most 16 GiB.

The reviewed Pi v0.5.1 source pin is
`7c018475d54d4e09c907691e47a520f1f09e3288` in
`Dicklesworthstone/pi_agent_rust`. `source_url` must be exactly its
[official commit](https://github.com/Dicklesworthstone/pi_agent_rust/commit/7c018475d54d4e09c907691e47a520f1f09e3288)
or its
[source archive](https://github.com/Dicklesworthstone/pi_agent_rust/archive/7c018475d54d4e09c907691e47a520f1f09e3288.tar.gz).
Another repository, revision or floating tag is rejected even with a valid
release signature.

`maintenance` requires equal schema floor/head and all three change flags false.
Schema transitions must declare `migrations: true` and use `approval-required`.
Any trust or permission change requires that approval classification. Staging
accepts only a current schema inside the signed interval and never interprets a
newer local schema as permission to downgrade.

These fields are signed publisher assertions. This tool does not prove the build
came from the declared commit, inspect binary/package architecture internally,
validate migration code, establish Pi sandbox safety, or establish that an upgrade
is behaviorally harmless. The source/build/compatibility checks remain separate
release gates. Optional Pi digests are recorded pins, not proof that Pi artifacts
were supplied or verified; absent hashes block later Pi activation.

## Create an unsigned manifest

Use reviewed, already-built local artifacts and real full revisions. The variable
names below deliberately require operator-provided values; there is no default
revision, key, source repository, artifact download or `latest` lookup.

```bash
python3 scripts/areas/releases/release.py create \
  --version 2.1.0-rc.1 \
  --source-revision "$MEMORY_RELEASE_REVISION" \
  --schema-floor 52 --schema-head 52 \
  --classification approval-required --trust-changes --permission-changes \
  --pi-source-url "$PI_REVIEWED_SOURCE_URL" \
  --pi-source-revision "$PI_REVIEWED_SOURCE_REVISION" \
  --pi-source-sha256 "$PI_REVIEWED_SOURCE_SHA256" \
  --artifact "amd64:debian-package:$RELEASE_INPUT_DIR/memory-layer_2.1.0-rc.1_amd64.deb" \
  --artifact "arm64:debian-package:$RELEASE_INPUT_DIR/memory-layer_2.1.0-rc.1_arm64.deb" \
  --output "$RELEASE_INPUT_DIR/manifest.json"
```

All filesystem arguments must be absolute paths through non-symlink directories;
input files must be ordinary, non-symlink files. The output parent must already
exist. An existing manifest is never overwritten. Paths are opened by directory
descriptor with `O_NOFOLLOW`, including ancestor directories, so a symlink cannot
redirect traversal. Source artifact bytes are measured locally; they are not
executed. Real schema numbers must come from the reviewed release, not the
example values above. The initial four-area deployment changes trust and permission
boundaries and is therefore **not** an unattended maintenance release. A schema
transition additionally declares `--migrations` with its actual schema bounds.
Only a separately reviewed, unchanged-schema maintenance update with no trust or
permission changes may use `--classification maintenance`.

To include independently checked Pi runtime pins, repeat
`--pi-artifact-sha256 amd64:<sha256>` and
`--pi-artifact-sha256 arm64:<sha256>`. The source archive hash is optional in the
format, but should be supplied when that archive is part of the reviewed release.

## Operator signing, separate from this tool

The operator reviews the exact manifest and artifacts, then signs with their
existing approved signing key. No key is generated or selected by `release.py`.
These commands belong to the operator's explicit release-signing action; they
are not run during manifest generation or verification:

```bash
gpg --batch --no-tty --local-user "${MEMORY_RELEASE_SIGNING_FINGERPRINT}!" \
  --digest-algo SHA256 \
  --output "$RELEASE_INPUT_DIR/manifest.sig" \
  --detach-sign "$RELEASE_INPUT_DIR/manifest.json"
gpg --batch --no-tty --armor \
  --output "$RELEASE_INPUT_DIR/signing-public-key.asc" \
  --export "$MEMORY_RELEASE_PRIMARY_FINGERPRINT"
```

Use the full uppercase fingerprint of the actual signing key or signing subkey.
The `!` suffix on `--local-user` makes that selection exact. Do not pass `--yes`
to overwrite existing signature evidence. Export only a public key, never secret
keys. Hardware-key or passphrase prompts are an operator interaction; the staging
tool does not request them. The public-key file must contain one armored public
primary key and its subkeys.

Deliver the trusted full **signing** fingerprint through an independent trusted
channel. A fingerprint inside an untrusted manifest, fetched beside an artifact,
or copied from the supplied public key is not independent trust establishment.
The primary fingerprint is not interchangeable with a signing-subkey fingerprint.

## Verify and stage on the selected host

Transfer the manifest, signature, public key and selected architecture's artifacts
using an already approved channel. Supply the current schema from an independent
local inventory. The staging parent must already exist, be owned by the invoking
user, and have exactly mode `0700`; shared/group-writable directories are rejected,
including sticky temporary directories. Use a dedicated private directory. This
command needs no root privileges:

```bash
python3 scripts/areas/releases/release.py verify-stage \
  --manifest "$RELEASE_INPUT_DIR/manifest.json" \
  --signature "$RELEASE_INPUT_DIR/manifest.sig" \
  --public-key "$RELEASE_INPUT_DIR/signing-public-key.asc" \
  --trusted-signing-fingerprint "$MEMORY_RELEASE_SIGNING_FINGERPRINT" \
  --architecture arm64 --schema-current 52 \
  --artifacts-dir "$RELEASE_INPUT_DIR" \
  --staging-parent "$RELEASE_STAGING_PARENT"
```

Use `amd64` for Monolith or `arm64` for the CA only after confirming the actual
host architecture. The explicit architecture is checked against the manifest's
selected entries; another architecture is never a fallback. All manifest entries
are structurally validated, but only the selected architecture's bytes are hashed
and staged. The report explicitly records this coverage limitation.

Verification uses a fresh mode-0700 temporary keyring, imports only the supplied
public key, and disables GPG configuration, inherited keyrings, automatic key
retrieval/import and agent startup. It requires a single binary detached
signature using SHA-2, an exact full signing fingerprint, and no expired/revoked
signature status. Private-key armor is rejected before reading its body.

Only after signature, fingerprint and schema checks does the tool create a fresh
random mode-0700 staging directory. Each selected artifact is copied once into a
new mode-0600 file while checking exact size and SHA-256. No archive is unpacked
and no executable bit is enabled. Existing files are not overwritten. A failure
removes only exact files created by this invocation whose recorded inode identity
still matches. The opened stage must match the newly created directory identity;
cleanup additionally verifies the parent/name binding without following symlinks.
Unexpected/replaced files or directories are preserved, even when that leaves a
failed staging directory for diagnosis. Cleanup never scans or recursively
removes untracked entries. Successful staging
retains `manifest.json`, `manifest.sig`, `signing-public-key.asc` and
`verification.json` alongside the selected artifacts. Existing successful staging
directories remain untouched on repeat runs.

The result is `authenticated_and_staged` with `installation_authorized: false`.
This is **not** a privileged installer or a standing maintenance approval. The
selected-instance opt-in, fresh backup/restore evidence, verified compatibility,
work fencing, CA restrictions, trust/permission review and installer authorization
remain mandatory. The verifier does not change the running release or provide a
rollback. Stage files remain owned by the invoking user; a future privileged
installer must reverify bytes under its own trust and file-access boundary.

For an additional signed release-tree archive, the separate
[selected-instance preparation and filesystem rehearsal](instance-README.md)
binds the complete Memory binary/Web tree and tests a reversible directory switch
inside a fresh disposable root. It still does not install, fence services,
switch credentials, verify a live database or implement migration rollback.

## Offline tests

```bash
python3 scripts/areas/releases/test_release.py
```

Manifest/path tests use temporary files. When GPG is available, signature tests
generate a disposable certification key and signing subkey in a fresh temporary
keyring, sign only synthetic artifacts, then terminate that fixture agent and
remove its keyring. They never use the operator's live keyring. Crypto tests
explicitly skip when GPG is unavailable; a skip is not signature-validation
evidence. Tests cover exact subkey identity, unsigned and tampered inputs,
wrong/unsupported architecture and schema, unrecognized fields, duplicate names,
traversal, symlinks, immutable outputs and private staging permissions.
