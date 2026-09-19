# Homelab / Workshop exchange increment

Status: independent local Homelab preview verified, 2026-09-19; exchange remains
in progress. This is an increment of the
[approved four-area contract](contract.md), not a completed network or customer
release. The approved-plan Memory checkpoint was renewed before edits.

## Current scope

- [x] Independently import a small reviewed Homelab observability collection,
  preserving original project/version identities and archive-reference provenance.
- [x] Verify encrypted backup copies and restore the new target before activation.
- [x] Demonstrate the independently authenticated local Homelab Web/CLI instance.
- [x] Add reviewed, digest-bound gateway membership enrollment tooling.
- [x] Add a private company topology without fixture accounts or bypass networks.
- [x] Rehearse real TLS, network separation and pinned-PDS provisioning with
  disposable credentials; independently verify the signed repository.
- [x] Provision Homelab's restricted internal answering identity through the
  transactional, audited installation API; keep remote grants disabled.
- [x] Prepare separate gateway schema and restricted runtime credentials; verify
  its encrypted NAS backup without connecting either preview.
- [ ] Provision the private gateway/PDS with separately pinned receipt/repository
  keys and a separate space authority; retain interrupted-operation evidence.
- [ ] Review exact per-recipient collections and enable deterministic exchange.
- [ ] Demonstrate both directions and a denied non-member, with signed receipts.

The initial local Homelab selection contains two historical observability
records: SigNoz endpoint ownership and Docker blackbox-probe routing. This is a
delegated operator content review under the approved logical area split, not a
claim of separate human approval or current infrastructure validation. No raw
captures, sessions, source paths or unrelated project rows enter the new corpus.
The complete originals remain in the protected archive. This does not finish
the full dataset partition or fence original writers.

Homelab starts with model calls, Pi execution, federation and background learning
disabled. Workshop's existing time-bounded model grant is not extended to it.
The local preview uses a different loopback host (`127.0.0.2:4261`) so its browser
session cookies do not replace Workshop's cookies on `127.0.0.1:4260`.
Neither owner-bootstrap preview endpoint is a LAN service.

Trust/CA, General, DNS, existing homelab services, paid evaluation and customer
release gates remain unchanged. Existing Workshop remains independently usable.

## Verified local increment

Use the [Homelab demo guide](../../user/homelab-preview.md). The separate instance
is available at `http://127.0.0.2:4261/`; `memory-homelab` opens its TUI.
The original Homelab project ID is preserved, with persistent imported instance
ID `1573e46e-ee3a-4ed9-b5ac-4b2fbf8da196`. The health endpoint's process instance
ID is a different runtime identifier, not an enrollment identity.

The preview contains two complete, single-version canonical lineages and 14
archive-only provenance references, no raw captures and no other projects. The
114-table pre-activation logical database restore matched row counts and ordered
row-content digests. The restored test database was then closed to connections.
This is database recovery evidence, not a full host/key/offsite recovery claim.

At 09:14 UTC, both instances accepted only their own credentials (`200`) and
rejected the other's (`401`). Asking Homelab for the absent `memory` project
returned an empty result, not Workshop records. Its deterministic query cited
exactly the two selected Homelab versions. Web inspection and a real PTY TUI
session showed both records and source detail; CLI health and query succeeded.
The app and database have no published Docker ports. Their network is internal;
only the fixed UI and database relays listen on host loopback.

Runtime startup exposed an owner-privilege assumption in automatic migration.
The new explicit `database.migrations = "verify_only"` checks the full embedded
migration history and fails closed without attempting DDL. The runtime role has
neither schema-create nor migration-history-write privileges. Required-database
runtime tests (including sealed R3 behavior), focused config tests, PostgreSQL/
TUI CLI build and strict Clippy, formatting and boundaries passed. The unchanged
Web bundle was copied from Workshop; this is not a claim of a new full workspace,
Web or Lean regression run.

### Protected evidence

Receipts and configuration are under
`~/.local/state/memory-homelab-preview-20260919`.
Encrypted, hash-verified NAS copies are under
`/media/bunnychow/backups/memory-four-area-20260917/homelab-preview-20260919`.
No credentials or source archive bytes are committed to the repository.

