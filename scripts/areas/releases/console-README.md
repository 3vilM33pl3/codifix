# Console first install: inactive, recoverable and instance-scoped

`console.py` is a root-only **first-install** tool. It authenticates a prepared
release again, snapshots its inputs, and places the executable/Web tree and the
matching configuration/digest at the named-instance paths. It never invokes a
package hook, Codifix binary, model, database command or mutating systemd command.
It does not create `activation-reviewed`. An installed result remains inactive.

This is not an upgrade tool. Existing instance directories, runtime/state/cache
remnants, active units, pending jobs, PIDs or activation markers are refusals.
The original singleton (`codifix.service`) is neither stopped nor replaced.
On the CA it continues using the shared database until a separately approved
writer cutover. Step CA and OpenBao are never targeted.

The current unsigned ARM64 candidate is **not** a valid input to installation.
Do not sign it with the disposable keys used in tests. Complete a coherent
release for both architectures and authenticate the operator's real signature
before proceeding. Pi is not installed or activated by this tool.

## Before using this on a host

These are future operator steps, not authorization to change the CA now:

1. Independently verify the release, installer and signer identity. Obtain the
   **full signing-subkey fingerprint** through a trusted channel. The public key
   accompanying a bundle is not its own trust anchor.
2. Install the independently authenticated Python tools as root-owned mode-0644
   files at `/usr/libexec/codifix/releases/{console,instance,release}.py`.
   Their directory and ancestors must be root-owned and not group/world writable.
   Future Debian builds package these files; the earlier preserved ARM64
   compatibility candidate does not contain them. Do not overwrite that artifact.
3. Use [instance.py](instance-README.md) to prepare the signed release in a
   root-owned mode-0700 directory with protected ancestors. The installer copies
   and authenticates a separate recovery snapshot. Do not `sudo` an arbitrary
   untrusted working-tree script or use caller-writable Python modules.
4. Prepare a separate root-owned, mode-0600 configuration, based on
   [the staging template](../../../deploy/areas/instance.toml.example). Review its
   exact bytes locally and record its SHA-256; do not paste credentials into chat
   or shell arguments. Supply a distinct API token, explicit database credential,
   database name and loopback port. The tool neither generates credentials nor
   proves that the chosen database/role is independent.
5. The closed initial configuration requires read-only service mode, exact
   `/opt/codifix/instances/<name>/release/web`, a loopback PostgreSQL URL and
   disabled watchers/automation, background work, federation, embeddings, models,
   reinforcement, offline replay, MCP and telemetry. Additional TOML fields are
   rejected. Remote-DB/gateway egress and CA-specific unit overrides are separate
   reviewed steps after this inactive installation, not workarounds for a refusal.
6. A loaded named unit must be the exact reviewed, disabled template without
   drop-ins, aliases or transient properties; a genuinely missing named unit is
   also allowed. Its inactive/no-job/no-PID state is checked repeatedly. The
   first-install template hash is pinned in the tool. New templates need review
   and tests, not a user-supplied hash override. Systemd must be available.

Do not remove a real activation marker or move existing state aside just to make
this tool accept a previously used name. Such a host needs an upgrade/recovery
procedure outside this first-install contract. It does not protect against
another root administrator deliberately changing the host concurrently.

## Install only the selected inactive instance

After the prerequisites and explicit host-change approval, replace each variable
below with the exact reviewed value. Do not use the shared database credential.
`CODIFIX_REVIEWED_CONFIG_SHA256` identifies configuration bytes, not a secret key.

```sh
sudo env -i PATH=/usr/bin:/bin /usr/bin/python3 -I -S \
  /usr/libexec/codifix/releases/console.py install \
  --instance trust \
  --prepared "$CODIFIX_ROOT_OWNED_PREPARATION" \
  --trusted-signing-fingerprint "$CODIFIX_TRUSTED_SIGNING_SUBKEY" \
  --schema-current "$CODIFIX_REVIEWED_SCHEMA" \
  --config "$CODIFIX_ROOT_OWNED_CONFIG" \
  --config-sha256 "$CODIFIX_REVIEWED_CONFIG_SHA256" \
  --database-name "$CODIFIX_TRUST_DATABASE_NAME"
```

The schema argument must exactly match the prepared assertion and the signed
compatibility interval. It is **still an assertion**: this tool performs no
database observation or migration. It checks the native ELF architecture header,
not the complete native runtime ABI or the CA sandbox.

Fixed paths (no CLI root or command override exists):

| Purpose | Path |
| --- | --- |
| Binary and Web files | `/opt/codifix/instances/trust/release/` |
| Configuration and binary digest | `/etc/codifix-instances/trust/` |
| Private recovery snapshot and metadata journal | `/var/lib/codifix-installations/trust/` |

