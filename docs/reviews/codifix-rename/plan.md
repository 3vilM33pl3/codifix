# Codifix — Full technical rename and single-user cutover plan

Approved for implementation. Sole user; breaking changes and downtime are acceptable.

Execution direction updated 2026-10-17: deliver directly with one recoverable copy and a focused health/read/write smoke. Reuse completed validation; no new rehearsal campaigns, failover work or all-platform/CA gate for independent deployments. CA changes require explicit approval and SSH stays disabled. Current results and remaining work: [live cutover](live-cutover.md).
Domain: codifix.dev, purchase verified in Namecheap. Candidate 3.0.0-rc.1; stable 3.0.0.
Rename GitHub 3vilM33pl3/memory in place to 3vilM33pl3/codifix.
Archive old federation evidence unchanged and re-enrol existing participating instances.

## Delegation and cost control

Astra coordinates architecture decisions and independently reviews milestones. GPT-6.1 Sol owns engineering, integration, data preservation, credentials and deployment. Luna handles bounded inventories and mechanical documentation. No project-imposed subagent cap; actual runtime capacity still applies. Use one writer per checkout and isolated worktrees for concurrent writers. Serialize the initial workspace-wide crate rename. One engineering integrator owns manifests, lockfiles and cross-cutting constants.

Each worker receives an input commit, owned paths, exact mapping, exclusions and acceptance commands, and returns commits, validation and gaps. Workers must not revert others' changes. Escalate after two bounded unsuccessful attempts; design uncertainty goes to Astra. Keep small incremental commits. One build owner per host; reuse caches and prior 2.1.0 coverage, freeze dependencies, and avoid duplicate full suites or surprise paid evaluation work. Use a reviewed path-scoped transformation with dry-run output; handle JSON/TOML structurally and lockfiles with native tooling. Record real usage where available without inventing costs.

## Identity map and preservation rules

- Product Memory Layer becomes Codifix; executable memory becomes codifix.
- Workspace mem-* crates and directories become codifix-*; Rust imports become codifix_*.
- Six declared binaries and other inventoried runtime launchers receive the codifix brand.
- MEMORY_LAYER_* and MEMORY_LAYER__ settings become CODIFIX_* and CODIFIX__.
- MEMCTL_BIN becomes CODIFIX_BIN with current tokenization semantics preserved.
- .mem becomes .codifix; .agents/memory-layer.toml becomes .agents/codifix.toml.
- .agents/memory-layer-command.json becomes .agents/codifix-command.json and explicitly routes codifix --area workshop.
- memory-* skills, memory-layer plugin/server and branded MCP memory_* tools/prompts become codifix equivalents; memory:// becomes codifix://.
- Product-owned config directories, service units, accounts, cookies and browser storage keys become codifix equivalents.
- Mac app/project/scheme/module become Codifix; app bundle ID dev.codifix.macos. Network Access helper becomes Codifix Network Access under dev.codifix. Credential namespaces become dev.codifix.* while retaining profile/principal binding.
- Packages, formulae and release assets become codifix. Federation dev.memory-layer.unspecced.* becomes dev.codifix.unspecced.*; locator derivation uses https://codifix.dev/federation/record-id/.
- Website https://codifix.dev retains /docs; www redirects to apex and the old website redirects preserving paths.
- Local checkout moves to /home/olivier/Projects/codifix LAST. Update this project's name/slug and repository-root metadata while preserving its UUID and migrating affected path-hash storage mappings.

Preserve canonical memories, provenance, attachments, embeddings, received knowledge, project/knowledge/principal/area-instance UUIDs, Git history, original signed federation evidence and historical release notes. Applied SQL migrations remain byte-for-byte unchanged, including comments. Freeze branded UUID-v5 identities to equivalent explicit UUID constants rather than recomputing renamed seeds.

Semantic memory fields, tables such as memory_entries, /v1/memory, std::mem and third-party names remain. Area names workshop/homelab/general/trust, ports and topology remain. Workshop and disposable dev stay separate. No rolling upgrade, dual namespaces, permanent compatibility aliases, general migration framework or failover system.

## A. Baseline and recovery

