# Named Memory instances: staged deployment contract

The Debian package installs an opt-in `memory-layer@.service` template. Package
configuration does not start, enable or restart any service and does not create
or rotate API/database/provider credentials. Upgrade hooks do not stop services.
The legacy `memory-layer.service` and `memory-watch.service` remain available;
explicit package removal/deconfiguration stops those two system units only.
User services and named instances are never selected by package hooks.

The new template is a foundation, not evidence of four deployed areas. No named
instance, database, identity, gateway, TLS binding, watcher, supervisor or Pi
runtime is created by package installation. The CA remains console-only.

Linux packages additionally carry `memory-area` (offline planning),
`memory-area-import` (protected verification/preparation/explicit local import)
and `memory-federation-gateway` (the separately configured private gateway).
All are built for the selected architecture with locked dependencies and are
never executed by package hooks. The gateway is not started or configured by
installation and does not belong on the CA. Read-only CA preflight and authenticated staging
tools are under `/usr/libexec/memory-layer/`; their operator guides are under
`/usr/share/doc/memory-layer/areas/`. These are staged tools, not an automatic
installer, database provisioner or approval to migrate. No Pi binary, provider
key, source data or reviewed ownership manifest is bundled.

The Debian builder supports `--output-dir` to keep artifacts on a build volume.
When cross-compiling, completions come from a host-native lean CLI; target
binaries are only packaged, never executed. An ARM64 package build is **not**
an ARM64 runtime test: actual CA compatibility remains a console gate. Artifact
filenames retain SemVer (`2.1.0-rc.1`); Debian's internal version is
`2.1.0~rc.1`, so a later `2.1.0` stable package upgrades the candidate correctly.
The build also requires `dpkg-shlibdeps` and the selected architecture's runtime
symbol metadata. It computes versioned libc/C++/GCC dependencies from all four
ELFs without executing them; missing or unresolved dependencies fail the build.
Cross sysroot package names are mapped only to their known target counterparts,
without reducing their version bounds. Python 3 and GPG support the operator
tools. PostgreSQL is suggested, not pulled in or started implicitly: provision
each intended database separately. Do not upgrade CA system libraries merely to
satisfy a candidate package; review compatibility before installation.

## Files and identities

Use lowercase names of 1–19 characters beginning with a letter, for example
`trust`, `homelab`, `workshop` and `general`. Trust belongs on the CA host; the
other three belong on Monolith. Each instance needs a distinct database and role,
API identity, Authentik binding, private hostname and verified TLS. Different
projects in one database do not provide this separation.

| Purpose | Path or identity |
| --- | --- |
| Configuration source, root-owned mode 0600 | `/etc/memory-layer/instances/<name>/memory-layer.toml` |
| Expected binary SHA-256, one lowercase hex digest | `/etc/memory-layer/instances/<name>/memory.sha256` |
| Operator activation record | `/etc/memory-layer/instances/<name>/activation-reviewed` |
| Independently staged executable | `/opt/memory-layer/instances/<name>/release/bin/memory` |
| Matching static assets | `/opt/memory-layer/instances/<name>/release/web/` |
| Dedicated dynamic service user | `memory-area-<name>` |
| Private persistent service state | `/var/lib/memory-layer/instances/<name>/service/` |
| Private cache / runtime state | `/var/cache/memory-layer/instances/<name>/service/`, `/run/memory-layer/instances/<name>/service/` |

All executable ancestors must be root-owned, non-symlink directories without
group/other write permission. The service verifies the binary against its loaded
digest before every start. The digest must come from a release manifest verified
against an independently trusted signing key; the SHA-256 check alone does not
authenticate a release. The release directory is a concrete staged copy, never
`/usr/bin/memory` or a `latest` symlink. Package upgrades do not replace it. A
future selected-instance installer must stop that instance before changing its
release directory and must retain its verified prior release and manifest.

The service manager loads only the selected TOML and digest as read-only
credentials. The runtime cannot read their source directory or write its config
or executable. State/cache/runtime directories have mode 0700 and separate
dynamic identities. Each instance's XDG paths point inside its own state. No
shared global environment file is loaded. Put the independent database and API
credentials in the selected TOML. Provider credentials are deliberately absent.
The fixed launcher clears **all** inherited environment variables, then supplies
only the production profile, fixed executable path and area-local XDG paths.
It ignores shell startup files and refuses the wrong instance credential
directory. Arbitrary `MEMORY_LAYER__` overrides, provider keys, proxy settings,
SSH agents and an ambient `MEMORY_LAYER_FEDERATION_CONFIG` cannot reach the app.
This is not an OS isolation proof; the unit sandbox remains independently required.

