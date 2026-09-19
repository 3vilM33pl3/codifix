# Homelab / Workshop exchange increment

Status: real two-way Homelab/Workshop expertise exchange verified, 2026-09-19.
This is an increment of the
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
- [x] Provision the private gateway/PDS with separately pinned receipt/repository
  keys and a separate space authority; retain interrupted-operation evidence.
- [x] Review exact per-recipient collections and enable deterministic exchange.
- [x] Demonstrate both directions and a denied non-member, with signed receipts.

The initial local Homelab selection contains two historical observability
records: SigNoz endpoint ownership and Docker blackbox-probe routing. This is a
delegated operator content review under the approved logical area split, not a
claim of separate human approval or current infrastructure validation. No raw
captures, sessions, source paths or unrelated project rows enter the new corpus.
The complete originals remain in the protected archive. This does not finish
the full dataset partition or fence original writers.

Homelab started with model calls, Pi execution, federation and background learning
disabled. Only the explicitly reviewed deterministic federation below is now
enabled; Workshop's existing time-bounded model grant is not extended to it.
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
operator operations. This was the pre-enrollment state; the real provisioning
increment below supersedes it. Collection activation remains separate.

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
the existing CA. The following increment records the resulting private deployment.
Neither staging nor the disposable rehearsal is the completed four-area network.

## Private PDS and gateway enrolled, 11:44 UTC

The real company PDS now owns `did:web:space.memory.robotmotel.com`. It uses the
unchanged pinned image/SDK, production account checks, explicit invitation and
durable bootstrap/create/key-discovery/handoff/activation phases. No disposable
fixture account, SSRF bypass, public DNS change or existing CA operation was used.
Names resolve only on the separate internal Docker networks. A newly generated,
scoped preview TLS issuer is not the existing company CA: its root lasts 30 days,
the leaves seven days, and renewal remains an explicit operator responsibility.
Its private root key is never mounted into a service.

The gateway runs with its restricted SQL role, independently pinned repository
and receipt keys, no model destinations and a complete schema-v3 fleet. The
separate authority enrolled Workshop and Homelab with Pull, Publish,
AskExpertise and AnswerExpertise only. Reviewed enrollment operation
`57767766-8fc9-44a8-896f-152c72f886f6` reached `published_proven` at sequence 8;
an exact retry retained the same record IDs and sequence numbers. Its published
tip is `f50656fb69923768a6fd6faaebe380410083ffecb53a0e7978d150e4a4969981`.

An independent, read-only pinned-SDK download verified the actual signed CAR
against the installed repository key and rejected the former bootstrap key.
It added no synthetic record to the real repository. CAR SHA-256:
`b36ec4f4054a20a47eb1fc35402ae5a10a9bef5a7fd077fe5b368255575446e7`.
The gateway audit key is distinct from both repository and bootstrap keys.

Encrypted, hash-verified NAS backups now include the enrolled PDS, its keys and
configuration, gateway logical dump and exact artifacts. The raw PDS was copied
only while stopped; live PostgreSQL data files were excluded. After-enrollment
ciphertext SHA-256 is
`c9e0b4fae14b2d2ffcc31614af22089d29100fa4293ec5e2fad7676e21a6b09d`.
Full restore/decryption/key recovery is still **not** demonstrated.

Only the PDS access token is installed in the gateway; the refresh token and
operator/bootstrap credentials are not. Session refresh, replacement token
installation, gateway restart and any blocked-publication retry require the
documented explicit operator workflow. At this checkpoint neither app is
attached, no knowledge collection is activated and no model/Pi call was made.
Local grants, exact review and real two-way answers are the next gates.

## Real deterministic exchange, 12:04 UTC

Both existing apps were preserved, upgraded and attached to their own company
gateway-edge networks. Their databases, source records, application TOMLs,
browser addresses and local credentials were unchanged. The original app
containers remain stopped with restart disabled. The standalone Workshop model
worker was paused for the backup and then resumed with its original manifest,
budget and 13:46:41 UTC expiry. Before resume, no eligible pending questions,
active actions or unsettled attempts existed; no paid call was initiated here.
Encrypted pre-attachment bundles were hash-verified on the NAS:

