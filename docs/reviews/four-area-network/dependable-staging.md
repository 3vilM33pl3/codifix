# Make the preview dependable and stage the four-area dataset

Approved by Olivier on 2026-09-19. This is staging, not cutover. Keep original
writers and Workshop/Homelab previews operational; do not install Trust on the
CA. R3 remains paused. No public DNS or existing CA changes. No paid verification
calls. The authoritative source is the shared original database, not a merger
of previews; live counts must be refreshed for the actual snapshot.

## Dependable credentials and operating limits

- [x] Add explicit refresh-only maintenance for the existing enrolled PDS account, separate from time-limited enrollment approval; no new accounts, identity/trust changes or password fallback.
- [x] Check every minute, refresh 15 minutes before expiry, serialize operations and reconcile interrupted refreshes durably. Five transient retries start at ten seconds and cap at ten minutes; invalid credentials/identity or exhausted recovery require intervention.
- [x] Keep refresh credentials outside gateway/participants. Add opt-in atomic access-only credential bundles, dedicated directory mounts, ownership/permission/identity/expiry validation, immutable per-attempt credentials, and preserve static-token configuration.
- [x] Distinguish authentication from invalid proof failures, stop claims without usable credentials, and resume only authentication-blocked publications after verified renewal; preserve IDs, ordering, proof verification and ambiguous read-back.
- [x] Expose authorized sanitized credential/grant health in gateway and Web/CLI/TUI: expiry, renewal, retry/intervention state, observation time; stale/unknown is not healthy. Add TLS expiry warnings, without automatic issuer/CA changes.
- [x] Renew Workshop's existing three-version immutable grant for seven days with gpt-5.6-sol, 20 calls and $2.50 per UTC day. Preserve prior/uncertain usage, coordinate service/worker pins, reject silent old-request rebinding, require explicit new-question confirmation, test with fake providers.

## Fresh backup and complete ownership review

- [x] Before operational changes, make fresh encrypted source and two-area deployment backups; preserve older artifacts.
- [x] Restore in isolated environments without production connections/provider credentials/workers. Verify schemas, rows, sequences and identity/configuration material, not only checksums.
- [x] Generalize snapshot tools to digest-bound restore descriptors instead of a fixed September 17 container, preserving isolation and schema allowlists and rejecting live DB targets. Use a new private timestamped staging directory.
- [x] Build a snapshot/full-history-bound versioned ownership ledger: Trust for authoritative CA knowledge; Homelab for infrastructure/networking/deployment; Workshop for Memory development/architecture/releases; General for the remainder retaining project boundaries.
- [x] Review redacted curated content/provenance only, recording basis/reviewer/hashes. Agent review is not Olivier approval. Secrets, raw captures and session bodies must not be disclosed. Uncertain/mixed/sensitive/unsupported material stays in a protected archive with explicit reasons.
- [x] Preserve IDs, complete histories, tombstones, authorship, timestamps and provenance; mixed raw captures/sessions stay local and no inferred excerpts are published. Preserve same-area relations and archive cross-area ones without invented sharing permission.
- [x] Preserve proposals, uncurated captures and derived-data gaps as an actionable backlog, not automatic approval/curation. Account for every source item as imported, protected archive or deliberately retired runtime state, without calling archive contents searchable migration.

## Four inactive staging databases

- [x] Prepare four fresh independent targets with the existing importer; import only reviewed ownership and evidence projections. Do not copy authentication/jobs/federation enrollment/supervisor authority. Disable watchers/enrichment/models/federation.
- [ ] Verify all imported rows, receipts, histories and relations against manifests; demonstrate harmless exact replay and changed-input rejection. Use disposable validation clones when app startup would activate import targets.
- [ ] Produce counts, protected exclusions, limitations and verification hashes for every area.
- [x] Add delta/revalidation: reuse decisions only when full-history and dependency hashes match; new/changed material needs review while original writers continue.
- [x] Prepare read-only CA-console preflight/handoff identifying compatibility and CA-only/offline data gaps. Staging Trust on Monolith is not CA migration.

## Verification and delivery

- [x] Test expired credentials, interrupted/ambiguous refreshes, competing maintainers, atomic replacement during publication and selective recovery that cannot unblock invalid proofs.
- [ ] Test grant expiry, retained daily budgets and old-profile rejection; full source accounting/mixed-content protection/no cross-area leakage; replay/changed source/delta detection.
- [x] Prove isolated recovery of sequences, identities, signed repositories and consistent publication state; rerun cited two-way exchange, access denials and restart recovery.
- [ ] Run relevant Rust/database tests, formatting, Clippy, boundaries, Web tests/build and CLI/TUI workflows.
- [ ] Commit small slices; update execution evidence and future-cutover runbook; verify this checklist before claiming completion.

Completion requires tested preview credential recovery, the bounded renewed
Workshop grant, four verified inactive partitions, and restore/cutover
instructions. It does not include writer cutover, General/Trust activation, CA
modification, autonomous agency, evaluation completion or customer readiness.

## Execution evidence

- Approved-plan dev checkpoint: `2026-09-19T22:00:51Z`, starting at `b13a0a7c`.
- Plan memory: `33c06fa3-f3d7-444e-8be1-e461a7a09f27`; 23 completion items.
- Raw capture: `761dc6a4-64fe-4eb4-bbbb-38dc4541f491`.
- CLI dev credentials initially returned 401; the existing credential already
  accepted by the dev service was reused for the checkpoint without rotating
  tokens or changing configuration. The failed local-only checkpoint is not
  represented as a successful server event.
- Implementation and verification are in progress. No completion gate is yet
  claimed for this increment.

### Recovery and first maintenance evidence

