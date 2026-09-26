# Trust inactive installation — approval-ready CA instruction

**Status: authorized for one bounded inactive installation.** Olivier gave
action-time approval on 26 September 2026 at 11:43 UTC. The executable Notion
instruction is `monolith-ca-20260926-trust-inactive-install-v1`. This approval
covers the credential and database-role changes described below and does not
authorize activation.

## Purpose and exact effects

Perform one bounded first installation of the new `trust` instance from the
already authenticated private preparation. The operation will:

- generate one fresh CA-local PostgreSQL password and one fresh CA-local API
  token;
- set only the password of the isolated `memory_trust` role in PostgreSQL
  `17/trust`;
- write one closed, read-only configuration into a new protected local input
  directory;
- use the reviewed `console.py` transaction to publish the prepared release,
  configuration and recovery journal to the three new named-instance paths;
- leave `memory-layer@trust.service` disabled, inactive and without an
  activation marker.

The credentials never leave the CA and must not appear in Notion, logs, command
arguments, shell history, reports or process listings. A password or token loss
before installation is a fail-closed stop: generate a fresh pair and set the
role password again rather than trying to recover disclosed bytes.

## Bound identities

Require all of these exact identities before mutation:

| Item | Required identity |
| --- | --- |
| Preparation directory ID | `memory-release-bb46bffe4c653f8a7214c4a596062fd1` under the existing private preparations root |
| Preparation record | `a6b358e3967bbc0117e0fb1d3066008f09b7280738b264acdb2a6111cfd683b8` |
| Release manifest | `5d06127ec4db38e0d56eb0942a41f26bee32987645f1bf81391fb929e19ee1d4` |
| Signing subkey | `4EB55288032C60EF65C73A0352EE81AD6D98F325` |
| Signed release archive | `memory-layer_2.1.0-rc.1_arm64.tree.tar`, SHA-256 `489ec3b5456b2a28464d5456cf2068867d1107cd019475e2783298486c575d1a` |
| Prepared executable | `897c368547e608062fdcd050902fc0c8906c47da6b928989f115c617c35ab119` |
| `console.py` | `3e5ed1ad489951fd91fd0047d2c4441d33fe233df9a6f3ddd90ad8e79c526675` |
| `instance.py` | `44d4849bf730b9eab3e69ef65a7345170ba6f4e110456f70bf588f2c6f43b63f` |
| `release.py` | `669eb1a491286a1296e5f10d543771497371744005f0257f79d441e846319af7` |
| Named-unit template | `418e629337592d207f11115226395015c11aa01223841692771b9e12f4db1dca` |
| Database restore receipt | `0e4840874dc80beb50344dc5663c54d5aef10232c9ef290be1eac14222b3a431` |

Require PostgreSQL `17/main` online on port 5432 and `17/trust` stopped with a
manual start policy on port 5433. Require the existing Memory service, Step CA
and OpenBao healthy and unchanged. Require `memory-layer@trust.service`
disabled/inactive, with no process, pending job, drop-in, alias or transient
property. Stop if any of these paths exists; do not remove or repair it:

- `/opt/memory-layer/instances/trust`
- `/etc/memory-layer-instances/trust`
- `/var/lib/memory-layer-installations/trust`

## Credential and configuration transaction

Create a new root-owned mode-0700 input directory on the local filesystem with
root-only mode-0600 files. Generate independent 64-character lowercase
hexadecimal secrets using the kernel CSPRNG: one PostgreSQL role password and
one API token. Do not reuse any singleton, Monolith, release-signing, OpenBao,
Step CA, encryption-recipient or operator credential.

Start only PostgreSQL `17/trust` long enough to set the password for the
existing unprivileged `memory_trust` role over its local Unix socket. Do not
change role attributes, database ownership, schema, data, listening addresses,
HBA policy, extensions or the `17/main` cluster. Verify without printing secret
bytes that:

- `memory_trust` remains a login role without superuser, createdb, createrole,
  replication or bypass-RLS authority;
- it still owns database `memory_trust`;
- the restored row/history/version/projection counts and restore receipt remain
  unchanged.

Stop `17/trust` again and require the original stopped/manual state before
installation.

Create the exact closed configuration below using the new local secrets. Do not
post or otherwise export its bytes:

