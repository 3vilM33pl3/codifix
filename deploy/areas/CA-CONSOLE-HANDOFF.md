# CA console handoff: preflight and release staging, not activation

Use the CA's existing local root console. Do not enable SSH, change signing or
OpenBao configuration, use sudo, or supply a provider key for this procedure.
Nothing below installs Memory/Pi, starts an area, applies a migration, reads a
credential file, rotates a credential, or changes the existing CA services.

**Current stop point:** a read-only inventory and, only when its independent
inputs are available, authenticated local release staging. There is no completed
live-validated CA migration or privileged selected-instance installer/rollback helper. A staged
package or a successful preflight is not installation permission. The package's
legacy singleton compatibility is not a completed Trust-area deployment.

## 1. Get the reviewed files to the console

An operator must first place the reviewed repository snapshot on the CA through
an already approved local transfer channel, such as the existing removable-media
procedure. The release owner must provide the exact reviewed revision and its
integrity evidence independently. This runbook does not invent a published
release URL, assume the current checkout is trusted, or fetch/run an installer.

The preflight is a standalone shell file from this snapshot and is also included
in the new candidate package. From the snapshot's repository root, the exact
read-only console command is:

```bash
bash deploy/areas/ca-preflight.sh
```

Expected: one `memory-ca-preflight-v2` JSON object with
`status: inventory_only_activation_not_approved`. It uses Python 3, a fixed
system executable search path, and a 60-second total subprocess budget. Missing
tools/permissions, denied local database access and exhausted checks are explicit
statuses, not empty-data claims. A missing Python interpreter returns a small
`unavailable/python3_missing` JSON object and exit status 1. Do not install missing
dependencies or weaken CA policy automatically; report the missing check.

Copy the resulting JSON into the review conversation. It contains OS/libc and
disk metadata, service states, port/scope metadata, local database names/sizes,
fixed aggregate table counts/schema heads and anonymous client counts. It does
not include configs, environment values, logs, application records, process
arguments/IDs, network addresses, usernames, CA keys, or secret-storage paths.
No container daemon, remote database, or model provider is contacted.

Do not paste `env`, service configuration, `/proc/*/environ`, database URLs,
passwords, application records or signing/OpenBao files as supplementary output.

## 2. Read the result as gates, not approval

| JSON field | Required interpretation |
| --- | --- |
| `host.architecture` | Native ARM64 must be `aarch64`/`arm64`. ARMv7/32-bit is not compatible with this artifact. |
| `host.libc` | Reports the current Python process's GNU libc, not another container's library set. Unknown/non-glibc requires separate review. |
| `pi_arm64.native_abi_precheck` | `blocked` on wrong architecture or glibc below 2.43. `minimum_version_matches_only` is not a launch/sandbox/dependency proof. |
| `runtime.namespaces` | Presence and kernel limits only. Zero/missing/denied limits need review; no namespace was created. |
| `runtime.cgroup` | Reports exposed unified controllers. Missing metadata may mean v1 or restriction, not absence. No delegation or resource enforcement was tested. |
| `runtime.tools` | Executable presence only. Docker/Podman access and container execution remain unverified; the preflight never opens their sockets. |
| `services` | Current singleton, Trust-template, CA, OpenBao, SSH, PostgreSQL and container-engine unit states. The command does not change them. |
| `network` | Listener port/protocol/scope in the current network namespace, not process ownership or proof of firewall/isolation. |
| `database_binding` | Remains `unknown`. Candidates and anonymous client counts do not establish which database the running Memory service uses. |

The official Pi v0.5.1 **ARM64** binary was independently checked on 2026-09-17
without execution: ELF64, little-endian AArch64, loader
`/lib/ld-linux-aarch64.so.1`, required GLIBC symbol version **2.43**. Its dependencies
include `libgcc_s.so.1`, `libm.so.6`, `libc.so.6` and `ld-linux-aarch64.so.1`.
The preflight embeds this static evidence; it does not inspect or execute a Pi
file on the CA. A lower native libc is a real compatibility blocker. Do not
upgrade the CA's libc, replace the loader, substitute another Pi build, enable a
privileged container, or loosen isolation to bypass it. A separately reviewed
compatible runtime would need its own approved images, ARM64 execution proof and
negative filesystem/network tests. AMD64 container results do not prove ARM64.

