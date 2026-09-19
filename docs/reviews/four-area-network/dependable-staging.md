# Make the preview dependable and stage the four-area dataset

Approved by Olivier on 2026-09-19. This is staging, not cutover. Keep original
writers and Workshop/Homelab previews operational; do not install Trust on the
CA. R3 remains paused. No public DNS or existing CA changes. No paid verification
calls. The authoritative source is the shared original database, not a merger
of previews; live counts must be refreshed for the actual snapshot.

## Dependable credentials and operating limits

- [ ] Add explicit refresh-only maintenance for the existing enrolled PDS account, separate from time-limited enrollment approval; no new accounts, identity/trust changes or password fallback.
- [ ] Check every minute, refresh 15 minutes before expiry, serialize operations and reconcile interrupted refreshes durably. Five transient retries start at ten seconds and cap at ten minutes; invalid credentials/identity or exhausted recovery require intervention.
- [ ] Keep refresh credentials outside gateway/participants. Add opt-in atomic access-only credential bundles, dedicated directory mounts, ownership/permission/identity/expiry validation, immutable per-attempt credentials, and preserve static-token configuration.
- [ ] Distinguish authentication from invalid proof failures, stop claims without usable credentials, and resume only authentication-blocked publications after verified renewal; preserve IDs, ordering, proof verification and ambiguous read-back.
- [ ] Expose authorized sanitized credential/grant health in gateway and Web/CLI/TUI: expiry, renewal, retry/intervention state, observation time; stale/unknown is not healthy. Add TLS expiry warnings, without automatic issuer/CA changes.
- [ ] Renew Workshop's existing three-version immutable grant for seven days with gpt-5.6-sol, 20 calls and $2.50 per UTC day. Preserve prior/uncertain usage, coordinate service/worker pins, reject silent old-request rebinding, require explicit new-question confirmation, test with fake providers.

## Fresh backup and complete ownership review

- [ ] Before operational changes, make fresh encrypted source and two-area deployment backups; preserve older artifacts.
- [ ] Restore in isolated environments without production connections/provider credentials/workers. Verify schemas, rows, sequences and identity/configuration material, not only checksums.
- [ ] Generalize snapshot tools to digest-bound restore descriptors instead of a fixed September 17 container, preserving isolation and schema allowlists and rejecting live DB targets. Use a new private timestamped staging directory.
- [ ] Build a snapshot/full-history-bound versioned ownership ledger: Trust for authoritative CA knowledge; Homelab for infrastructure/networking/deployment; Workshop for Memory development/architecture/releases; General for the remainder retaining project boundaries.
- [ ] Review redacted curated content/provenance only, recording basis/reviewer/hashes. Agent review is not Olivier approval. Secrets, raw captures and session bodies must not be disclosed. Uncertain/mixed/sensitive/unsupported material stays in a protected archive with explicit reasons.
- [ ] Preserve IDs, complete histories, tombstones, authorship, timestamps and provenance; mixed raw captures/sessions stay local and no inferred excerpts are published. Preserve same-area relations and archive cross-area ones without invented sharing permission.
- [ ] Preserve proposals, uncurated captures and derived-data gaps as an actionable backlog, not automatic approval/curation. Account for every source item as imported, protected archive or deliberately retired runtime state, without calling archive contents searchable migration.

## Four inactive staging databases

- [ ] Prepare four fresh independent targets with the existing importer; import only reviewed ownership and evidence projections. Do not copy authentication/jobs/federation enrollment/supervisor authority. Disable watchers/enrichment/models/federation.
- [ ] Verify all imported rows, receipts, histories and relations against manifests; demonstrate harmless exact replay and changed-input rejection. Use disposable validation clones when app startup would activate import targets.
- [ ] Produce counts, protected exclusions, limitations and verification hashes for every area.
- [ ] Add delta/revalidation: reuse decisions only when full-history and dependency hashes match; new/changed material needs review while original writers continue.
- [ ] Prepare read-only CA-console preflight/handoff identifying compatibility and CA-only/offline data gaps. Staging Trust on Monolith is not CA migration.

## Verification and delivery

- [ ] Test expired credentials, interrupted/ambiguous refreshes, competing maintainers, atomic replacement during publication and selective recovery that cannot unblock invalid proofs.
- [ ] Test grant expiry, retained daily budgets and old-profile rejection; full source accounting/mixed-content protection/no cross-area leakage; replay/changed source/delta detection.
- [ ] Prove isolated recovery of sequences, identities, signed repositories and consistent publication state; rerun cited two-way exchange, access denials and restart recovery.
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