| Artifact | SHA-256 | Scope |
| --- | --- | --- |
| Reviewed manifest | `8b82d938eed242f8d93b6ab15303880e8f8867ca213d6f8e38bade4ef2ca45d8` | Two selected lineages; complete remaining source archive |
| Encrypted source/review archive | `2d0af863e67e5e53f6b8c930231f8efc6be7206dd6081a55001393a77c220518` | Protected full source and review inputs |
| Encrypted pre-activation dump | `023f6f6d96172fbebb5ce31729bb02512c2225a748a84161d2703cb9299589c2` | Logical contents independently restored before activation |
| Restored-table comparison | `d0bdce70e924605c0e5482de3fa31903aa3b8e1478e179ab165559ac5583e97f` | All 114 logical tables |
| Initial prepared application | `35a2416aae9aca5d094cde45819ece41976a58e314649650860cedf11de57ac8` | Preserved in the initial runtime recovery bundle |
| Runtime recovery bundle | `d77faf76e639ac7fd800c201a886fd8b78e77d492316919214f3953b46e4e6b9` | Fresh dump, exact binary/Web, own configuration/credentials and relay helpers; no provider key |
| Pre-answerer-upgrade encrypted bundle | `ee7466d90a3186e7302e1fba55a2e1b16dcca7e9e43c88fc77ba071bd02f600b` | Fresh stopped-app dump, previous binary/configuration and unchanged Web assets; NAS copy verified |
| Running application after answerer upgrade | `f5cfca67ab20f0acf27d6a91f95187ba597288504ce2a13c9d84d929398d1c3f` | Source `5532e35c`; prepared database plus audited restricted identity provisioning |

The runtime bundle and encrypted decryption/recovery path have **not** had a full
restore rehearsal. Do not equate a verified ciphertext copy with verified key
recovery. Do not replay the import into the now-active target.

### Restricted answering identity

After the fresh encrypted backup, only the Homelab app was stopped/upgraded and
restarted. Schema `0060`, configuration, existing credentials, database and
Workshop were unchanged. At 09:52 UTC, authenticated health, two-record
deterministic retrieval, absent-project isolation, separate container networks
and crossed-credential rejection (`401`/`401`) passed again.

The supported `memory auth expertise-answerer` command provisioned internal
principal `6ac610fb-6bf8-48dd-abee-22da17dc7149` under request
`6a408c12-9442-42ad-9a02-933d4a93dd64`. Database inspection verified exactly one
membership in Homelab, only `memory_read`/`bundle_export`, no global role, no
service token or browser session, and one protected auth audit event. Exact CLI
retry returned `replayed: true`, without adding authority. Federation grants,
collection consent, model permission and answering-worker activation remain
separate, unexecuted steps.

Five focused required-database identity tests passed, including concurrency,
current authority, audit rollback and HTTP protections. The complete service
test invocation passed serially: 143 library, 22 database and one OpenAPI test,
with two ignored tests. A first parallel run exhausted the disposable database's
connection capacity; it did not establish a product failure or a pass. Scoped
strict Clippy, formatting, boundaries and the PostgreSQL/TUI build passed.
The CLI runtime group passed 111 tests with the unrelated local-document-link
checker excluded: older financial/pitch documents still reference unavailable
machine-local files. No complete workspace/Web/Lean regression is claimed here.

## Gateway work remaining

The [membership operator](../../user/federation-enrollment.md) requires an
independent authority, exact review digest and pinned signed history. Its tests
include private-input file replacement rejection. The
[company topology](../../../deploy/federation/company-topology.md) and isolated
[bootstrap overlay](../../../deploy/federation/company-bootstrap-topology.md)
keep member, publisher, operator and DID-host paths separate.

The [durable PDS provisioning operator](../../../deploy/federation/pds-enrollment/README.md)
now retains creation, signing-key handoff and refresh intent instead of borrowing
a short-lived session from the disposable fixture probe. Its 24 injected tests
pass. The [combined disposable rehearsal](../../../tests/company-topology/evidence-2026-09-19.md)
also passed real pinned-image SDK, TLS/DID, strict account creation, reviewed key
handoff, activation, session refresh and independent signed-repository checks.
It proves those bounded prerequisites, not a running Memory gateway or real
knowledge exchange. The installed previews must be reused through dedicated
per-member gateway networks, not replaced by the blueprint's new empty apps/DBs.
Gateway token installation/restart and blocked-publication retry remain explicit
operator operations. No real company PDS, membership or collection has yet been
activated by this increment.

