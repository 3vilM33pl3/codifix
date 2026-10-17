# Codifix — Full technical rename and single-user cutover plan

Approved for implementation. Sole user; breaking changes and downtime are acceptable.
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
- [ ] Inventory actual live installations, services, watchers, databases, offline stores, credentials, agent configs, consuming project markers and participating peers. Treat Monolith/Mac/CA docs only as leads.
- [ ] Record exact old/new state paths and storage keys; verify domain and GitHub target availability.
- [ ] Freeze naming map and explicit exceptions.
- [ ] Prepare a matched recovery bundle for every changed stateful participant: databases, configuration, attachments, keys, old binaries/packages. Keep secrets out of Git, Notion and logs.

## B. Core workspace

- [x] Rename crates, imports, dependency declarations, six declared binaries and inventoried launchers; update build scripts and boundary/feature checks.
- [x] Update help, completions, subprocess calls, executable discovery, environment/config/marker/state defaults.
- [x] Preserve durable UUIDs and update lockfiles without unrelated upgrades.
- [x] Compile and run focused checks; commit coherent milestones and obtain Astra review.

## C. Applications, services and installation

- [ ] Update Linux services/accounts/paths and Web/TUI branding, browser storage, session and CSRF cookies.
- [x] Rename Swift project, targets, modules, app and Network Access helper; update signing, entitlements and privacy metadata.
- [ ] Preserve connection metadata and rehearse Keychain transfer/access BEFORE uninstalling the old Mac app.
- [ ] Update Windows MSI identities, registry/install paths, user/server tools; update Homebrew, DEB and all PKG variants.
- [ ] Provide a release-specific reinstall procedure; validate fresh installation and transition from populated state.

## D. Agents and project integration

- [ ] Rename plugin/marketplace/MCP tools, prompts and resources; update skills, internal references and generated bootstrap copies.
- [ ] Update explicit Workshop helper routing without a dev fallback.
- [ ] Update product-owned sections in consuming repos while preserving user custom configuration.
- [ ] Migrate markers/path-hash state mappings, wrappers, schedules and saved application paths.
- [ ] Validate real MCP clients and fresh sessions, including Workshop/dev separation.

## E. Federation reset and preservation

- [x] Change namespaces, signed-purpose strings and record-key derivation together; regenerate synthetic fixtures from test keys and hashes.
- [ ] Archive original signed records, receipts, keys and configuration unchanged.
- [x] Implement one release-specific reset against exact reviewed schema/rows/tables; reject schema drift and unknown dependencies. No wildcard deletion or TRUNCATE CASCADE.
- [ ] Preserve canonical/received knowledge, identifiers, provenance, attachments, embeddings and area identities; retire old queues/enrolments/subscriptions so they cannot restart.
- [ ] Re-enrol only previously participating peers; retain inactive status and explicitly restore approved subscriptions/permissions.
- [ ] Validate original-origin citations, attachments, deduplication and fresh publication; do not automatically republish imported knowledge.

## F. Documentation and release

- [ ] Update current docs, examples, diagrams and assets; preserve historical evidence with explicit exceptions.
- [x] Update release version validators, manifests, signing and artifact inventory for 3.0.0-rc.1 and 3.0.0.
- [ ] Cover Linux amd64/arm64, Mac Intel/Apple silicon standard and update packages, Windows ZIP/user MSI/server tools. Windows remains preview.
- [ ] Prepare website, redirects and installation URLs; freeze a single candidate source SHA.

## G. Rehearsal and independent review

- [ ] Restore an isolated production snapshot and rehearse the complete cutover.
- [ ] Compare IDs, versions, content digests, relationships, provenance, attachments and embeddings; exercise retrieval, history and imported-source resolution.
- [ ] Validate permissions, credentials, cookies/CSRF, native clients, MCP, markers, watchers and Workshop routing.
- [ ] Exercise synthetic federation publication, proof, deduplication and revocation with fake providers; confirm old services/queues stay disabled.
- [ ] Rehearse matched-bundle restoration, run consolidated checks and resolve independent Astra blockers.

## H. Single-window live cutover

- [ ] Stop watchers, capture, supervisors, federation and queued/inflight work; stop other writers and take final consistent recovery bundle.
- [ ] Install prepared artifacts and apply rehearsed paths/configuration/scoped database transforms.
- [ ] Start database/apps with background, paid-provider and federation execution OFF.
- [ ] Bring Workshop up first, update the helper route and validate data/auth/clients.
- [ ] Establish fresh federation enrolments and explicit sharing; selectively resume previously active services after queue inspection.
- [ ] Rename GitHub repository and switch website/DNS late, after candidate gates.
- [ ] Quiesce agents, integrate worktrees, then move checkout LAST; update saved paths and storage hash mappings.
- [ ] Run final smoke tests from the new checkout and installed packages.

Before new remote federation effects, a matched backup restore is available. After remote effects, a local restore cannot undo publications: stop federation and recover forward or coordinate restoration across affected peers.

## Completion evidence

- [ ] Active branded references use Codifix and remaining legacy references are explicitly allowlisted.
- [ ] Fresh installs and populated cutover pass; preserved data is functional; CLI/Web/TUI/Mac/agents connect.
- [ ] Workshop/dev remain explicit; fresh federation works and retired queues stay retired.
- [ ] Versions, source/artifact hashes, commits and validation evidence are recorded; inactive services were not activated.
- [ ] Update Notion status, finish the Workshop plan workflow and remember the implementation. Keep backups until explicit deletion approval.


## Current operational boundary

CA is console-administered only; SSH remains disabled and no certificate renewal
is required. Ask Olivier before any CA change. The
[read-only console inventory](ca-console-checks.md) has been received and its
report checksum verified; process/database binding, recovery coverage and restore
validation remain pending. Coordinate remaining observations through Notion's
CURRENT ACTION and RESULT INBOX. Current source checks and
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
