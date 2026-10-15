# Mac project replication execution

Approved 2026-10-14; continues the existing onboarding work on
`codex/mac-project-onboarding`. The device downloads all curated knowledge for
the selected Homelab and Workshop/Memory projects. Only locally curated knowledge
uploads; raw captures and session transcripts remain on the collecting machine.
Git clone uses user credentials; commit/push remains the user's normal workflow.

The dev checkpoint succeeded before implementation: capture
`ffc3768d-06e7-4610-9faa-b16be4f61356`, curated plan
`c3b4ac9a-63ce-447c-b384-973270144f21`, thread
`mac-project-sync-20261014`. The isolated onboarding database and dev service at
loopback port 4250 were restarted. Production services were not changed.

## Delivery checklist

- [x] Versioned full curated-project device download, separate from shared
  publication consent; histories/state/tombstones/asserted same-project relations
  and bounded provenance. The derived heuristic graph remains owner-local.
- [x] Transactional curated contribution queue and current-authority owner
  submission/receipts; corrections/conflicts preserve owner history.
- [ ] Startup/reconnect/five-minute and post-curation synchronization, local
  writable workflow, normal retrieval/history/agent access and watcher routing.
- [x] Homelab company configuration, identity/project assignments and manifests.
- [ ] Native package with dependencies, guided resumable setup, Keychain and
  LaunchAgents without changing unrelated repositories/settings/services.
- [ ] Observed MacBook acceptance: login/select/clone/offline search, both sync
  directions, restart/retry/conflict/revocation and no raw/session transfer.

## Compatibility and acceptance

The existing publication-only feed and private ATProto schemas remain compatible.
Full-project device replication does not grant other participants access to the
project. Downloaded knowledge retains origin attribution and cannot be echoed
as a new local publication. Acceptance requires actual native macOS execution;
synthetic/Linux tests are intermediate evidence only. Trust, Windows, R3,
new paid providers and additional plugin formats remain separate.

## Implemented software (2026-10-14)

- Separate `curated_project_v1` device mode: origin-authorized histories,
  canonical state, tombstones and asserted same-project relations; the derived
  heuristic graph remains owner-local. Unavailable provenance remains an ID, not
  permission to fetch raw bytes. Existing shared-publication
  profiles and schemas remain compatible.
- Signed, nonce/device/project/cursor-bound pages and atomic projection commits.
  The company and owner recheck current authority around transport. Gateway
  signatures attest this scoped delivery, not an entire ATProto repository.
- Explicit device contribution consent and transaction-triggered curated outbox.
  Additions, jobs, events and immutable receipts commit together at the owner;
  corrections create proposals/conflicts. Separate immutable approval/rejection
  outcomes reconcile owner versions without rewriting submission receipts.
- Startup and five-minute polling with two-second queued-work detection. Failed
  uploads do not prevent verified downloads. Ineligible oversized work becomes
  blocked rather than poisoning the queue; transient errors back off 30 seconds.
- Local lexical queries merge pending local work with origin-labelled owner
  copies. Accepted copies are suppressed after owner versions arrive, including
  after tombstone retention purges; identity-only watermarks prevent resurrection.
- Web/TUI show upload counts and distinguish read-only owner results. Mac setup
  binds exact repository/project identities, initializes sync before watchers,
  and uses per-repository LaunchAgents. Curated MCP disclosure requires its own
  mode-bound reviewed grant; legacy publication consent is not broadened.
- Explicit corrections to originally downloaded knowledge now stage an exact
  source/digest-bound local proposal. Concurrent retry uses the same operation,
  local intent/event/chunks commit together, and owner history is never edited.
  The CLI defaults to preview; confirmed Web/CLI staging awaits owner review.
- Retained-history inspection exposes versions, author, digest and tombstones,
  without fetching unavailable source references. Normal query results link to
  the owner-history view; TUI Synchronized knowledge supports paginated history.
- Operational sync errors are persisted as bounded sanitized codes. Failed
  transfer remains visible alongside the last verified download and upload counts.
  Pending local query results show contribution outcome/review status.

Important remaining software limitations: local content additions/corrections
are queued, but local mutable-state/relation/deletion operations are not an
owner-write synchronization protocol. Unsupported advanced laptop ranking/history
query flags fail explicitly rather than implying full query parity. Pending
proposals are not automatically approved. No paid curation provider is enabled
by setup. These limitations must remain visible before claiming complete parity.