## Gateway staging, 10:07 UTC

The new `memory-company-20260919-gateway_db-1` container holds a dedicated
`memory_gateway` database, not a copy of a participant database. It runs as
`1000:1000`, read-only root, all capabilities dropped, with no published port.
Its internal IPv4-only `memory-company-20260919_gateway_database` network has
only this database attached. Its owned, protected Unix socket permits the
schema-owner operation without opening another TCP relay.

The new production [schema-only operator](../../../deploy/federation/gateway-schema.md)
applied all 60 migrations in one transaction after exact target/digest review.
The separate `gateway_runtime` role successfully authenticated over TCP and was
checked: no superuser/database ownership/schema creation, no memory/principal
read access, no migration writes, SELECT on exactly ten federation tables, allowed
mailbox cleanup and no audit deletion. Actual gateway process startup and its
full permission check remain pending. No app bootstrap occurred: memories,
principals, enrolled spaces, accepted records and publication outbox are empty.

The shared empty-database guard additionally rejects schema-scoped objects such
as collations/operators/text-search definitions and standalone publications and
foreign-data wrappers. Nineteen required-database operator tests, strict Clippy,
formatting and boundaries passed. Source: `34f1eac8`.

Prepared artifacts are local immutable image IDs, **not** registry manifest
digests or signed release artifacts. Build contexts contained only their
explicit recipes/binaries; builds used no network or implicit pulls. The
gateway/enrollment binaries were checked by SHA-256; a wrong digest was rejected.
The enrollment executable and cap-free Caddy started under non-root/read-only/
no-network constraints. The gateway executable started and correctly rejected a
missing config; this is not a configured gateway service test.

| Prepared artifact | SHA-256 or local image ID |
| --- | --- |
| Gateway/enrollment image | `sha256:75b7779cc11bbab20f5265c37c05a4813d397d8ca607d0917e4dcc37332e1e14` |
| Cap-free proxy image | `sha256:ef6057bcc90312b685a620f9a5c1eb511b582da32a2504919dcf331a9eb303e3` |
| Schema-owner operator | `54aeaa21c524cde23726305e4b5ff770c21053e4ddb9d98cc54bf6e517a6f87c` |
| Encrypted pre-enrollment backup | `e133f2986443eec37a7bfd9a57dd0825b4188408de66772575d4a21954ad41bb` |

Private state/receipts are in `~/.local/state/memory-company-20260919`, and the
verified ciphertext copy is under
`/media/bunnychow/backups/memory-four-area-20260917/company-20260919`.
The backup includes the logical dump, this new cluster's roles, own credentials,
planned identities and exact binaries. It excludes live raw database files and
contains no source knowledge or provider keys. Full restore/key recovery remains
unrehearsed; the copy's hash is not a restore claim.

The [existing-preview adoption planner](../../../deploy/federation/company-preview-adoption.md)
removes duplicate member apps/databases/UI. Its narrow prepared-gateway-database
option preserves this staged DB and joins only the gateway to its exact external
network. Workshop and Homelab each get a different fresh gateway-edge network;
neither existing app/database network is shared. Ten adoption tests and eleven
underlying topology tests pass. Live ID/label/alias/network checks are still
required before attachment; supplied inventory is not a live verification.
Because the DB has its own Compose file, **never use orphan-removal options** on
the shared company project.

Olivier approved the private namespace `memory.robotmotel.com` on 2026-09-19,
before permanent enrollment. This does not authorize public DNS or changes to
the existing CA. No permanent repository DID, TLS issuer, PDS account, remote membership
or collection consent has been installed. No public DNS, existing CA service,
Workshop model policy or original dataset writer was changed. Next: settle the
private names/trust configuration, provision the pinned PDS, verify/install its
keys, start the restricted gateway, and review both directions of deterministic
knowledge exchange. Neither staging nor the disposable rehearsal is the completed
four-area network.