- [x] Record clean source/release baseline and Workshop approved-plan checkpoint before implementation.
- [x] Inventory the changed Monolith, Mac, gateway and CA installations, identities, state, credentials and participating peers; retain per-host evidence in the live ledger.
- [x] Record exact old/new state paths and storage keys; verify domain and GitHub target availability.
- [x] Freeze naming map and explicit exceptions.
- [x] Retain recoverable database/configuration/state and old runtime copies for the changed participants; verify the recorded populated restores. Keep secrets out of Git and Notion. This does not claim a simultaneous whole-estate backup.

## B. Core workspace

- [x] Rename crates, imports, dependency declarations, six declared binaries and inventoried launchers; update build scripts and boundary/feature checks.
- [x] Update help, completions, subprocess calls, executable discovery, environment/config/marker/state defaults.
- [x] Preserve durable UUIDs and update lockfiles without unrelated upgrades.
- [x] Compile and run focused checks; commit coherent milestones and obtain Astra review.

## C. Applications, services and installation

- [x] Update Linux services/accounts/paths and Web/TUI branding, browser storage, session and CSRF cookies.
- [x] Rename Swift project, targets, modules, app and Network Access helper; update signing, entitlements and privacy metadata.
- [x] Preserve connection metadata and rehearse Keychain transfer/access BEFORE uninstalling the old Mac app.
- [x] Update Windows MSI identities, registry/install paths, user/server tools; update Homebrew, DEB and all PKG variants.
- [x] Provide a release-specific reinstall procedure; validate fresh installation and transition from populated state.

## D. Agents and project integration

- [x] Rename plugin/marketplace/MCP tools, prompts and resources; update skills, internal references and generated bootstrap copies.
- [x] Update explicit Workshop helper routing without a dev fallback.
- [x] Update the inventoried Monolith/Mac product-owned integrations and installed skills while preserving custom configuration; record uninspected consuming repos as outside this acceptance.
- [x] Migrate this checkout marker/path-hash mapping and inventoried wrappers, schedules and app paths; retain the documented Mac project-path and loaded-session exceptions.
- [x] Validate installed CLI/MCP reads, scoped wrappers, the Workshop helper and separate dev route; do not claim every consuming repository or loaded client session.

## E. Federation reset and preservation

- [x] Change namespaces, signed-purpose strings and record-key derivation together; regenerate synthetic fixtures from test keys and hashes.
- [x] Archive original signed records, receipts, keys and configuration unchanged.
- [x] Implement one release-specific reset against exact reviewed schema/rows/tables; reject schema drift and unknown dependencies. No wildcard deletion or TRUNCATE CASCADE.
- [x] Preserve recorded canonical/received data, identifiers, provenance and retained storage through per-host cutovers; retire old federation work. Reuse populated-state fingerprints without claiming new attachment/vector functional probes.
- [x] Re-enrol the previously participating peers, restore approved subscriptions and retain inactive work; apply only the separately approved human Pull/AskExpertise grant on Workshop codifix.
- [x] Verify fresh collection publication, original citation identity/version/content digest and same-request replay without duplicate execution; do not republish imported knowledge. Retained attachment data is covered by preservation evidence, not a new serving probe.

## F. Documentation and release

- [x] Update current docs, examples, diagrams and assets; preserve historical evidence with explicit exceptions.
- [x] Update release version validators, manifests, signing and artifact inventory for 3.0.0-rc.1 and 3.0.0.
- [x] Cover Linux amd64/arm64, Mac Intel/Apple silicon standard and update packages, Windows ZIP/user MSI/server tools. Windows remains preview.
- [x] Prepare website, redirects and installation URLs; freeze a single candidate source SHA.

## G. Prior validation evidence (no new rehearsal campaign)

- [x] Reuse completed isolated populated restores and per-host cutover checks; the 17 October direct-cutover instruction supersedes a new whole-estate rehearsal.
- [x] Reuse recorded table/schema fingerprints and functional retrieval/history/origin checks; record the absent dedicated attachment/vector probes as limits.
- [x] Reuse focused permission, credential, cookie/CSRF, native client, MCP, marker, watcher and Workshop routing acceptance recorded in the ledger.
- [x] Exercise synthetic federation publication, proof, deduplication and revocation with fake providers; confirm old services/queues stay disabled.
- [x] Reuse recorded restoration and consolidated checks, resolve independent Astra blockers and retain recovery copies; no new coordinated restoration campaign.

## H. Per-host direct cutover