The independent roots avoid legacy service-owned parents. They do not migrate
the earlier unsigned candidate's layout. Any per-instance configuration,
state/cache/runtime or journal under that older layout causes a refusal; preserve
it for explicit review. Never chown the singleton's directories as a workaround.

Published release directories are `0755`, the binary `0755`, and Web files `0644`
for the service's DynamicUser. Configuration directories remain `0700` and files
`0600`. No symlink-based current-release switch is introduced. The digest file
matches the exact installed executable. The API/DB credentials are only in the
protected configuration and recovery snapshot; reports/events contain hashes,
identities and state, never their contents.

## Inspect and recover

Retain the fingerprint independently: recovery reauthenticates the saved release
instead of treating a local success receipt as signature authority.

```sh
sudo env -i PATH=/usr/bin:/bin /usr/bin/python3 -I -S \
  /usr/libexec/codifix/releases/console.py inspect \
  --instance trust --trusted-signing-fingerprint "$CODIFIX_TRUSTED_SIGNING_SUBKEY"

sudo env -i PATH=/usr/bin:/bin /usr/bin/python3 -I -S \
  /usr/libexec/codifix/releases/console.py recover \
  --instance trust --trusted-signing-fingerprint "$CODIFIX_TRUSTED_SIGNING_SUBKEY" \
  --action finish
```

The journal publishes complete fsynced events, chained to the immutable plan.
Each tree is complete and fsynced before its publication intent. Renames refuse
existing destinations. A death between rename and acknowledgement is reconciled
only if the source is absent and the destination has the expected inode,
complete byte inventory, ownership and permissions. Both paths present, both
absent, changed files, links, journal gaps or new activation require intervention.
Per-instance `flock` excludes competing installer/recovery commands.

An interrupted copy is never reused as complete: a new bounded generation is
made and its partial predecessor retained (at most five copy generations).
Temporary event files are not valid
events. A crash **before** `plan.json` commits cannot have published installation
paths; the incomplete private input snapshot is preserved for manual inspection,
not automatically cleaned up. Journal capacity is bounded; repeated unexplained
failures require intervention. These fsync/rename assumptions apply to normal
local Linux filesystems, not arbitrary network filesystems or failing hardware.

To retain an installed but never activated first installation:

```sh
sudo env -i PATH=/usr/bin:/bin /usr/bin/python3 -I -S \
  /usr/libexec/codifix/releases/console.py recover \
  --instance trust --trusted-signing-fingerprint "$CODIFIX_TRUSTED_SIGNING_SUBKEY" \
  --action retain
```

This moves only the two authenticated instance trees to adjacent
`.retained-trust-<transaction>` names. No data is deleted and the private recovery
snapshot remains. An ambiguous forward publication is completed **inactive**
before retention. Interrupted retention is resumable; completed retention cannot
be converted back into installation. No previous executable, credential or schema
is restored: this is first-install recovery, **not upgrade or database rollback**.

## Activation remains separate

Do not start Codifix just because installation returns `installed_inactive`.
Service startup can migrate/bootstrap a database even with read-only application
mode. Native CA ABI and secret-path/network denials, independent DB provisioning,
offline reconciliation, backup/restore, delta review, original writer fencing and
explicit activation approval are still required. Apply the reviewed CA-specific
denials before activation and verify Step CA/OpenBao separately. Pi admission,
federation enrollment, model egress and budgets remain separate.

## Verification

```sh
python3 scripts/areas/releases/test_console.py
python3 scripts/areas/releases/test_instance.py
python3 scripts/areas/releases/test_release.py
bash packaging/tests/test-debian-lifecycle.sh
```

For the actual root-only CLI, explicitly select an already-present, independently
reviewed Python 3.11+/GPG container image by its full ID:

```sh
python3 -I -S scripts/areas/releases/test_console_cli.py \
  --image "$CODIFIX_REVIEWED_LOCAL_IMAGE_ID" \
  --evidence-parent /data/tmp
```

This does not pull an image. It creates a uniquely named, network-disabled,
read-only-root container with no capabilities, only individual read-only fixture
mounts and disposable tmpfs installation paths. A five-minute limit bounds the
rehearsal; cleanup targets its exact recorded container ID. The private evidence
directory retains input hashes, invocation, CLI output and cleanup result. The
tested image ID and results are in the
[execution ledger](../../../docs/reviews/four-area-network/ca-console-installation.md).

Unit tests use synthetic signed headers and injected service-state observations,
not working application binaries. The separate root-container CLI rehearsal has
no host configuration or service sockets and uses a fake systemctl. Neither test
form is a native CA sandbox or real-systemd activation test.
