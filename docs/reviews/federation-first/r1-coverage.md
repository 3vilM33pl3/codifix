# R1 mutation coverage inventory

Current audit: 2026-09-10. This supersedes the intermediate inventory; dated
foundation/result-boundary reports remain historical slice evidence. A boundary
is a transactional timeline event, a dedicated audit, or an operational journal,
not necessarily a second event for every row. Tests use disposable data.

Paths beginning `service/` mean `crates/mem-service/src/`; other crate paths are
relative to `crates/`. Pool wrappers own a transaction; transaction-taking APIs
participate in the caller's transaction and never broadcast before commit.

| Persisted operation family | Write path and durable boundary | Regression evidence | Covered |
| --- | --- | --- | --- |
| Capture, session/task, implicit project | `mem-curate/src/repository.rs` capture transaction: stable capture/event IDs and trusted context | Capture replay, competing delivery and late-event rollback in curate DB tests | yes |
| Curation, versions, tags, sources, chunks, automatic replacements | Curate transaction: run/mutation events and version-pinned `mem-store::post_curation` recipe | Curation/event/job rollback and competing curators | yes |
| Corrections, replacement decisions, canonical state | Curate replacement APIs, reinforcement validation, loop proposal decisions: version/source/review/audit/event/intents together | Review event failure, competing approvals, stale source, procedural decision tests | yes |
| Explicit and derived relations | Curate relation transaction APIs and loop apply; semantic stage fenced result and operation event | Curate semantic/relation DB tests and loop proposal regressions | yes |
| Archive, tombstone, retention | `service/repository/handlers/memory/{mutations,retention}.rs`: changes/deletes with subject or operation event and retention watermark | Service DB repository event failure, scoped retention, dry run and gap tests | yes |
| Provenance checks | Prepared checks, serialized apply, volatility audit and operation event in service provenance repository | Late-event rollback and read-only dry run | yes |
| Bundle import/export | Service bundle repository: import receipt/project/records/relations/chunks/intents/event; export intent then prepared receipt + event | Duplicate import, event failure, export receipt failure remaining pending | yes |
| Graph/index/embedding maintenance | mem-graph extraction and mem-search rebuild/backfill/prune: prepare, revalidate, atomic batch + event | Provider failure preserves index; late-event rollback, stale inputs and lease expiry | yes |
| Access reinforcement, scores, thresholds, volatility, compaction | mem-reinforce recorder/repository transactions with access/score/utility journals | Batch audit failure, concurrent rewards, decay, volatility, pruning and compaction | yes |
| Validation scheduling, results, review | Requested run + ownership + event; verdict/evidence/score/version/job/event transaction; failure journal | Provider cancellation, lost owner, stale source, review race and verdict tests | yes |
| Projects and commit synchronization | Service project upsert and commit-sync transaction APIs with ProjectChanged/CommitSync | Direct repository commit-sync attribution/rollback and implicit-project tests | yes |
| Checkpoint, plan and standalone activity | Activity is itself the persisted event; service activity/offline replay; local checkpoint/HTTP outcomes have receipts | Activity attribution, idempotent replay, concurrent JSON writers and failed external ack | yes |
| Agent workspaces | Service agents repository start/heartbeat/finish transactions with WorkspaceChanged | Direct callers, implicit project and late-event rollback | yes |
| Loop registry, global switch, settings | Service loops transaction with loop_setting_audit; project settings also append timeline event | Registry/run regressions, setting event failure and protected global-audit failure | yes |
| Loop approvals, phases, cancellation, feedback | Request/decision + sanitized audit or run trace; owning run lock; foreground result fences | Failed approval audit, cancellation, disconnected owner, stale command/result phase tests | yes |
| Trigger intake | trigger_events is intake journal; 0047 adds trusted actor separately from reported source in same insert | Failed actor insert leaves no intake; duplicates stable; changed-input/cross-project dedupe rejected | yes |
| Loop proposals and execution results | proposal_creation receipts, proposal_decisions transactions, run_persistence events/jobs/ownership | Duplicate operation, failed event, stale lease, terminal-run atomicity and interruption tests | yes |
| Auth administration/bookkeeping | Service auth store/OIDC: credentials, membership, bootstrap, token use, flow consumption, login/logout/revocation with protected audit | Revocation/membership/login/session fault tests and HTTP permission/CSRF regressions | yes |
| New offline captures | Compound DuckDB capture/activity intent; PostgreSQL capture + event commit before local ack | Reopen and repeat after ambiguous acknowledgement; one capture/event and original attribution | yes |
| Legacy offline/standalone activity | Legacy identity explicitly unverified; activity stable ID; local pending/synced/failure journal | Activity replay/attribution tests; no historical compound-intent guarantee claimed | yes |
| Configuration, providers, watcher restarts, export effects | Service effects, strict-JSON/resume and direct embedding: private intent before effect, separate outcome, no automatic replay | File serialization/symlink/failed-ack tests; provider observes prior intent; cancellation/watcher tests | yes |
| Local checkpoint/plan and sandbox effects | mem-platform journal, CLI resume/checkpoint/activity and mem-loops sandbox: owner locks, synced intent/outcome and reconciliation | Concurrent writers, owner loss, immutable receipts, failed ack, sandbox failure/cleanup receipt tests | yes |
| Background stages and explicit retry | mem-store jobs/post_curation and service background: immutable inputs, leases, attempts, caches, budgets, child IDs and fenced results | Store/worker fault suites, independent-process crashes and actual Web/CLI/PTY workflow | yes |
| Explicit old-enrichment repair | post_curation::request_repair and privileged example: fresh event + selected-version jobs, no historic runs/versions | Late failure rolls back jobs, duplicate operation retains four stages/one event, changed actor conflicts | yes |
| Federation | Existing source/outbox/accepted-order/inbox/projection/cursor/audit transactions retained | Store durability, runtime C/D, HTTP verification, service E two-instance required-DB tests | yes |

