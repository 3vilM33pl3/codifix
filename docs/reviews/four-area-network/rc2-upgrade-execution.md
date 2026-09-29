# Upgrade every Memory installation and complete the four-area setup

Approved by Olivier on 2026-09-29. Target: one coherent internal Memory Layer
2.1.0-rc.2 release from the current federation branch. Workshop, Homelab and
General run independently on Monolith; Trust runs locally on the CA. Preserve
schema-v2 history, schema-v3 private federation and current PDS/Atrium/Pi pins.
R3 stays paused. No public access, unrestricted sharing or new paid execution.

## Release and installation

- [ ] Inventory active/dormant client, service, watcher, MCP, plugin and skill launch paths on both hosts; preserve historical and disposable installations.
- [ ] Build coherent AMD64 and Debian-12-compatible ARM64 binary/Web/operator/skill artifacts from one frozen commit; refresh skills from that exact bundle.
- [x] Implement a journaled selected-instance upgrade: signed inputs, backups, selected service stop, executable/Web switch, compatible append-only migration and health verification; preserve CA sandbox drop-ins.
- [ ] Sign the release using the established identity and normal local unlock, with explicit schema compatibility and no database downgrade.

## Complete dataset review and reconciliation

- [ ] Encrypt fresh source/area/gateway/PDS/runtime backups and verify isolated restoration including sequences, identities, receipts and usage.
- [ ] Inventory all source items and reuse ownership decisions only when full-history and dependency hashes match.
- [ ] Review redacted captures, sessions, replacement/loop proposals, protected histories, relations and derived gaps; give every item an import, pending-work, protected-archive or retired-runtime disposition. Secrets remain local.
- [ ] Preserve IDs, versions, tombstones, authorship and provenance; archive mixed sensitive material and cross-area relations without implicit disclosure grants.
- [x] Implement a separate existing-target reconciliation operator with source/decision/target hashes, stopped writers, durable operation receipts, identical-record deduplication, compatible history append and explicit conflicts; retain destination-only records and all live authority/budget/federation state.
- [ ] Complete initial review live and final-delta review with old writers fenced; account for every item before routing switches.

## Routing and federation

- [x] Implement operator-owned area routing shared by CLI/TUI, launchers and watcher manager: explicit area, exact repository/project mapping, then General; unavailable areas never fall back elsewhere.
- [x] Bind watchers and offline outboxes to area; preserve replay IDs and prevent duplicate watchers.
- [ ] Route this repository to Workshop, infrastructure to Homelab, CA-local work to Trust, remaining projects to General; distinguish same-name projects by area and identity.
- [ ] Keep Trust service CA-local and access remote expertise through the gateway; retain Trust-bound writes for CA reconciliation.
- [x] Extend federation to explicit per-project bindings, retaining single-project configuration and one ordered stream per instance/space; separate project authority and answerer grants.
- [ ] Enroll General; preserve existing instance identities and reviewed collection allowlists.
- [ ] Enable scoped operator-bound Codex/ChatGPT OpenAI MCP expertise callers with explicitly reviewed output-destination consent. Use deterministic answers and explicit remote questions.
- [ ] Display area, instance, release and federation status in Web/TUI and provide an area landing page.

## Coordinated cutover

- [ ] Complete release, review and disposable rehearsal while existing services remain available.
- [ ] Deliver one versioned CA execution/recovery/report package through Notion; avoid routine human command relaying.
- [ ] Stop identified legacy writers on both hosts, fence the old shared source and resolve final/offline work.
- [ ] Reconcile final delta; sequentially upgrade gateway and participants, create General, preserve identities and accepted publication state.
- [ ] Switch all clients/project routes; make CA command/TUI/watchers use Trust and disable its old shared-source writer.
- [ ] Restart verified local services/watchers and only still-authorized existing background/supervisor work.
- [ ] Preserve old source read-only and prior artifacts; recover forward after new writes/publications.

