# Four-area Memory network: approved execution contract

Approved by Olivier on 2026-09-17; subsequent model correction selects OpenAI GPT-5.6 Sol (`gpt-5.6-sol`), not GPT-5.4. Target 16:00 Europe/London internal demo, conditional on safety gates. R3 is paused, not complete.

## Agreed operating model

Four independently running Memory installations with separate databases, credentials, storage and Pi Rust supervisors: Trust (CA knowledge, on the console-only CA); Homelab (infrastructure, on Monolith); Workshop (Memory development and releases, on Monolith); General (other genuine knowledge retaining project boundaries, on Monolith). Trust contains procedures and configuration knowledge, never private keys.

Supervisors inspect their area, discover approved expertise, ask policy-approved questions, find stale knowledge and propose corrections. Homelab automatically installs eligible signed Memory maintenance releases 02:00–03:00 Europe/London after backup/compatibility checks. CA signing changes, broader sharing, credential changes and destructive operations require human approval. Separate supervisor ceiling $10/day fleet, $2.50 and at most 20 model calls per area; deterministic routine health checks. Never use R3 credentials/budget. Use normal approved OpenAI provider with explicit scoped credentials and destinations. No model substitution.

Today's demo: four actual areas, selected real knowledge, discovery, governed questions, visible agents and release notification/scheduling. Messaging and unattended installation are not prerequisites. Deadline never overrides restore/isolation/CA gates; no unlabelled simulated substitutes.

## Architecture

- Separate application/database sandboxes on Monolith; Trust database and sandboxed Pi on CA. Four schemas/projects alone do not satisfy isolation.
- Dedicated company gateway/database/private PDS on Monolith; no participant raw PDS, gateway DB or shared credentials. Approved private ATProto profile, not native Spaces/public Bluesky/direct peer HTTP. Reviewed publications and advertisements use ordered outbox; private Q&A uses addressed mailbox.
- Default deny; enrollment alone grants no knowledge. Exact collection, requester, model and onward destination grants. Preserve original project subdivisions; one federation-facing project per instance initially, with deliberately prepared General exchange summaries.
- Valid TLS, separate private hostnames/cookie scopes and Authentik bindings. Existing DNS is not verified TLS.
- Pin Pi Rust v0.5.1 source/artifact hashes for Linux AMD64/ARM64. Integrate local stdio RPC, not an embedded runtime. Disable inherited skills/extensions/context and unrestricted tools. Only explicitly bound Memory tools and narrow adapters.
- No Pi DB password, signing key, Docker socket, general privileged shell or broad filesystem access. On CA also deny step-ca secrets, OpenBao administration and signing/service controls. Sandbox failure blocks activation.
- Dedicated Rust supervisor runtime with persisted schedules, observations, decisions, attempts, approvals and outcomes; stable IDs, leases and fencing. Not a timer over existing synchronous loops.
- Deterministic checks every five minutes; coalesced event work and one daily stale-knowledge review. Budget admission outside Pi before each provider request, reserve worst-case usage, retain uncertain usage, stop when caps cannot fit.
- Standing-question policy binds requester/responder/collection/purpose/onward destination. Exact generated question and policy revision logged; never fake human consent or weaken F1 human checks.
- Shared status/action contracts and CLI/Web/TUI parity: heartbeat/policy/budget/chat/questions/history/evidence/decisions/results/pause/resume/cancel/approval. Lifecycle observed, proposed, authorized, queued, running, verified plus denied, failed, expired, uncertain.
- Four area cards, plain-language activity and area detail. Fleet view aggregates authorized metadata only. No private chain-of-thought, credentials or unrestricted provider bodies.
- Workshop publishes reviewed structured release notices containing version/revision/digests/architectures/compatibility/signed manifest. Homelab independently verifies using local authority. Imported prose is not executable permission.
- Separate narrow privileged installer; automatic maintenance only for opted-in compatible releases without migrations, trust or permission changes. Other releases require approval.

## Step 0: preservation and checkpoint

- [x] Record R3 paused and preserve artifacts/diagnoses/approvals/budget; no evaluation calls.
- [x] Checkpoint approved plan in dev Memory before implementation.
- [ ] Inventory all writers, database bindings, watchers, offline stores, configuration/credential locations on Monolith and via CA console.
- [ ] Fresh encrypted backup of databases/evidence/offline stores/configuration/journals/required identities with separately protected credentials.
- [ ] Isolated restore with workers/watchers/federation/provider calls disabled; verify contents and startup.
- [ ] Protect migration backup from seven-day cleanup. Existing approximately 210 MB dump compression test is not restore evidence.

## Step 1: lossless, privacy-preserving partition

- [ ] Versioned migration manifest assigns every canonical lineage/supporting record to owner or protected archive.
- [ ] Classify actual 105-project/3,367-version/2,244-capture baseline using source/project/content, not names alone. CA material in homelab moves to Trust; Memory to Workshop; `ca-2` is not assumed Certificate Authority.
- [ ] Keep canonical histories/tombstones together; preserve IDs/authorship/timestamps/provenance; explicitly journal project reassignment.
- [ ] Preserve General project boundaries. Test/eval/ambiguous sensitive data retained in operator-only non-retrievable archive.
- [ ] Mixed captures/sessions remain in protected archive; targets use provenance references or reviewed excerpts without unrelated private data.
- [ ] Table-by-table policy: migrate owned data, rebuild derived data, retain historical operational evidence, explicitly retire runtime state. Never reactivate old jobs/tokens/federation identities.
- [ ] Reconcile counts, ID/hash inventories, references and unresolved records; no unexplained loss/duplicate ownership/dangling references. Use schema-aware migration, not lossy bundles.

