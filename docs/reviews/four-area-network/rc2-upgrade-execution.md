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