## Verification and completion

- [ ] Test reconciliation/replay/conflicts/target-only changes/rollback/interruption, routing/name collisions/offline queues/stale writers, and multi-project federation isolation/legacy compatibility/ordering.
- [ ] Verify four-area discovery and cited answers, General enrollment, allowed OpenAI and denied unbound MCP callers, withdrawal/revocation/restart/catch-up.
- [ ] Run required-database Rust and federation/PDS regressions, fmt/Clippy/boundaries, Lean, Web tests/build, and CLI/TUI/MCP workflows.
- [ ] Verify active hashes/launchers, local search/capture/history/citations for every area, and Workshop-to-Trust exchange.
- [ ] Record deployment and protected-item evidence in repo and Notion, obtain concrete CA result and commit implementation/evidence.

Complete only when all active Memory components use the coherent release, four
areas own their intended data/new writes, all backlog items have a disposition,
approved agents exchange cited knowledge, and no active writer targets the old
shared database. CA result must include release/hash, DB/schema, default CLI/TUI,
watcher routing, counts, federation sync and answer results, with failures explicit.

## Execution evidence

- Approved-plan checkpoint succeeded through dev Memory at 2026-09-29T20:32:33Z.
  Task `548a0737-9d40-4cd2-ba08-82d54c026ad5`; capture
  `4ce311cb-0a7f-4611-8a94-3f956a56100e`; curated plan
  `e9a1d45f-fa56-4b0f-85f6-8b1f7183b4fc`. Starting commit `7e54f185`.
- The dev service was unavailable. A private temporary configuration restored
  port 4250 with model curation, embeddings, durable workers, consolidation,
  provenance checks, reinforcement and cluster discovery disabled. Existing
  installed service and federation participants remain unchanged.
- Initial inventory: default Monolith CLI paths resolve to `/usr/bin/memory`
  2.0.0; the default service and manager still use that installation. Workshop
  and Homelab preview applications and CA Trust use 2.1.0-rc.1. General is staged.

Status: implementation in progress; no release or data cutover yet.

### RC.2 software and online baseline, 29 September

The checked items above mean implemented and tested software, not installed
components. The initial candidate `de99b052` was superseded before signing by
the portable CA partition, reconciliation performance/provenance fixes and
scoped-token compatibility work. Its successful ARM64 build is build evidence,
not the final release. The final common source revision and both artifact hashes
must be recorded after the remaining authentication tests complete. Signing and
installation remain pending.
Migration 0061 is additive. Existing-target reconciliation is a separate command
from fresh-target import. Selected-instance upgrade preserves the effective
sandbox and becomes forward-recovery-only once candidate startup is attempted.

Monolith inventory recorded 448 hashed inputs, 17 units, 23 processes and 154
registered projects. Both user and system singleton services remain live writers
to the old shared database. Five fixed watchers, one transient watcher, the old
manager, seven resident MCP processes and one TUI require coordinated switching;
changing a symlink alone will not replace those processes. Historical/test
containers remain excluded. CA inventory has been requested through CURRENT
ACTION v11; the last observed CA result is the completed rc.1 capability report,
not an rc.2 upgrade.

Fresh encrypted source snapshot `05fa26bb-270d-4b64-8216-ef5b6d1c937d`
was restored with no network, providers or application workers. Schema/rows and
four sequence states matched. Independent payload verification covers 125 tables
and 3,518,406 rows; payload SHA-256
`d2ea8638777d6f3f2628a44a49e4871de47b9909b5a57e6c1125f2c9f72ef421`.
Private evidence is retained under
`/data/tmp/memory-four-area-rc2-baseline-20260929T2130`.
Separate encrypted Workshop/Homelab rehearsal dumps were restored into
network-none containers: existing schema 60 and respectively three/two curated
versions. These are rehearsal copies, not the coordinated final backup.

