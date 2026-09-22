# Debian 12 Trust candidate: review before any CA change

This is an **internal, unsigned compatibility candidate**, not installation or
activation permission. Keep the existing CA Memory, Step CA and OpenBao services
running. Do not upgrade glibc, enable SSH, run package hooks, change credentials,
start Trust/Pi or point this executable at the shared database.

This guide describes the preserved September 20 ARM64 compatibility bundle.
The newer [September 22 coherent review](../../docs/reviews/four-area-network/coherent-release-preparation.md)
contains both Memory architectures and corrected console tools from one source
commit. It remains unsigned and does not authorize installation. Do not mix its
artifacts/checksums with this older bundle. The approved Trust database belongs
locally on the CA; remote database placement would be a separate design decision.

The operator confirmed that current CA Memory connects to the original shared
Monolith database. Its persisted rows are covered only up to the verified source
snapshot boundary. The inactive Trust partition is a reviewed subset, not a full
copy of all CA-related or shared data. Pending offline writes/local evidence and
new shared writes require reconciliation before cutover.

## What this package establishes

Read `PROVENANCE.json` and the linked build evidence before selecting anything.
The Memory source, Pi source/patch/lock/license, builder image, sysroot and every
artifact are separately bound. The package carries no dataset, credentials,
enrollment, consent, supervisor budget, live configuration or activation marker.

Static ELF inspection checks AArch64, loader, transitive libraries, required
symbol versions and the glibc 2.36 ceiling. Explicit QEMU userspace checks use
Debian 12 libraries, no network except a synthetic loopback provider, no real
credentials and a non-root read-only container. This is not native CA execution,
CPU compatibility proof, a CA kernel/cgroup test or proof of secret-path denial.
Existing production Pi pins remain unchanged; accepting a newly built Pi in the
runtime is a separate reviewed action, not a caller-supplied hash override.

## Read-only transfer verification

Only transfer through the already approved local console/removable-media
procedure. The release owner must supply the `SHA256SUMS` digest independently.
From the transferred package directory, these are the only immediate commands:

```sh
sha256sum SHA256SUMS
sha256sum --strict --check SHA256SUMS
```

Compare the first value to the independently supplied digest. Stop on any
mismatch. Hash agreement establishes identity with the reviewed copy, not an
operator signature or permission to execute it. Do not run `dpkg -i`, copy files
over `/usr/bin/memory`, start a service, or launch Pi from this package yet.

No published/signed `memory-release-v1` is asserted. That manifest currently
requires both supported architectures from its declared release; a single-ARM64
candidate must not be padded with unrelated older AMD64 artifacts. Operator
signing/release assembly and any Pi redistribution review remain separate.

Subsequent repository work adds an
[inactive console first-installer](../../scripts/areas/releases/console-README.md).
It is not included in this preserved compatibility bundle, does not accept an
unsigned candidate, and cannot upgrade or stop the existing CA singleton. Its
journaled placement/retention is distinct from live migration or activation.

## Metadata still needed from the CA operator

Return only these non-secret answers; `unknown` is valid and remains unresolved:

- Shared Monolith database binding: **confirmed by Olivier**.
- Any offline capture/activity store in use: yes / no / unknown.
- Pending unsynced items: aggregate count / unknown.
- Evidence files needed beyond the shared database: yes / no / unknown.
- Existing CA Memory writers/clients identified for later fencing: yes / no / unknown.
- CA signing, certificate distribution and OpenBao health independently checked:
  yes / no / unknown (running units alone do not prove these operations).

Do not send configs, connection strings, environment values, filesystem listings,
raw records, capture/session bodies, CA signing files or OpenBao storage. Do not
copy a live DuckDB store. Any consistent capture/replay is separately scoped.

## Later installation sequence: review gates, not an executable installer

1. Authenticate the final release with the independently trusted operator key.
   Prove the actual CA runtime and required library versions, then prove native
   filesystem/network denials under the real proposed service identities.
2. Prepare the independent Trust database/role and fresh scoped configuration.
   Confirm its actual schema. Do not discover/provision these by starting Memory.
3. Agree the exact maintenance window, identify and fence **all** old writers,
   including the CA's existing shared-database client, direct tools and offline
   replay. Keep signing/OpenBao and unrelated services untouched.
4. Make a final encrypted capture, independently restore it, revalidate ownership
   decisions against full-history/dependency hashes and reconcile offline work.
   Import into fresh inactive targets and verify histories/provenance/receipts.
5. A reviewed selected-instance installer must preserve the prior concrete
   release and matching digest/configuration, durably journal each external
   effect, reauthenticate bytes under its privileged boundary and stop only the
   exact Memory instance. Such a host-privileged installer is **not supplied**
   by this preparation package.
6. Activate Memory alone only after approval. Verify exact DB binding/schema,
   local search/history, denied cross-area access, authorization, restart and
   continued CA/OpenBao health. Review gateway enrollment/disclosure separately;
   then test deterministic two-way citations/revocation/reconnect. Pi stays off.
7. Admit Pi only after independent native sandbox, exact artifact/profile,
   service identity, model-egress consent, request confirmation and authoritative
   usage-ledger checks. Never expose the Memory database credential to Pi.

## Recovery boundary

The supplied `instance.py` verifies signed release-tree archives and rehearses a
filesystem switch and rollback **only in new disposable private directories**.
It never controls systemd or switches live credentials. Its tests cover tampered
files, competing paths, schema incompatibility and caught interruption; they do
not prove recovery after power loss/SIGKILL or a database migration.

For a future installation, stop only the newly selected Memory instance and
fence writes before recovery. Re-read the database schema and authenticate the
retained release. A previous binary may be restarted only if proven compatible;
never downgrade the schema. After new writes, any data restoration requires
separate reconciliation/forward recovery. Keep failed candidate/data/evidence,
and never undo CA signing, OpenBao, libc or unrelated services as Memory rollback.

The [preparation guide](../../scripts/areas/releases/instance-README.md),
[execution evidence](../../docs/reviews/four-area-network/ca-bookworm-release.md)
and [future cutover](../../docs/reviews/four-area-network/staging-cutover.md)
remain the review references. Copies are included beside this guide in the
standalone candidate handoff.