The staging TOML explicitly disables offline replay, reinforcement, provenance
rechecks, background work and automated model/embedding work. Do not start the
application to perform a supposedly inert import: normal service startup runs
migrations and bootstraps local administration. Perform import with writers
stopped, using the protected importer, before the separately authorized startup.
Use the explicit `memory-area-import schema-plan` / `initialize-schema` workflow
from the [operator guide](../../crates/mem-area-ops/README.md#schema-only-preparation-before-importing)
to prepare an already provisioned empty dedicated local target. It does not
create a database/role, upgrade an existing schema or activate an instance.

### Explicit federation credentials, after enrollment review

Federation stays off unless the operator adds both a named systemd credential
and the launcher's `--federation` flag. The only accepted file is `federation.json`
beside this instance's loaded TOML. The JSON must follow the current runtime
contract and reference only separately scoped credentials. A reviewed drop-in
for the selected instance can use:

```ini
[Service]
LoadCredential=federation.json:/etc/memory-layer/instances/homelab/federation.json
ExecStart=
ExecStart=/usr/libexec/memory-layer/run-instance homelab %d --federation
```

This is an inactive example, not permission to enroll or start a service. Replace
the instance consistently, review exact TLS/gateway/receipt-key and collection
grants, and test egress/isolation before enabling it. An existing credential
without the flag does nothing; the flag with a missing/symlink credential fails.
The Memory credential directory must not be exposed to Pi.

## Activation gates

The [inactive console first-installer](../../scripts/areas/releases/console-README.md)
can place authenticated release/configuration bytes for a **new** named instance
and recover interrupted publication. It refuses existing names and leaves
activation disabled. Its tests do not satisfy the independent gates below.

1. Complete the writer inventory, encrypted backup and isolated restore proof.
   Fence old writers before migration/cutover. Preserve the original rollback
   source; package installation is not a migration or a backup.
2. Verify the release signature, source revision, binary and web asset hashes,
   architecture and schema compatibility. Stage only the selected release and
   retain its manifest. The package builder does not sign release manifests.
3. Provision distinct restricted databases/roles and credentials. Prepare the
   per-instance TOML from `instance.toml.example`; replace every REVIEW field.
   The empty API token and placeholder database intentionally are not usable.
   Do not copy old runtime jobs, tokens or federation authority into targets.
4. Import/reconcile canonical data with workers, watcher, federation and all
   provider calls disabled. Check histories, IDs, hashes and cross-area denials.
5. Verify the host's systemd supports the sandbox directives and enforces network
   filtering; syntax verification alone does not prove a sandbox. The template
   permits loopback only. An independently reviewed egress policy is required
   for a remote database or private gateway. Never widen it to unrestricted
   internet access to make activation succeed.
6. On the CA console, inventory and deny every actual step-ca, OpenBao, signing
   key, backup and secret-storage path in a local unit drop-in, including custom
   mounts. The template denies common `/etc` and `/var/lib` service paths, all
   home directories, container sockets and privileged kernel interfaces, but
   does not discover custom paths. Prove negative reads under the real service
   identity and verify signing/certificate distribution/OpenBao remain healthy.
   Failing or unsupported sandbox controls block activation.
7. Record the reviewed release, restore proof, policy revision and checks in
   `activation-reviewed`, then explicitly start only the reviewed unit, such as
   `systemctl start memory-layer@homelab.service`. Enable it only after a verified
   restart/recovery check. A missing marker prevents the template from starting.

This template applies to the Memory application service, which needs its own DB
credential. Pi is a separate process and sandbox: never give Pi this TOML,
database credential, signing material, Docker socket, broad shell or privileged
installer access. No Pi/supervisor unit is shipped until that executable and its
narrow RPC contract exist and pass the independent CA and budget gates. Do not
run Pi as a child of the DB-bearing application service.

## Legacy singleton and rollback

Existing singleton configuration/credentials survive upgrades. Fresh singleton
setups must explicitly provision an API token after reviewing their configuration,
for example with `memory --config /etc/memory-layer/memory-layer.toml service
ensure-api-token --json`; rotation is a separate operator action. Only after that
review should the operator start `memory-layer.service`. Package installation
alone leaves an already running singleton on its old mapped executable until an
explicit selected-service restart. Schedule that restart and assess schema
compatibility; it must not be hidden in a package hook.

Stopping a named instance is explicit: `systemctl stop
memory-layer@<name>.service`. Retain its data, configuration, release and evidence.
Removing the package does not delete instance state or credentials. A stopped
named instance cannot restart after removal until its template/verifier has been
restored. Binary rollback requires proven schema compatibility; these templates
do not perform automatic database downgrades or claim an external rollback.

## Development checks

Run `bash packaging/tests/test-debian-lifecycle.sh`. The tests stub lifecycle
commands and execute the real hooks; they never install a package or change host
services. A real on-host namespace/identity/network isolation test is a separate
activation gate and is not implied by those tests passing.
Run `python3 packaging/tests/test-instance-launcher.py` for synthetic launch tests:
they rebase paths into temporary storage, execute a fake app, and verify clean
environment, disabled implicit federation, exact credential paths and denied
shell-startup injection without touching `/opt`, host credentials or services.

The Debian builder uses a fresh private staging directory and refuses to
overwrite an existing `target/debian/memory-layer_<version>_<arch>.deb` artifact,
including symlinks. Preserve or explicitly remove that exact output before an
intentional rebuild of the same version. Builds never delete a previous staging
directory or write through its symlink.

## Read-only CA console preflight

For the exact console handoff, expected results, ARM64/libc blockers, optional
authenticated staging and recovery stops, use
[CA-CONSOLE-HANDOFF.md](CA-CONSOLE-HANDOFF.md). It does not claim an installer exists.

After placing this reviewed repository snapshot on the CA using an existing
approved transfer path, open the CA's local root console in the repository root
and run this single command:

```bash
bash deploy/areas/ca-preflight.sh
```

The command prints redacted JSON directly to the console; the JSON is suitable
to share for migration planning. It requires Python 3 and neither invokes sudo
nor enables SSH. It makes no service, package, configuration or credential
changes, and does not contact a provider or remote database. It reads OS metadata,
filesystem capacity, service active states and listener port/scope metadata.
Format `memory-ca-preflight-v2` additionally reports GNU libc, namespace/kernel
limits, exposed cgroup-v2 controllers, container-tool presence and pinned Pi ARM64
static ABI metadata. The official pinned ARM64 binary requires GLIBC 2.43; the
script compares versions without executing it. Tool/controller presence proves
neither a working container runtime nor an enforced sandbox. No container client
or daemon is invoked. The system executable search path is fixed, not inherited.
Service logs, command lines, network addresses, config/env content, secret paths
and application records are excluded. It does not inspect CA keys or OpenBao.

When run as root with `psql` and `runuser` available, it inventories up to four
local PostgreSQL sockets in the standard runtime directory and 16 databases per
socket. It uses passwordless local peer authentication as `postgres`, with
startup/password/service files disabled, read-only transactions and query
timeouts. Output includes database names/sizes, recognized Memory table counts
and SQLx schema head, plus anonymous local/client connection counts (not process
or Memory-client identity). It only selects fixed aggregate/catalog metadata from
ordinary base tables, excluding views, foreign/partitioned tables and inherited
children. Memory entry counts include all versions and tombstones; they are not
a content review.
The subprocess time budget is 60 seconds; partial checks are labelled.

An unavailable, denied, missing or custom socket never proves there is no data.
Custom locations, containers, remote/shared databases and bindings are explicitly
unverified. The script deliberately does not read the application's database
configuration, so **shared versus unique local data remains unresolved** even
when a local database has Memory-shaped tables. A separate reviewed binding and
content reconciliation is required before migration. The top-level binding result
lists local candidate metadata but remains explicitly `unknown`; connection
counts never identify the Memory service or prove the absence of remote writers.
Table counts are live observations across separate queries, not a consistent
backup or restore proof.

Test without inspecting a host or database:

```bash
python3 packaging/tests/test-ca-preflight.py
```

The additive `dataset_coverage` report remains unknown for CA-only databases,
offline captures/activity and external evidence. Trust staged on Monolith is
not a CA migration. The shared original database remains authoritative while
writers continue; previews are not implicitly merged into that source. Use the
[future-cutover runbook](../../docs/reviews/four-area-network/staging-cutover.md)
for fresh full-history/dependency revalidation and the later writer-fencing,
activation and forward-recovery gates. Nothing in that runbook authorizes a
current cutover, CA installation or database downgrade.