```toml
[service]
bind_addr = "127.0.0.1:8091"
api_token = "<fresh CA-local API token>"
read_only = true
web_root = "/opt/memory-layer/instances/trust/release/web"

[database]
url = "postgresql://memory_trust:<fresh CA-local DB password>@127.0.0.1:5433/memory_trust"

[cluster]
enabled = false

[offline]
enabled = false

[reinforcement]
enabled = false

[provenance]
reverify_enabled = false

[mcp]
enabled = false
http_enabled = false
require_token = true
read_only = true

[features]
llm_curation = false

[background_work]
enabled = false
internal_automation = false

[automation]
enabled = false

[embeddings]
enabled = false
create_enabled = false

[consolidation]
enabled = false

[procedural]
enabled = false

[telemetry]
enabled = false
```

Hash the exact protected configuration locally. The hash is reportable; the
configuration, database URL and secrets are not.

## Inactive console installation

Run the installed root-owned `console.py` with Python isolated/no-site mode and
an empty environment, using:

- command `install`;
- instance `trust`;
- the exact retained preparation above;
- trusted signing fingerprint
  `4EB55288032C60EF65C73A0352EE81AD6D98F325`;
- schema assertion `60`;
- the protected configuration and its locally calculated SHA-256;
- database name `memory_trust`.

Require result format `memory-console-install-result-v1`, status
`installed_inactive`, `activation_authorized: false`,
`service_mutated: false`, `database_contacted: false`,
`database_schema_verified: false` and
`native_abi_or_sandbox_proven: false`. Run `console.py inspect` against the
installed journal and require the same immutable plan, release and
configuration identities.

If the installer is interrupted after committing `plan.json`, inspect the
journal and use only its authenticated `recover --action finish` path to
complete this same inactive transaction. Stop for any changed bytes, unexpected
path, identity mismatch, missing journal intent or ambiguous database state. Do
not delete or move anything to force acceptance.

After completion require:

- the exact executable/Web tree at
  `/opt/memory-layer/instances/trust/release/`;
- root-only configuration and digest at
  `/etc/memory-layer-instances/trust/`;
- the protected immutable plan, saved inputs and journal at
  `/var/lib/memory-layer-installations/trust/`;
- `memory-layer@trust.service` still disabled/inactive with no processes;
- no `/etc/memory-layer-instances/trust/activation-reviewed` file;
- PostgreSQL `17/trust` stopped/manual;
- existing Memory, PostgreSQL `17/main`, Step CA and OpenBao unchanged.

Do not run the Memory executable, start or enable the named service, create the
activation marker, change PostgreSQL networking, contact a model, enroll in
federation, install Pi, alter DNS/OS/USB state, or touch the shared Monolith
database.

## One sanitized result

Post one `RESULT` or `BLOCKED` comment to the established Notion coordination
page. Include the message/reply IDs, timestamps, bound preparation/release/tool
identities, configuration SHA-256 and size, installation transaction/plan
SHA-256, final path ownership/modes, unit and cluster states, unchanged protected
services, preserved data counts/restore receipt, recovery action if any, and
deviations. Never include secret values, the database URL, configuration bytes,
private paths containing random components, private Memory content or full
logs.

Passing this step proves only a recoverable inactive filesystem installation
with locally provisioned credentials. Native execution, the service sandbox,
database connectivity, loopback exposure, activation, writer fencing and
federation remain separate.

## CA result and corrected retry

At 12:11 UTC on 26 September the CA agent reported `STOPPED_SAFE`. Credential
and database-role provisioning completed, but `console.py` rejected the retained
preparation before creating `plan.json` because its ancestor
`/var/lib/memory-layer` belongs to the running singleton service user. No live
installation path was created. The agent correctly left the singleton ownership
unchanged and did not weaken the installer.

The retained closed configuration is 812 bytes with SHA-256
`7a7cc020a38a44146d0276d7c713ad4e59b8fd23502837c3e52c953558188f00`.
The role remains least-privileged, the restored counts and receipt are
unchanged, PostgreSQL `17/trust` is stopped/manual, and the existing Memory,
PostgreSQL `17/main`, Step CA and OpenBao services remain protected.

Instruction `monolith-ca-20260926-trust-inactive-install-v2` corrects only the
input trust boundary. It reconstructs the preparation from the same signed
release inputs below a new root-owned top-level preparation parent whose entire
ancestry passes `console.py` checks. It also ensures the retained configuration
is read from an equivalently root-trusted private path, copying the exact 812
bytes locally if necessary and requiring the same SHA-256. It does not
regenerate credentials or change the database role again. The new preparation
must derive the same preparation-record SHA-256 and executable/Web inventory
before the original inactive installer is retried. The service and activation
exclusions above remain unchanged.
