# R1 — Durable background work and transactional mutation events

Approved 2026-09-09. This scoped implementation contract does not replace or
complete the federation-first master roadmap. F remains gated.

## Transactional persistence

- [ ] R1.1 Introduce a small `mem-store` for transaction-aware event append and durable jobs; pure shared contracts in `mem-record`, execution in `mem-service`. No ATProto, HTTP clients or configuration in the store.
- [ ] R1.2 Couple all persisted knowledge operations (capture, curation, corrections, canonical state, relations, provenance, imports, retention, graph/index and reinforcement) with events and necessary job intents; cover HTTP, background and direct-library callers through transaction-taking APIs and compatible pool wrappers.
- [ ] R1.3 Cover administrative operations (projects, agent workspaces, loop settings/runs/proposals and auth). Maintain a complete operation-to-event/audit/journal inventory. Use operation-level events for derived batches and attempt journals for jobs, without recursion. Keep global security audits private, authenticated actor separate from authorship, and secrets/unrestricted payloads out of events.
- [ ] R1.4 Replace timeline allocation with a commit-held transactional counter, preserve old sequences, use consistent lock order, and broadcast committed events only. Support bounded cursor recovery and explicit retention gaps.
- [ ] R1.5 Atomically queue offline capture/activity in DuckDB, replay both atomically into PostgreSQL with stable IDs/attribution, and acknowledge afterwards. Journal and reconcile external/filesystem intent and outcome without claiming cross-system atomicity.

## Durable execution

- [ ] R1.6 Add new migrations after 0038 for jobs, dependencies, attempts, lease tokens, retry generations and project slots. Persist bounded IDs/fingerprints, not whole capture requests or secrets. Curation success means results/events/job intents committed, not enrichment finished.
- [ ] R1.7 Replace the process queue with durable embedding, dependent semantic dedup, independent validation-due checks and coalesced consolidation, with durable synthesis children. Preserve enablement, dry-run, utility-floor, provider and daily-cap policy; distinguish skipped and unavailable stages.
- [ ] R1.8 Recheck authority/policy before work and result writes; explicitly identify internal automation. Two concurrent projects per process, one post-curation stage per project across processes; poll 2s, lease 120s, renew 30s, five attempts per generation, exponential delay 10s capped 10m; validate configurable bounds.
- [ ] R1.9 Retry transient faults; immediately block quota/credential/auth/config faults; expose exhausted failures. Fence all result writes, persist reusable stage outputs and input-version fingerprints, deduplicate proposals, revalidate consolidation inputs. Do not promise exactly-once provider billing; expose uncertain attempts and use provider idempotency when supported.
- [ ] R1.10 Recover at startup/database reconnect; stop claims and cancel owned tasks on shutdown/reload. Polling is authoritative. Keep federation storage/runtime independent and unchanged in semantics.

## Operators

- [ ] R1.11 Add JobSummary/Detail/Attempt/Status/retry contracts; GET project jobs and job detail, POST job retry with stable request ID. Bounded filtered cursor listing; queued/running/retry_wait/blocked/failed/succeeded/skipped, dependencies, sanitized errors, next attempt and worker health. Optional job reference on curate response.
- [ ] R1.12 Require project read access and LoopsRun plus current operation permissions for retry; enforce CSRF/read-only policy. Retry retains history/completed stages and immutable inputs, opens bounded generation, cannot restart running/successful jobs or bypass policy.
- [ ] R1.13 Provide Web Jobs, memory jobs --project commands and TUI Jobs inspection/explicit retry parity. Report queue age/depth/failures; never expose prompts/source contents/provider bodies.

## Verification and rollout

- [ ] R1.14 Disposable PostgreSQL/fake-provider fault tests: rollback mutation/event/job, all operation families and direct callers, delayed commits/cursors/retention, worker crash boundaries, competing processes/stale leases, retry/block/exhaustion/revocation/config changes, partial-stage/proposal dedup and offline ambiguous replay.
- [ ] R1.15 Real service Web/CLI/PTY journey, cross-project denial/sanitization, federation integration regressions, required-DB workspace tests, Clippy/fmt/boundaries, Web test/build, OpenAPI/policy inventory and lean CLI build. No paid providers/customer data.
- [ ] R1.16 Document coordinated writer stop/backup/additive migration/restart (no mixed versions during ordering cutover), no fabricated historical events/jobs, explicit operator repair for old enrichment. Record evidence and verify this contract before marking R1 complete.

## Delivery

Small commits: contract/inventory; store/order; knowledge then administrative and
offline coupling; durable workers/stages; clients/docs; fault and workflow proof.
R1 is complete only with no unexplained inventory gaps, restart recovery,
actionable failures, fenced results and proven mutation/event/job atomicity.
It does not prove customer DR, release readiness, answer quality or F eligibility.