Evidence is from the exact source revision
[`7c018475d54d4e09c907691e47a520f1f09e3288`](https://github.com/Dicklesworthstone/pi_agent_rust/commit/7c018475d54d4e09c907691e47a520f1f09e3288),
the [v0.5.1 checksums](https://github.com/Dicklesworthstone/pi_agent_rust/releases/download/v0.5.1/SHA256SUMS)
and [ARM64 build manifest](https://github.com/Dicklesworthstone/pi_agent_rust/releases/download/v0.5.1/build-manifest-pi-linux-arm64.json).
The downloaded archive SHA-256 was
`89906e3fd11842c1a571f174740a188ebdc6c8a7a65f51f1d6f0b23800ad2e44`;
the exact `pi` member was
`0190009efe002c3cbca80a5f005472cea0df5901706c9e49dcb6c4c6891e0884`.
Static `readelf` inspection established the ABI above. This upstream consistency
check is not an independently trusted operator signature or deployment approval.

### Database binding remains a console/operator gate

Only passwordless peer access as `postgres` to standard local Unix sockets is
attempted. The script bypasses startup/password/service files and requests
read-only transactions with connection, statement and lock deadlines. It never
reads query text or connection credentials. Counts are separate live queries,
not a transactionally consistent backup.

Before any migration/install decision, the operator must provide a separately
reviewed **redacted binding record**: the intended source Memory instance,
database identity, local/remote/container topology, whether any other writer or
host uses it, and its independently verified schema head. Use existing trusted
operator records; this script deliberately cannot derive that mapping from
secret-bearing config files. An active Memory unit plus one Memory-shaped local
database is insufficient. Failed connections, zero observed clients, a missing
standard socket, or empty recognized tables never prove there is no other data.

## 3. Optional authenticated staging once inputs are supplied

When a separately verified new Debian package has already been installed, its
read-only helper is `/usr/libexec/memory-layer/ca-preflight`, and its staging tool
is `/usr/libexec/memory-layer/area-release.py` (invoke with `python3 -I -S`).
The `memory-area` and `memory-area-import` commands and offline guides are also
included. The commands below use the transferred repository paths so preflight
and authentication remain possible **before** installing any new package.
Tool presence does not satisfy installation or migration gates.

Stop here if there is no real release bundle, independently trusted full signing
fingerprint, reviewed source revision/schema compatibility, or resolved target
architecture. Do not invent those values or copy a fingerprint from the supplied
public key and treat it as independent trust. This step uses public signature
verification only; it never accesses an operator private key or signs anything.

With a reviewed bundle already transferred locally and Python/GnuPG available,
these exact Bash commands request the three non-secret operator inputs and make
a fresh mode-0700 staging parent. The schema value must come from the confirmed
application binding, not merely a candidate database:

```bash
(
set -euo pipefail
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
read -r -p 'Absolute local release bundle directory: ' MEMORY_CA_RELEASE_INPUT
read -r -p 'Independently trusted full signing fingerprint: ' MEMORY_CA_SIGNING_FINGERPRINT
read -r -p 'Confirmed current Memory schema head: ' MEMORY_CA_SCHEMA
MEMORY_CA_STAGE_PARENT="$(mktemp -d /var/tmp/memory-ca-release.XXXXXXXX)"
python3 -I -S scripts/areas/releases/release.py verify-stage \
  --manifest "${MEMORY_CA_RELEASE_INPUT:?}/manifest.json" \
  --signature "$MEMORY_CA_RELEASE_INPUT/manifest.sig" \
  --public-key "$MEMORY_CA_RELEASE_INPUT/signing-public-key.asc" \
  --trusted-signing-fingerprint "${MEMORY_CA_SIGNING_FINGERPRINT:?}" \
  --architecture arm64 --schema-current "${MEMORY_CA_SCHEMA:?}" \
  --artifacts-dir "$MEMORY_CA_RELEASE_INPUT" \
  --staging-parent "$MEMORY_CA_STAGE_PARENT"
)
```

The input paths must be absolute, regular files through non-symlink ancestors;
the staging tool enforces its path, signature, selected architecture, schema,
size and hash contracts. Expected successful report:
`status: authenticated_and_staged`, `installation_authorized: false`, with a
new private staging location and selected ARM64 artifact evidence. It does not
unpack archives, execute their contents, inspect internal ELF compatibility, run
package hooks or switch a running release. Preserve the returned manifest,
signature, public key, verification report and artifacts together.

## 4. Stop before installation or activation

The remaining required evidence is not produced by this handoff:

- Confirmed existing Memory database binding and all writers; encrypted source
  backup and isolated restore/reconciliation proof; approved preservation of
  every version, tombstone and provenance record.
- A completed reviewed importer and target checks, distinct Trust database/role
  and identity, target hostname/TLS/Auth integration, and a reversible cutover.
- Authenticated release identity and genuine binary/web/schema compatibility;
  an installer that rechecks bytes under its privileged boundary, stops only the
  selected instance, preserves the old release, and never downgrades the DB.
- Actual CA-specific deny rules for every signing/OpenBao/backup/secret location
  and negative tests under the real service identity; no secret contents need
  to be copied into the handoff report. CA health must be checked independently.
- Separate Pi sandbox/proxy-only admission and authoritative per-area budget
  binding; no provider/CA/DB key is given to Pi. One authoritative ledger per area
  is required; cloned databases cannot be treated as a second spending budget.

The software now provides `memory-area-import schema-plan` and explicitly
confirmed `initialize-schema` for an already provisioned empty dedicated local
database. See the [schema-only preparation guide](../../crates/mem-area-ops/README.md#schema-only-preparation-before-importing).
Do not start the Memory service to obtain that schema: it also bootstraps
administration and makes fresh import ineligible. This available operator tool
does not authorize running migrations on the CA before its target, backup,
release and writer-fencing gates are resolved.

Do not create `activation-reviewed`, enable/start `memory-layer@trust.service`,
run a migration, or install the staged package solely because the preceding
commands succeeded. No `memory area install`, automatic cutover, or automatic
database rollback command exists. The exact privileged installation action and
rollback evidence must be reviewed separately against the resulting CA inventory.

## Recovery from these steps

Preflight made no changes to undo. Preserve any partial JSON and rerun the same
command only after the missing tool/permission or time-budget issue is reviewed;
do not change authentication or start PostgreSQL just to make the report green.

A signature, hash, schema, architecture or path failure means **stop**. Preserve
the failed staging evidence for review; do not install, overwrite the prior
release, delete existing data, or suppress the failed check. Staging may retain
unexpected/replaced files deliberately rather than deleting unrelated content.
Repeating staging creates a new private directory; it does not repair a live
installation or overwrite an earlier stage.

If the existing CA, OpenBao or Memory service is already unhealthy, report its
preflight state and stop this rollout. This handoff supplies no restart or repair
authorization. A future activated instance's recovery must target only that
instance and retain its data/configuration/release evidence. Never use a global
restart-all, package removal, database drop/reinitialization, or libc downgrade
as a rollback for this preflight/staging procedure.