The fresh source baseline is `baseline-20260919T2202`, independently copied back
from NAS into a new network-none restore. Snapshot
`2154cc37-c3cd-4784-b335-37062e6d240d` contains 104 tables and 2,587,041 rows;
all table row hashes and all three sequence states match the encrypted archive.
It includes 107 projects, 3,482 memory versions and 3,207 canonical histories.
The plan checkpoint accounts for a new memory since the initial scope count.
The normalized complete inventory digest is
`997ecdf64b7490e3a104a5d4c9efa01b88ab77dfb85fa47fad04d195b1770251`.

The separate coordinated preview archive
`dependable-backup-20260919t2212.tar.gpg` has SHA-256
`c0ad3dbe68367de400e9881af2745fd5df462cb1ef28a60f4a5f4e65d73b772d`.
Its isolated recovery proves three databases' schemas, rows, sequences,
roles/grants and permissions; four PDS SQLite integrity checks; ten signed
repository records and fourteen CAR blocks; enrolled-key verification with a
negative bootstrap-key probe; ten audit receipts and their chain; retained
sequence/cursors at ten and four saved answers. The recovery starts no Memory,
PDS or provider process. See [the recovery operator](../../../deploy/federation/PREVIEW-RECOVERY.md).
This is offline recovery evidence, not customer disaster-recovery certification.

Existing-account refresh-only adoption was authorized under the approved plan
with review digest
`d743780c58a8bbb8410880298eb70b7d69af4db1e8c73e0cdcd3586f2d6d4c34`.
Its first real refresh succeeded at `2026-09-19T22:27:32Z`, yielding generation
two without password fallback, account creation or trust-key changes. The
maintainer has a private operator-edge network and access-only output directory;
no gateway, participant, database or provider credential mounts. Its user service
is `memory-pds-refresh-only.service`. This evidence predates the gateway/app
rollout; generation two alone does not prove publication recovery.

Four fresh targets received independently assigned identities and schema-only
head 60 under `memory-four-area-staging-20260919T2350`. They have no network,
published ports, application processes, copied authentication or background
workers. All four imports and exact replays have subsequently succeeded;
independent database and application-clone verification remain separate gates
in the [staging evidence report](staging-evidence.md).

The first full-history export exposed a scaling defect: repeating 468,219
relations in every incident history made the private metadata index too large.
Its failed artifacts remain protected. The verified v2 pack uses compact,
digest-bound dependency metadata. Representation-only rebinding checked exact
version, source and project equality; it did not reuse changed content decisions.

### Preview rollout and ownership review

The new gateway, edge and both applications were rolled out from reviewed,
hash-bound replacement contracts. Old containers remain stopped and disconnected
for recovery; the original source writer was untouched. The gateway mounts only
the dedicated access-bundle directory, not the maintainer's refresh credential.

Workshop's immutable replacement grant expires `2026-09-26T22:42:47.500Z`.
Manifest SHA-256:
`14ad9f571699ff391580864151090111c0929eda0b54580e3e42230c2b41e306`.
It preserves the same three source versions, model and daily ceilings. At the
renewal check the UTC-day budget still recorded one call and $0.008172 charged;
the renewal did not reset usage. Service and worker pins were coordinated, and
old-profile requests were rejected in offline verification.

Fresh deterministic questions in both directions were answered with exact
reviewed citations at `2026-09-19T23:08:58Z`. Requests
`be4bf2dd-d4b0-4067-9d28-02fcd39cd416` and
`2dae7006-217a-4024-8da6-200381c5ff55` retained identical answer hashes after
restarting the gateway, both applications and the refresh maintainer. The
restart receipt at `23:18:31Z` confirms sequence `10:10`, unchanged paid-attempt,
settlement and budget journals, and unchanged historical actions. Two new
verified deterministic health observations were expected. The initial overly
broad comparison treated these as an error; reconciliation checked their type
and restored the paused question worker without reissuing work. No paid
verification calls occurred. Unauthenticated/fake-token gateway access returned
401; raw repository/configuration bypass probes returned 404.

Both Web interfaces displayed current credential health after restart. The
focused federation TUI displayed expiry, renewal and TLS metadata; the main
Supervisor TUI independently displayed the renewed grant, model and limits.
These observations establish bounded preview operation, not autonomous agency
or proof of future availability.

At `2026-09-20T00:13:24Z`, the minute-based maintainer performed its next
scheduled refresh automatically. Generation three expires at `02:13:24Z`.
Both authorized CLI status reads and both live Web pages displayed the renewed
generation with no blocked intents or synchronization failures. The gateway
container remained `fc190b691a82a497229a89a25c061d62211c8567eb1e6eb86d53ff6c818989f6`,
started at `2026-09-19T23:10:53.454822249Z`: this renewal was accepted without a
gateway restart. No forced refresh, identity change or paid question was used.
Private metadata evidence is `final-preview-health.json` in the staging folder.

The completed ownership ledger accounts for all 3,207 histories: Workshop 1,536,
Homelab 393, Trust 19, General 742, protected archive 517. Six explicit annotation
sets retain the actual reviewing agent per decision. Four proposed imports were
held by the importer compatibility check, without a lossy provenance fallback.
Sensitive and ambiguous histories remain protected. Original evidence bodies
are not approved: selected sources become opaque archive-reference projections.
An unchanged-snapshot delta check reused all 3,207 decisions; synthetic changed
history/dependency tests require renewed review. This does not capture continued
source writes.

The read-only [CA handoff](../../../deploy/areas/CA-CONSOLE-HANDOFF.md) and
[future cutover runbook](staging-cutover.md) explicitly leave CA-only/offline
coverage unresolved. The private handoff package was prepared but not delivered
to or executed on the CA.
