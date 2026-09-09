# ADR 0007: Immutable memory versions, canonical state, and the ordered event log

Status: accepted (2026-08-20)

## Context

Preparing for AT Protocol federation requires content-addressable records and
a durable, ordered change log. Version rows were documented as immutable but
mutated in place (archive, curate dedup boosts); relations FK'd version rows
and were wholesale re-derived; the timeline log was fire-and-forget with no
ordering; bundles hashed wall-clock timestamps.

## Decisions

1. **Per-canonical state table, not version-per-change.** `memory_canonical_state`
   (migration 0028) holds status/archived_at and the live confidence/importance,
   keyed by canonical_id with no FK (retention may prune any version row - the
   memory_scores precedent from ADR 0002). Archives are bulk operations and every
   new version forces chunk/embedding rebuilds, so minting versions for status
   flips would multiply storage for zero content change. Version rows are now
   fully immutable: `confidence`/`importance` on memory_entries are the values
   asserted when that version was captured. An AFTER INSERT trigger keeps state
   rows in step with new versions; inline-able SQL functions (`memory_state_*`)
   serve read paths.

2. **Relations key on canonical ids with origin provenance** (migration 0029).
   Edges survive version churn; `origin` distinguishes `asserted` edges
   (consolidation, proposals, imports - never touched by curate) from
   `derived` heuristics that curate recomputes.

3. **The timeline log evolves in place** (migration 0030): a backfilled
   monotonic `seq` for cursors and gap detection, a plain canonical/version
   subject reference that survives retention, and loud (logged) persistence.
   R1 migration 0039 replaces sequence allocation with a transactional counter
   held through commit and records retention watermarks. Capture/curation now
   append inside their repository transactions and broadcast only after commit;
   new offline captures carry capture/activity intent in one durable item.
   Other mutation families still require conversion: see the
   [R1 coverage inventory](../../reviews/federation-first/r1-coverage.md).
   No claim of full same-transaction coverage follows from ordering alone.

4. **Canonical JSON via serde_json Value round-trip.** serde_json's map is a
   BTreeMap (the `preserve_order` feature is not enabled), so serializing a
   `Value` emits sorted keys deterministically. Bundle v2 hashes a canonical
   projection excluding exported_at/bundle_id/summary; the bundle id derives
   from the hash.

## Consequences

- Content addressing of memory records is safe: nothing mutates a version row
  after insert.
- Global timeline positions are not contiguous within a project. The new store
  cursor primitive uses deletion watermarks rather than treating every numeric
  gap as missing data. Historical missing events cannot be reconstructed.
  The timeline is not yet a complete mutation synchronization source; the
  federation outbox continues to use its own transactional source capture.
- `memory_source_checks` (migration 0031) preserves verification transitions;
  the old table name survives as a latest-wins view for readers.
