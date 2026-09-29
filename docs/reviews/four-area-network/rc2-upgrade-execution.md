# Upgrade every Memory installation and complete the four-area setup

Approved by Olivier on 2026-09-29. Target: one coherent internal Memory Layer
2.1.0-rc.2 release from the current federation branch. Workshop, Homelab and
General run independently on Monolith; Trust runs locally on the CA. Preserve
schema-v2 history, schema-v3 private federation and current PDS/Atrium/Pi pins.
R3 stays paused. No public access, unrestricted sharing or new paid execution.

## Release and installation

- [ ] Inventory active/dormant client, service, watcher, MCP, plugin and skill launch paths on both hosts; preserve historical and disposable installations.
- [ ] Build coherent AMD64 and Debian-12-compatible ARM64 binary/Web/operator/skill artifacts from one frozen commit; refresh skills from that exact bundle.
- [ ] Implement a journaled selected-instance upgrade: signed inputs, backups, selected service stop, executable/Web switch, compatible append-only migration and health verification; preserve CA sandbox drop-ins.
- [ ] Sign the release using the established identity and normal local unlock, with explicit schema compatibility and no database downgrade.

## Complete dataset review and reconciliation

- [ ] Encrypt fresh source/area/gateway/PDS/runtime backups and verify isolated restoration including sequences, identities, receipts and usage.
- [ ] Inventory all source items and reuse ownership decisions only when full-history and dependency hashes match.
- [ ] Review redacted captures, sessions, replacement/loop proposals, protected histories, relations and derived gaps; give every item an import, pending-work, protected-archive or retired-runtime disposition. Secrets remain local.
- [ ] Preserve IDs, versions, tombstones, authorship and provenance; archive mixed sensitive material and cross-area relations without implicit disclosure grants.
- [ ] Implement a separate existing-target reconciliation operator with source/decision/target hashes, stopped writers, durable operation receipts, identical-record deduplication, compatible history append and explicit conflicts; retain destination-only records and all live authority/budget/federation state.
- [ ] Complete initial review live and final-delta review with old writers fenced; account for every item before routing switches.

## Routing and federation

- [ ] Implement operator-owned area routing shared by CLI/TUI, launchers and watcher manager: explicit area, exact repository/project mapping, then General; unavailable areas never fall back elsewhere.
- [ ] Bind watchers and offline outboxes to area; preserve replay IDs and prevent duplicate watchers.
- [ ] Route this repository to Workshop, infrastructure to Homelab, CA-local work to Trust, remaining projects to General; distinguish same-name projects by area and identity.
- [ ] Keep Trust service CA-local and access remote expertise through the gateway; retain Trust-bound writes for CA reconciliation.
- [ ] Extend federation to explicit per-project bindings, retaining single-project configuration and one ordered stream per instance/space; separate project authority and answerer grants.
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