- [x] Quiesce each changed participant as recorded in its cutover procedure and retain its recovery set; do not claim a simultaneous estate-wide outage or snapshot.
- [x] Install prepared artifacts and apply reviewed paths/configuration and scoped database transforms on the changed participants.
- [x] Use the reviewed cold-start/disabled-work boundaries during each cutover; enable only the explicitly approved runtime and federation scopes.
- [x] Bring Workshop up first, update the helper route and validate data/auth/clients.
- [x] Establish fresh federation enrolments and explicit sharing; selectively resume previously active services after queue inspection.
- [x] Rename GitHub repository and switch website/DNS under the revised direct-cutover direction.
- [x] Move checkout after source workers finish; update main-project markers and storage hash mappings. Keep a temporary old-path link for already-running sessions.
- [x] Run focused new-checkout/helper and installed-package health/client checks, plus the final Workshop-to-Trust exchange/replay.

Before new remote federation effects, a matched backup restore is available. After remote effects, a local restore cannot undo publications: stop federation and recover forward or coordinate restoration across affected peers.

## Completion evidence

- [x] Inventoried active branded surfaces use Codifix; documented legacy paths, historical evidence and semantic memory names remain explicit exceptions.
- [x] Recorded fresh-install and populated-cutover checks pass; verified CLI/Web/TUI/Mac/MCP paths and preserved-data reads work within the ledger scope.
- [x] Workshop/dev remain explicit; fresh federation works and retired queues stay retired.
- [x] Versions, source/artifact hashes, commits and scoped acceptance evidence are recorded; no previously inactive work was activated without explicit approval.
- [ ] Record final Notion readback, complete the Workshop plan workflow and remember the implementation; retain backups until explicit deletion approval.


## Current closeout — 19 October 2026

Codifix 3.0.0-rc.1 rename and scoped participating-host cutover are complete.
This reconciles the original checklist with Olivier's 17 October instruction to
finish the single-user direct cutover using completed checks. Superseded broad
rehearsal and universal-client wording is narrowed explicitly above; it is not
represented as executed. [Live cutover](live-cutover.md) records per-host evidence.

Monolith, Mac, gateway and CA Trust are upgraded. The domain/GitHub cutover,
client/bootstrap repair, legacy executable removal and rc.1 publication are
delivered. All 11 candidate jobs passed; published source `66c2d1ec` and all 43
downloaded files remain immutable and byte-verified. Later CI fixes are on main.

The CA-local report confirms fresh Trust activation and collection revision1.
On 19 October the normal Workshop Authentik session completed the deterministic
local-human exchange and exact replay. The original citation identity, version
and content digest match; answer/binding/citations are identical, with exactly
one gateway execution and no provider starts. The separately approved human
project grant contains only Pull and AskExpertise. All three non-CA areas are
healthy at cursor17 with empty source queues. See [Trust closeout](trust-federation-closeout.md).

Stable 3.0.0, signed/notarized distribution, pinned Homebrew, exhaustive consuming
repo/session audits, new attachment/vector functional probes, OS reboot, pixel
UI acceptance and a separate Homelab contribution test are outside the claims
of this prerelease. Existing recovery copies remain retained.

## Current operational boundary

CA is console-administered only; SSH remains disabled and no certificate renewal
is required. The local Trust application upgrade and its required narrow HBA
name-field reload were approved and completed. Fresh same-scope federation
activation was explicitly approved and completed; unrelated CA changes still
require approval. Coordinate through Notion's CURRENT ACTION and RESULT INBOX without
repeating the completed cutover. Current source checks and
private rehearsal evidence are in [integration status](evidence/integration-status.md)
and [federation preservation](evidence/federation-slice.md). Source-level checked
items do not establish native installation, final recovery or live cutover.

## Disposable development scope

Dev/test memories are disposable; durable project knowledge remains in Workshop.
The rename does not require preserving dev rows, OIDs or passwords. At cutover,
stop only the identified project-dev service/processes and retain their current
configuration privately. Leave shared PostgreSQL and unknown legacy consumers
untouched. A fresh dedicated Codifix dev database/login may be provisioned only
through existing, verified database/role-management privileges, then bound to a
separate dev overlay and checked for `status=ok`, `database=up`. The CLI does not
implicitly create these database credentials.

If that authority is unavailable, keep the known dev service stopped and record
fresh dev provisioning as pending; it need not block durable Workshop cutover.
Do not label every old shared catalog object inactive without client inventory,
rename shared roles blindly, change host authentication or route dev to Workshop.
Unresolved shared legacy objects remain explicit pending rename work.
