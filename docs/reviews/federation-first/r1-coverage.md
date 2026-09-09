# R1 mutation coverage inventory

Status: implementation in progress. An unchecked row is an explicit gap, not a
claim of transactional coverage. Each completed row requires tests and concrete
write-path references before the R1 gate can close.

| Operation family | Existing ownership | Required durable boundary | Verified |
| --- | --- | --- | --- |
| Capture | mem-curate `store_capture_tx`/`store_capture_as` | session/task/capture + attributed event, replay identity and failure rollback | [x] |
| Curation | mem-curate `curate_tx_with_actor`/`curate_as` | run/memories/automatic replacements + events + version-pinned durable enrichment intents; late-event failure rollback tested | [x] |
| Replacement/validation versions and relations | mem-curate transaction APIs; mem-reinforce | replacement decisions and new versions/events/job intents coupled; direct relation batches emit operation events; reinforcement review/score orchestration still open | [ ] |
| Archive/delete/retention | service memory repository `mutations`/`retention` transaction APIs | state/tombstone/pruning + surviving subject or operation event; direct callers, late-event rollback, concurrent tombstones, scoped retention and dry run tested | [x] |
| Provenance checks | service provenance repository prepared checks + transaction apply; mem-reinforce volatility transaction | source checks + serialized status transitions + volatility audit + attributed operation event; failed event rolls all changes back; dry runs stay read-only | [x] |
| Bundle import/export | service bundle repository | import project/receipt/versions/sources/relations/chunks + enrichment intents + event atomic and replay tested; export/external intent still open | [ ] |
| Graph/index/embedding batches | mem-graph, service embedding repository | batch result + operation event | [ ] |
| Reinforcement and procedural learning | mem-reinforce, service loops | state + dedicated journal/event | [ ] |
| Project/commit/checkpoint/plan activity | service project/activity repositories | mutation + event | [ ] |
| Agent workspaces | service agents repository `_tx` APIs and pool wrappers | start/heartbeat/finish + attributed event; implicit project creation and late-event failure rollback tested; only committed HTTP broadcasts | [x] |
| Loop settings/runs/proposals | mem-loops, service loops | proposal creation + receipt + trace + event coupled; other lifecycle/state transitions still open | [ ] |
| Token creation/revocation, membership grant/revoke | service auth store | state + protected audit; audit failure rolls back revocation | [x] |
| Other auth administration/session/token bookkeeping | service auth store/oidc | state + protected audit or explicit operational journal; remaining inventory | [ ] |
| New offline captures | service offline DuckDB `QueuedCapture`/`sync_capture` | compound local intent, atomic PostgreSQL replay/attribution, retry before ack | [x] |
| Legacy offline and other activity operations | service offline/activity | audit remaining paths; old separate capture/activity rows have no R1 atomic-intent guarantee | [ ] |
| Filesystem/external effects | settings/export/watch/provider boundaries | durable intent + reconciled outcome | [ ] |
| Post-curation stages | service curation worker | fenced output + attempts + dependent intent | [ ] |
| Federation | existing federation store/runtime | retain accepted/outbox/inbox/audit boundaries; regression proof | [ ] |

Operational data (leases, attempts, retry receipts, access/cache bookkeeping)
must map to its own journal or an explicitly documented operation-level event.
They must not recursively enqueue jobs or copy private inputs into activity.

## Proven primitives, not connected workflows

- 0039: commit-held global timeline allocation and retained-history watermarks;
  store snapshot pagination tested. Existing service cursor endpoints/remaining
  notification callers have not yet been converted.
- 0040: immutable work keys, dependency edges, project slots, bounded attempts,
  retry receipts and fenced result transactions. Store tests cover lease loss,
  competing claims, rollback, retry/block and dedup. The service now consumes
  these jobs through `background.rs`; the in-memory queue has been removed.
- Broad R1 validation and user interface work remain open. The master R1
  checkbox must remain unchecked until the entire scoped contract passes.

### Connected worker and operator backend (2026-09-10)

Migrations 0042–0044 pin curation versions, retain private provider outputs and
synthesis inputs, journal budget reservations and fence concurrent version
insertion. The worker runs independent validation checks, embedding-dependent
semantic work and coalesced consolidation with durable synthesis children.
Credential, membership and internal-automation policy checks run inside fenced
result transactions. HTTP jobs endpoints provide bounded project-only listings,
attempts, stage dependencies and request-ID retry receipts.

Required-database tests exercise fake-provider transient recovery, shutdown
during provider execution, expired-lease recovery and revoked credentials before
execution. HTTP tests cover pagination, sanitized details, cross-project denial,
read-only mode and idempotent retry. Full R1 fault coverage remains open,
especially source/policy races, synthesis partial delivery, reconnect and the
remaining mutation families. This evidence does not complete R1.

### Prepared embedding results (2026-09-09)

`mem-search::rebuild` separates provider preparation from transaction-only
application. Existing rebuild callers now preserve searchable chunks on provider
failure and atomically replace them after source revalidation. Prepared outputs
are serializable operational data, with stable chunk identities on replay.
They must not be exposed in job listings. Current execution authority and policy
still need to be checked by the future worker.

Required-database `mem-search` tests passed (46 tests), including malformed fake
provider output, rollback, source changes/deletion, replay, and applying real
embedding results inside a `mem-store` fence whose expired lease rolls back the
entire result. Package Clippy and workspace formatting checks passed. This is a
write boundary, not a connected durable worker or operation-event coverage;
the embedding and post-curation inventory rows remain open.

### Atomic proposal creation (2026-09-09)

`loops::proposal_creation::{create,create_tx}` provides the direct/pool and
transaction-taking entry points. Migration 0041 adds original-input SHA-256
receipts keyed by operation UUID; retries cannot overwrite subsequent human
edits or create another proposal. HTTP creation broadcasts the committed event
instead of asynchronously appending another one. Other existing proposal
creators use an explicit internal identity. Referenced runs and target records
must belong to the same project. Run trace allocation/count updates now share
the owning run lock and transaction across appenders.

Consolidation provider preparation is separated from proposal persistence;
provider-attempt audits remain separate external-operation evidence. Source
snapshot pinning/revalidation, reusable provider outputs, coalescing, policy
rechecks, durable child identities and the service worker are not implemented by
this slice. Existing automatic callers still allocate new operation IDs; their
retry deduplication is not yet wired. Proposal edit/approve/reject transaction
coverage remains open. No additional R1 contract checkbox is complete.

See [result-boundary evidence](2026-09-09-r1-result-boundaries.md) for the focused
fault tests, required-database workspace regression result and remaining limits.