- Homelab: `8d8aeb05ab75ab652e8cf70580f59b6b6bda30056dfb2e58788590f60b66bbb0`.
- Workshop: `c25afe6a18eb1d4f46787c539977c8a6b091d1b0ffbf3ed400a363f29642d013`.

The initial NAS check stopped before service changes because `findmnt` reported
both autofs and its CIFS mount. A separately reviewed amendment retained the
original staging receipt and verified the exact `//bunnychow/olivier` CIFS share.
Only encrypted NAS files permit its existing group permissions; local secret
checks were not relaxed. Neither backup has a full restore/decryption rehearsal.

Workshop's new internal answerer `bd431be5-ffa4-447c-8bb2-5c9db0fe1ef4` was
provisioned through the supported transactional API, with only local project
read/export access and no login credential. Each answering identity then received
only its answering capability and its own exact collection grant. The human
operator has separate local pull/publish/ask/answer grants, not gateway authority.

| Origin | Reviewed collection | Scope |
| --- | --- | --- |
| Workshop | `212e0207-7f1f-4048-b5dd-c33e1f1fda45` | Three exact v1 Memory-development records; Homelab requester only |
| Homelab | `7bb6c76d-5319-459d-9898-91e2e0a3aab2` | Two exact v1 observability records; Workshop requester only |

Both revisions permit deterministic excerpts and `local_human` output only;
model destinations and selected evidence arrays are empty. These are delegated
operator reviews under the approved split, not a claim of a new separate human
sign-off. Review digests bind every field; original authorship/source versions
and timestamps are retained. No raw captures, paths, sessions or implicit evidence
are added. Advertisements, not collection contents, reached published sequence
10. Both apps caught up, with no blocked or queued publication intents.

Homelab request `fb567735-d3d5-4d5f-bd14-0eecff8ac168` and Workshop request
`762a3977-785a-47e8-b3c4-f500c79725ea` both reached `answered`. Local validation
recomputed request/review/collection/content digests, checked every citation's
author, source version, responder and exact approved excerpt, and confirmed
deterministic answer construction. Each answer contains one citation. Answer
bodies stayed in protected local artifacts, not cloud tool output.

The real browser workflow separately discovered Homelab, reviewed and submitted
`What do we know about SigNoz and blackbox probe routing?`, then displayed the
answer and attribution. Its request is `cffd69b6-eecf-448a-9d89-545e3095cdf0`.
An exact browser retry returned completed status without executing another
answer: the gateway held three distinct questions, three execution claims and
three completed answers. Submission/retry returns metadata only; fetching the
answer performs a fresh origin-approved delivery. The focused federation TUI
also started in a real PTY with its explicit local-human policy banner.

Eighteen live checks across the two member networks rejected missing/fake
credentials (`401`) and raw repository/CAR/event/config routes (`404`). Fresh
container inspection confirmed each app has only its own database network and
its own gateway-edge network, with no published ports or cross-database peer.
These checks do not substitute for a second-space/valid-wrong-credential test.

Source `28633739` adds explicit loopback human sessions for CLI/TUI without
changing default token behavior or minting deployment service tokens. Required-
database browser-session/CSRF/revocation tests, 13 federation client tests, four
supervisor regressions, scoped strict Clippy, formatting and boundaries passed.
The full CLI invocation had 258 passes and one pre-existing machine-local docs
link failure. No complete workspace/Lean regression is claimed.

The [demo guide](../../user/homelab-preview.md) describes the actual UI and CLI.
This is two independently stored areas on one host, not four independent hosts,
the complete dataset partition, autonomous expertise exchange, a signed software
release, CA activation or customer readiness. R3 remains paused. The scoped TLS
and PDS-session maintenance and full recovery gates above remain open.