## Step 2: v2.1.0-rc.1 four-area foundation

- [ ] Independent instance services with pinned executable/config/data paths; installing packages must not call restart-all or disturb development/unrelated instances.
- [ ] Package migration tooling, gateway, deployment manifests and CA console instructions.
- [ ] Immutable AMD64/ARM64 artifacts, checksums, signed manifest, source revision, release notes; never publish private data/secrets.
- [ ] Import clean targets and verify partitions with all autonomous/model/watcher work disabled.
- [ ] Enroll four instances; review small useful collections; prove allowed questions and denials.
- [ ] Fence/stop old writers, final backup/repartition/reconcile, switch clients, retain original read-only rollback source.

## Step 3: v2.1.0-rc.2 supervised preview

- [ ] Pi integration, durable work, standing-question policies and hard budget admission.
- [ ] Web/TUI supervisor interaction, visible actions, approvals and pause controls.
- [ ] Enable deterministic health and scoped model work.
- [ ] Real Workshop release notice; Homelab receipt/compatibility/scheduling decision.
- [ ] Source change invalidates collection; reviewed revision restores eligibility.
- [ ] Complete 16:00 demo. Reassess gates at 14:00; preserve verified state and label incomplete work if time is insufficient. No hidden fake agent/Trust host substitution.

## Step 4: v2.1.0-rc.3 automatic maintenance; then v2.1.0

- [ ] Privileged allowlisted installer and instance-specific fencing.
- [ ] Fresh backup, supported architecture/schema, signature and idle-work checks.
- [ ] Stage, stop selected instance only, install, restart, verify version/DB/auth/federation catch-up.
- [ ] Reconcile ambiguous/interrupted effects before retry; never falsely claim external rollback.
- [ ] Real upgrade in explicitly selected daytime test window before nightly automation.
- [ ] No automatic DB downgrade; binary rollback only with proven compatible schema, otherwise operator recovery.
- [ ] Stable promotion after overnight cycle, restart/recovery tests and restore proof.
- [ ] Document later optional messaging release; today Web/TUI human communication only. Later recipient/disclosure policies and receipts; messages alone never authorize installs/CA actions.

## CA handoff pack (new installer stages, not existing commands)

- [ ] Tested ARM64 pinned installer/manifest/migration archive/exact commands/expected output/recovery.
- [ ] Console preflight: architecture/OS/disk/Memory version/services/DB schema/connectivity without secrets. Detect shared DB vs unique local CA data; reconcile unique records.
- [ ] Verify signatures against independently supplied release key and every required hash; no curl-pipe-shell/latest.
- [ ] Back up CA Memory locally and encrypted off-host; no signing bootstrap/rotation/export.
- [ ] Stopped-service install preserves configuration/migration history, provisions restricted Trust database/identities/Pi state, no workers before import.
- [ ] OpenBao console configuration through explicit local Ansible connection scoped to Memory; no CA bootstrap or privileged CA credentials in Pi.
- [ ] Idempotent Trust import/count/hash/reference verification and negative cross-area reads.
- [ ] Start non-autonomous Trust, authenticated TLS Web access and outbound gateway enrollment; DB/Pi RPC unexposed, SSH remains disabled.
- [ ] Enable supervisor only after sandbox/model/budget/collection tests; verify step-ca/cert distribution/OpenBao unchanged and healthy.
- [ ] Redacted verification report. On failure stop new Trust and preserve evidence; recover old Memory only through compatible recovery, leave signing services untouched. Olivier performs console/interactive sudo, no password in chat.

## Verification and demo gates

- [ ] DB migration history/tombstone/mixed-provenance/cross-area/interruption/idempotency tests.
- [ ] Isolation across DB/files/credentials/unreviewed data; no leakage through citation/session/model/feed/export.
- [ ] Fake-provider injection/budget/revocation/malformed/uncertain-usage cases.
- [ ] Restart/concurrent claims/expired leases/duplicate events/interrupted effects.
- [ ] Reject invalid signatures/incompatible schema/wrong architecture/stale approvals.
- [ ] Existing federation/PDS/isolation regressions, relevant Rust/Lean conformance, fmt/Clippy/boundaries/Web tests/build.
- [ ] Real Web/TUI human questions/denials/pause/resume/failure visibility.
- [ ] Independent Trust and Monolith-area recovery without stale writer authority.
- [ ] Demo actual four running areas; Homelab discovers Workshop and cites release answer; restricted request denied; real release notice and installation decision; human stale-knowledge question; restart preserves work/history.

Completion is verified internal deployment, not customer readiness or evaluation success. R3 remains paused. General delegation/public federation/autonomous CA administration are outside this series. Small incremental commits and truthful evidence throughout.
