# Stage C: durable source-to-PDS-to-replica integration

This completes the software integration missing from the earlier
[durability foundation](2026-09-08-c-durability.md), following the
[C completion contract](c-completion-contract.md). It does not complete D/E,
the separate R1 post-curation queue, or any customer release gate.

## What now runs

- `mem-federation-store::sharing` persists explicit local grants, exact reviews
  and standing stream subscriptions. Migrations 0034/0035 connect existing memory,
  canonical-state and tag mutations through deferred, transactional source capture.
  The source stays usable during disconnection; no stream is enrolled by default.
- Preparation assigns independent shared-content/state counters with immutable
  envelopes and causal dependencies. Evidence remains exactly the reviewed
  selection. Review or source transaction rollback leaves no publication work.
- `mem-atproto::publication` encodes all eight schema-v2 records in the separate
  unpublished publication collection, preserving nested unknown extensions and
  keeping neutral envelope digests distinct from DAG-CBOR CIDs. Record UUIDs are
  scoped to publisher and space; the legacy six Lexicons remain unchanged.
- `mem-federation-runtime` accepts authenticated instance publications at one
  gateway, validates them before committing order/reducer/outbox atomically,
  creates immutable PDS records and verifies exact readback, then serves only the
  contiguous published prefix to currently authorized members.
- Source/receiver scheduling persists receipts, failures, backoff and blocked
  work. Local grants are checked again for prepared records; a current local role
  cannot substitute for a remote capability. Inbox/projection/cursor commits are
  atomic. Full paginated replay verifies the already trusted prefix before swap.
- `mem-service` has explicit opt-in configuration and owns task shutdown/reload.
  It resolves current principal/project permissions using the existing service
  authentication policy. The default still starts no federation workers.

These are trusted service APIs plus opt-in workers, not review or membership UI.
See [configuration and status](../../user/federation-runtime.md) for exact scope.

## Executed evidence

All database verification uses disposable PostgreSQL 16/pgvector data, not the
developer's Memory database. The C workflow creates **three separate databases**:
origin, gateway and receiver, with distinct local principals and explicit grants.

`crates/mem-federation-runtime/tests/c_workflow.rs` exercises actual
`mem_curate::apply_validation_revision` and automatic capture, explicit review,
queued versus accepted versus published versus replicated state, source rollback,
gateway disconnect/restart and writer rotation, duplicate/tampered/reordered
delivery, local permission removal, a 32-record network page boundary, full resync,
independent state changes, tombstone and revocation. The receiver preserves exact
source version, origin, original author and reviewed evidence; raw source paths
and unselected capture markers do not enter published payloads. Gateway revocation
denies later access even before the revocation record reaches the PDS, without
erasing the receiver's prior copy.

`node tests/federation-pds/run.mjs --runtime` runs that same Rust workflow against
the pinned actual upstream PDS. A fixture proxy discards the first **successful
upstream create** response; the Rust worker succeeds only through exact readback.
After all publications, the independent upstream SDK validates the v2 Lexicon
wrappers and signed repository CAR, rejecting a wrong signing key. Network
isolation, `PDS_DEV_MODE=false`, SSRF protection and crawling-disabled settings
are checked. No PDS fork or broad security bypass was needed. The original
encrypted cold-restore PDS harness and its three corruption/format tests also pass.

Additional tests cover all eight v2 variants, nested unknown-field retention,
unsupported semantic versions/kinds, tampered known content, float rejection,
origin/space key collisions, incorrect CIDs, malformed readback, redirect refusal
and bounded response size. Service tests cover disabled-by-default startup,
private token files, real background retries, current permission/disabled-principal
resolution and task abortion on shutdown. The nine storage durability regressions
continue to cover concurrent acceptance, SQL fault rollback, leases, fencing,
quarantine, immutable conflicts and corrupt/truncated/forked replica recovery.

Validation: focused Rust/Atrium/property/conformance tests, the
expanded real-DB CI command, both PDS modes, boundary checks, and Lean `lake build`,
`lake test` (fourteen traces), `lake env leanchecker`. Reducer semantics/proofs are
unchanged; old Lean proof-linter warnings remain. Workspace all-target tests pass
with isolated XDG paths, inherited Memory/database variables unset and serial
execution; database-gated cases are covered separately by the real-DB run.
Workspace Clippy with `-D warnings`, format/diff checks, JavaScript syntax and CI
YAML parsing pass. The documentation-link test was rerun after the complete
documentation set was created. Expired attempt leases are distinguished from
stale writer epochs, so a late request does not permanently stop a current worker.

The approved-plan workflow synced the master checklist at **11/39 complete** and
correctly refused to finish/archive the full roadmap: 28 items remain. Only C1–C3
are newly checked; the six-item C completion contract is complete. No broader
roadmap item is treated as passed by these integration tests.

Remote CI and independent local nanoda are not claimed as executed. CI adds the
runtime/store/service changes to the database/PDS buckets and runs the Rust-to-real
PDS workflow as well as the existing cold-recovery gate. The pinned Lean action
and its no-sorry settings remain unchanged.

## Deliberate remaining limits

This is the approved private-gateway profile, not native Spaces or public Bluesky
interoperability. Its schema namespace is not approved for external publication.
The runtime trusts pinned endpoint credentials/TLS and gateway publisher binding;
the independent signed-CAR harness is **not runtime signature verification**.
D still owns deployed raw-PDS isolation, identity/signature/key lifecycle, model
forwarding, audit/retention/legal holds and full security tests. E still owns
Web/CLI/TUI review, enrollment, imported retrieval/citations and proposal approval.
The tests use separate databases and runtime instances, not separately administered
customer installations with completed browser workflows.

Replay is O(history); resync is bounded to 16 MiB and fails closed above that size.
One role/space is configured per service process. Automatic token refresh, scalable
checkpoints, coordinated backup of all Memory/PDS identities/jobs/cursors, and
customer operational readiness are not claimed. PostgreSQL must remain available
locally during peer disconnection. Plaintext is visible to administrators and
admitted recipients; revocation cannot recall bytes already delivered. R1's
advisory queue and timeline transaction gap are not synchronization dependencies
and remain open. No source Notion page, startup document or live deployment changed.
