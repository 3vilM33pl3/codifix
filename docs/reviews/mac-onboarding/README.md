# Mac onboarding: sign in, select projects, start working

Approved by Olivier on 2026-10-05. This is the approved implementation contract,
with execution checkboxes added for the mandatory Memory completion workflow.

## 1. Architecture and intended experience

Build this in two increments: company administration first, then the Mac installer.
The journey is install Memory, sign in, select authorized projects, clone
repositories, install approved skills/plugins and synchronize approved knowledge.

Authentik owns human accounts/login/MFA; Memory owns project membership/device
permissions/onboarding; the private ATProto profile carries approved knowledge
exchange; the Git provider authorizes cloning/pushing; Codex and Claude execute
using project tools and disclosure policies. ATProto does not replace human or
Git administration. Reuse existing integrations, Access UI and macOS packaging.

Chosen defaults: one company console on Homelab; approved Workshop/Homelab/General
knowledge offline; Trust remains CA-local through approved remote expertise;
configure Codex and Claude; preserve the private federation profile and pins.

## 2. Administration

- [ ] Users: Authentik create/invite/disable/manage with narrow server-side credentials, no new password store.
- [ ] Stable pre-login assignments, effective access including groups, independently revocable devices, user disablement and separate human/federation/disclosure authority.
- [ ] Projects: reviewed manifests bind area/project identity, owner instance, repository/branch, offline permission, pinned skills/plugins/hashes, tools and permitted destinations.
- [ ] Trust appears as remote expertise without CA copies or local administration.

## 3. Login, selective synchronization and installer

- [ ] CLI auth login/logout, project selection and synchronization status with Web/TUI equivalents; browser-assisted per-device login and Keychain credentials.
- [ ] Architecture-matched package; dedicated loopback PostgreSQL/pgvector with separate area databases and named LaunchAgents, no unrelated restarts or PDS/gateway/server database credentials.
- [ ] Server-authorized project snapshot/delta feed selects before transmission and preserves approved memories/evidence, versions, authorship, provenance, digests and tombstones; no raw captures/sessions/secrets/unapproved material.
- [ ] Searchable read-only remote projection; local captures/proposals and offline queues retain origin ownership and recheck current permission before replay.
- [ ] Atomic update/cursor commits, harmless replay, startup/reconnect/five-minute sync, visible freshness/pending/failures, withdrawal and revocation semantics.
- [ ] Gateway-attested scoped delivery verifies digests without downloading whole unauthorized CAR/repositories or claiming complete repository verification.
- [ ] Clone with user Git credentials; preserve existing/dirty repositories; confirmed pinned tool installation without arbitrary setup hooks; merge only Memory-owned settings.
- [ ] Separate Codex/OpenAI and Claude/Anthropic identities/disclosure policies; resumable installation journal.

## 4. Agent configuration and optional API

- [ ] Sol primary coding/high effort; Astra read-only adviser/reviewer/high effort; maximum two concurrent subagents, default Sol/medium; bounded installer/verifier/security/docs roles as needed.
- [ ] Update older engineering model guidance and provide exact custom-agent/default configuration; preserve Claude's independent models and credentials.
- [ ] No dependency on an unverified Decision API. Optional Agents SDK explicit per-agent models or separate Responses calls; authorization remains deterministic; no new paid API execution.

## 5. Delivery and acceptance

Small committed slices: checkpoint/admin/catalog, device login/project selection,
authorized snapshots/offline search, installer/repository/agent setup, clean-Mac
rehearsal and instructions.

- [ ] Synthetic-account/project tests prove unauthorized bytes are never sent and same-named projects remain distinct.
- [ ] Interrupted installation/sync, duplicate delivery, withdrawal and device/user revocation tests.
- [ ] Existing repositories/custom settings/services preserved; Trust remote-only verified.
- [ ] Relevant Rust/database/Web/packaging validation and observed real-Mac login/select/clone/setup/offline-citation/reconnect journey; cross-compilation is not macOS acceptance.

Completion requires an observed macOS journey. Windows, R3, autonomous
supervisors and public release remain separate. No existing installations are
changed by this development work.