## Verification evidence

All data/providers in these checks are synthetic; the disposable PostgreSQL 17
database runs on loopback port 55471. No customer dataset or paid model call was
used.

- Required-database service suite before the final history-view slice: 216 passed,
  3 explicitly ignored. Includes independent company/owner HTTP download/upload,
  exact retry, cross-project denial, signature failure, and authority changes.
- Required-database federation store/runtime all-target suites passed, including
  queue interruption, receipt ordering, project scoping and replica replay tests.
  This is not a new live-PDS deployment acceptance claim.
- Focused runtime test proves a failed upload still commits a signed incoming
  page. Store test covers archive, tombstone and physical owner retention while
  retaining suppression watermarks.
- Required-database release `mem-federation-store --lib device_replica` checks
  passed on 2026-10-15: 2 passed, 0 ignored. A synthetic 100,806-edge derived
  graph stays outside the feed; asserted same-project edges sync, cross-project
  edges stay excluded, reclassification/deletion removes replicas, replay is
  harmless, and the 100,000-record authoritative-feed bound still fails closed.
- Contribution tests cover mutation/event rollback, oversized queue recovery,
  approved/rejected correction outcomes and immutable receipt replay.
- Web offline/query tests and build passed before the history-view slice;
  focused TUI owner-result test passed. Final slice results are recorded below.
- Boundary check passed. Build cache measured 34 GiB, with 404 GiB free; no
  destructive cleanup was needed.
- All 106 Python Mac setup/operator/package tests passed. These use fake native
  commands and runtime files, not a native dependency bundle.
- Lean `lake build && lake test` passed with existing linter warnings. No new
  proof of the device sync implementation or authentication is claimed.
- Lean CLI build passed; reduced-feature existing dead-code warnings remain.
- OpenAPI route inventory passed. Focused CLI correction parsing confirms that
  preview does not invoke company enrollment or contact the service.
- Focused required-database cache/correction checks: 6 passed, including
  history retention, cross-scope denial and correction transaction rollback.
- Astra independently found queue poisoning, post-retention resurrection and
  pending-proposal polling starvation. All three were fixed; the runtime test
  demonstrates fair polling beyond 32 pending operations in bounded passes.
- Final CLI/TUI/project-sync suite: 365 passed, none ignored. This includes
  public help/completion coverage, portable documentation links, native installer
  non-restart behavior, immutable workspace retries and local correction preview.
- Final required-database service library suite: 217 passed, 3 explicitly ignored
  (live browser/CLI rehearsals needing separately configured executable/browser
  inputs). No ignored test is claimed as acceptance evidence.
- Final history/correction UI checks: 25 Web tests passed and Web build passed;
  focused TUI offline/history checks: 5 passed.
- Final targeted Clippy passed with warnings denied for the CLI with
  `tui,project-sync` and for federation runtime/store/service libraries.
  Formatting, diff whitespace and boundary checks passed.
- Final lean CLI build passed after removing the old TUI-only gate from cache
  history HTTP support. Existing reduced-feature dead-code warnings remain.
- Verified software milestone remembered through dev Memory: capture
  `b5c28efa-e521-4e4e-a6c9-cb68874de7aa`. This deliberately records partial
  implementation evidence, not completion of the approved deployment plan.

## Deployment and native acceptance remain open

On October 15, the backed-up Homelab and Workshop owners were upgraded to the
frozen candidate and schema 84. Their company origins use verified HTTPS and
multi-user authentication; scoped local client credentials and existing watchers
were renewed. See the [company deployment ledger](company-deployment-20261015.md)
for exact changes, preserved identities and forward-recovery requirements.
The dev company console remains separate from the live company service. Human
login, four bounded company delegates, both reviewed manifests and the approved
Mac device's offline/contribution grants are now verified in the company ledger.

The real ARM64 Mac is now reachable and has the native candidate installed.
PostgreSQL 17.11, pgvector 0.8.7 and Python 3.14.8 were built and bundled on that
Mac. Installed runtime hashes/modes and the installed Memory executable are
verified; the user-approved login-Keychain trust verifies Authentik HTTPS.
See [native evidence and package hashes](native-mac-20261014.md). This is not yet
the completed company login and two-project synchronization journey.

Next deployment actions are the corrected native installer, exact two-project
selection/clone/offline/search/upload/correction/revoke/reboot acceptance sequence.
The six-step plan is not complete, and no final plan-verification claim has been
recorded.