The complete review pack contains 3,537 histories / 3,842 versions. Its strict
delta carries zero decisions automatically: schema/dependency hashes changed.
Of the 3,207 older histories, 3,203 retain identical direct curated/supporting
row bindings and four received changed-content review. All 330 new histories
have now been reviewed across their full redacted versions. The complete ledger
assigns 1,669 histories to Workshop, 429 to Homelab, 20 to Trust, 900 to General
and 519 to the protected archive. Its exact file SHA-256 is
`1c6afbe5c254c53ae3e768f53115b00ddff99cdb5905e153641b87f5c7e68e3f`.
The independently validated normalized manifest SHA-256 is
`8d97bec20cbee6000b155f82f5096beec2d3b2d86785510544f770841664c593`.
Per-item backlog accounting and materialization remain separate checks.
Agent ownership decisions
are not Olivier's individual factual approval or external-disclosure consent.

Scoped Rust/database tests, Web tests (149), Web build, Lean build/test,
formatting, boundaries and scoped Clippy pass. The first whole-workspace
functional run exhausted PostgreSQL connection pools under default parallelism;
the bounded four-thread rerun passed 1,227 tests across 91 groups (14 ignored,
seven documentation checks filtered). One existing documentation test
rejects historical machine-local evidence paths in M2 review notes; that failure
is recorded rather than hidden by changing old evidence. The explicitly enabled
browser/PTY and real CLI/session/CSRF rehearsal tests also pass. Fifty-one PDS
refresh/operator/replacement tests pass. Reconciliation now retains full
reference-only version/source/task provenance in its immutable receipt, including
exact replay, and hashes each target table in one ordered cursor scan.
Production workspace Clippy passes; full all-target Clippy found test-only lint
issues that were repaired and requires its final rerun. Native package checks
and live CLI/TUI/MCP/federation acceptance remain pending.

### Frozen release and complete-data rehearsal

Final runtime source: `bf78efa20478881b5eff15713a0daa5f2b6d7cb4`; source archive
SHA-256 `93dbd4c1a8a7bfbd194e01a0ef56085b8432c8d6cc30df4f64bc1f37ca092bab`.
Single-user mode now accepts explicitly issued, project-scoped machine tokens
without changing local browser login. Thirty required-database authentication
tests and 17 federation regressions pass. Final workspace all-target Clippy,
formatting and boundary checks pass. Bookworm Web tests pass (149).

All 3,518,406 rows now have recorded dispositions in the private backlog ledger
(file SHA-256 `b3939d89502c2727d67cbce5e07c1dcb9653f2b881c02e0647e5e8524c00b5d8`).
Pending captures, proposals, raw sessions, cross-area relations and derived-data
gaps remain explicitly pending/protected; this is not factual approval or a
claim that archived material is searchable. The exact complete archive is
encrypted and independently verified on the NAS.

Disposable rehearsals, not live imports:

| Area | Histories | Versions | Projects | Outcome |
| --- | ---: | ---: | ---: | --- |
| Workshop | 1,669 | 1,814 | 4 | Existing-target merge; 437,886 inserted rows; exact replay |
| Homelab | 429 | 470 | 7 | Existing-target merge; 29,758 inserted rows; exact replay |
| General | 900 | 980 | 34 | Fresh import; 26,804 rows including references; exact replay |

Workshop and Homelab both retain byte-equivalent ordered contents of all 61
authentication, federation and supervisor tables compared with their pre-merge
dumps. General retains no imported authority: its four disabled supervisor
defaults are migration seeds, not copied source grants. The bounded Trust-only
bundle is encrypted to the already established CA recipient; ciphertext SHA-256
`a93e01d3f0dbfaa8c8d9a36be973887180b3276d2b8fbdf35139bb02ff720624`.
Workshop exceeds the portable profile's 256 MiB bound and uses the validated
full-input local reconciliation command instead; that bound was not relaxed.