## Operational boundaries and limits

- Trigger **received** does not mean **dispatched** or **completed**. Intake
  remains when routing stops before creating all runs. Inspect linked
  `loop_runs.trigger_event_id` and traces before explicitly replaying missing
  work through a new policy-checked manual run. Never reroute duplicates
  automatically or borrow the former requester's authority.
- Foreground interruption preserves completed phases; it does not pretend a
  worktree, command or provider call was rolled back. Background enrichment is
  the recoverable queue; arbitrary foreground commands are not automatically
  retried jobs.
- Attempts, execution owners, provider caches, consolidation inputs/budgets and
  retry receipts are operational journals, without recursive jobs/events.
  Private cache contents never enter Jobs listings. Existing opt-in bounded LLM
  diagnostics are separate and not required for job recovery.
- Watcher presence, process health, broadcasts, retrieval caches and local
  watcher observation bookmarks are advisory, not proof that capture committed.
  PostgreSQL or a committed offline intent is authoritative. Lost notifications
  recover through sequence reads with explicit retention gaps.
- Installer/updater output, generated scaffolding, downloaded user artifacts
  and evaluation reports are installation/export artifacts, not a second
  application database or timeline. R1 does not add database rollback semantics
  to package installation, evaluation or user-authored files.
- Old offline items/events/jobs cannot acquire provenance they never had. No
  migration fabricates lost events, owners, jobs or actors. Windows directory
  metadata power-loss durability remains unverified; see
  [external effects](../../developer/architecture/external-effects.md).

## Ordering and evidence

Migration 0039 preserves sequence values and exclusively installs a commit-held
counter. Domain locks/writes precede timeline append; broadcasts follow commit.
Cursor reads are bounded/ascending with retention watermarks. Claims take short
global admission before project slot/job/attempt locks; result transactions
never acquire admission. No provider request runs inside a result transaction.

See [operator/process evidence](2026-09-10-r1-operator-workflow.md). The final
contract and rollout evidence must pass before declaring R1 complete. This is
not a customer disaster-recovery or production-release claim.