Fresh coordinated preview backup ciphertext:
`ad4789a829e55dce1ddbb71c31cddcd34fb8a7917fc438da767f76cfef548c81`.
Its isolated restore matches all rows, schema, roles/grants and four sequences
per database, along with identities and usage journals. Signed PDS verification
covers 15 records and 15 audit receipts; accepted/published/cursor positions are
all 15. Recovery evidence is retained under
`/data/tmp/memory-preview-recovery-rc2-20260929`. The first backup attempt failed
because GPG did not trust the existing recipient; its partial artifacts remain.
The retry used a private wrapper accepting only that exact approved fingerprint,
without modifying key identity or trust stores. Both previews are healthy again,
credential maintenance is active and the expired paid worker remains stopped.

Frozen-package local queries against reconciled validation clones return
versioned source references. Workshop's probe returned three answer citations.
Homelab's broad three-term probe correctly reported insufficient evidence;
its exact `SigNoz` lookup returned seven results and one answer citation. This
is workflow evidence, not retrieval-quality evaluation or an R3 result.

Signing, final writer stop/delta, installation, new caller/collection activation
and CA execution remain pending. No original database routing has switched.

### Final packaging and CA inventory reconciliation

Both final Debian-12 packages now pass ABI and offline smoke checks from the
same frozen source. See [the build evidence](rc2-bookworm-build-evidence.md).
The exact frozen plugin and updated CA procedure have separate digest-bound
host-common archives; runtime artifacts were not rebuilt for documentation.
They remain unsigned pending the final Trust delta. The ordinary administrator
preflight verified the existing local PostgreSQL 17/main source, its dedicated
non-superuser owner and database-scoped reader grant. It made no changes. The
prepared cutover preserves those original settings and uses a hard writer fence
before the final dump; default read-only alone is not treated as sufficient.

CA inventory v11, recorded at 22:13:37Z, reports **547 versions / 498 histories**
in the actual Trust database: 537 versions in Homelab and ten in 3DPrinting.
All 547 version rows reportedly match the shared source exactly. This supersedes
the earlier small-partition assumption; it does not mean all those records belong
to Trust. The final ownership ledger must classify these existing copies before
claiming area isolation. Additive reconciliation alone would retain foreign-area
copies. Do not remove or tombstone them implicitly: preserve a verified encrypted
CA backup, exact destination-only authority and provenance, then review a bounded
quarantine disposition for source-identical copies owned elsewhere.

The CA's legacy shared-database singleton remains active according to v11;
its default CLI/TUI already points to the independent Trust service. CURRENT
ACTION v12 requests only the scoped legacy-writer stop, with Trust kept available.
This precedes the final source snapshot and therefore must not depend on delivery
of the signed final-data bundle. No v12 completion report has yet been observed.

Existing Workshop/Homelab containers explicitly override migration mode to
`verify_only` and run non-owner application roles. Their prepared upgrade uses a
separate short-lived, loopback-only schema initialization service with the
existing owner channel, background/model/federation work disabled, and no host
ports. Normal replacements retain `verify_only`; configuration alone cannot
override the existing container environment. This step is prepared, not executed.

Both disposable local-query services have been stopped after verification;
their reconciled databases and evidence remain. Live Workshop and Homelab remain
healthy at rc.1. General's initial three-record SCTP collection has received
preliminary curated-content review only; its exact post-import service draft and
final source hashes must still be reviewed before activation.

Resume verification: the approved Memory checkpoint is still available. The
six frozen software/common artifacts were rehashed successfully. The private
final-delivery assembler passes seven offline tests, including rejection of
unfenced baseline data, changed bytes, wrong recipient/snapshot bindings and
premature CA-readiness claims. It accepts the existing CA-only encrypted/signed
artifact transfer at `10.22.6.42:8444`, not arbitrary plain HTTP destinations.
The final-delta workflow now pins the exact Bookworm release operators rather
than an earlier local build. No unsigned package was installed, no source writer
was stopped, and no source fence or final snapshot was executed in this resume.
