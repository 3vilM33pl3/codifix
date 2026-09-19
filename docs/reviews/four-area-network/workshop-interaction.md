# Workshop interaction

Continuation approved on 2026-09-18 after the bounded first-live milestone.
This increment implements the human-facing part of the four-area contract,
starting with the independent Workshop preview. It does not activate other
areas, R3, automatic installation or unrestricted Pi tools.

The operator's exact reviewed source grant, Sol model, normal credential and
USD 2.50 / 20-generation daily limit remain authoritative. Private content is
separate from metadata feeds. An approval records review, not execution authority.

- [x] Checkpoint continuation through dev Memory before implementation.
- [x] Private project-scoped saved-result inspection with exact source references.
- [x] Immutable, digest-bound, idempotent approval/rejection with current authority.
- [x] Web/CLI/TUI result and review workflows with explicit confirmation.
- [x] Explicit bounded question submission and isolated recoverable execution.
- [x] Fake-provider, database, authorization, replay and frontend verification.
- [x] Update the independent preview, rehearse the workflow and record evidence.

The parent four-area plan remains open. Results must not imply deployed
cross-area exchange, automatic maintenance or complete autonomous supervision.

## Deployed saved-result slice, 2026-09-18

The dev checkpoint succeeded before edits: task
`2ec9e981-9c01-446c-b6f8-0573d1a2ac8e`, plan capture
`7e523326-15ad-4f4b-93f6-92b7f6fb8fa7`. Implementation commits are
`58ef0704` (private API/review), `a72e5e57` (Web/CLI/TUI), and `383ec247`
(independent preview upgrade). The unfinished checkboxes above apply to the full
interaction increment, not a claim that no verification/deployment occurred.

- Exact-project authenticated reads revalidate the pinned question, source
  versions and evidence digests. Source withdrawal and revoked credentials fail
  closed. Model text is untrusted display data, not instructions to execute.
- Review requires current `loops_run` in addition to read permission; browser
  writes retain CSRF/Origin checks. Immutable migration-0059 receipts bind the
  actor, request ID, action, digest and decision. Conflicting changes fail;
  matching retries return the existing receipt. Approval launches no work.
- Required disposable PostgreSQL tests cover authorization, read-only mode,
  concurrent retry, failed commit, revoked credentials and withdrawn sources.
  The 76 partition/import/operator tests pass with the new explicit schema
  catalog; review history is archived, never revived as execution authority.
- All 115 Web tests and the Web build pass. Focused service/CLI tests, OpenAPI
  parity, strict scoped Clippy, formatting and boundary checks pass. Existing
  GraphTab chunk-size warnings remain. This is not a new full-workspace or
  federation acceptance run.
- Live browser and CLI displayed the existing paid answer and its three
  citations. Browser and PTY TUI opened and cancelled confirmation without
  submitting a review. The live database still has three memory versions, zero
  captures, one model attempt, one proposal and zero human reviews.
- Only the dedicated Workshop app was replaced. Its database migrated from 0058
  to 0059 after a writer stop and encrypted, checksum-verified NAS backup. The
  pre-upgrade backup is not restore-rehearsed; older app containers are stopped
  and must not be started against the newer schema. App isolation, loopback URL,
  source grant and provider budget are unchanged. No new model call occurred.

The remaining work at that checkpoint was bounded new-question submission and
recoverable execution. The following entry records the subsequent implementation.

## Bounded private questions, 2026-09-19

Continuation was checkpointed through dev Memory before edits, using task
`f8e1c1a5-1f99-4513-bd66-db119425dd9a` and plan capture
`eabacd6d-4fd5-4475-a030-e597edd0a21a`.

### Implemented contract

- Append-only migration 0060 persists the exact private question, stable request
  and attempt identities, confirmed profile/input digests, original requester
  principal and revocable credential reference. It stores no credential secret.
  Question bodies do not enter the operational timeline or retrieval corpus.
- Preparation makes no model call. Confirmation binds the exact question and
  reviewed source collection. Changed retries fail; matching retries return the
  existing receipt. Eight pending requests per project and a ten-minute deadline
  bound intake. Submission needs current direct read, loops-run and bundle-export
  permissions; browser CSRF, read-only and evaluation restrictions remain active.
- List/status is requester-private. A cited completed answer is project-private
  to authorized readers, not guaranteed requester-only. Both execution and
  delivery recheck the original requester's credential and current permissions.
  Revocation, source withdrawal and pinned configuration changes fail closed.
- The isolated operator claims one eligible question at a time, rechecks current
  authority before disclosure and result commit, and retains uncertain provider
  attempts without automatic reinvocation. Paid actions serialize across worker
  processes. Source/configuration checks run again after lock waits; expired
  workers cannot publish results. The original daily budget remains authoritative.
- Web, CLI and TUI provide preparation, exact-term confirmation, list/status and
  cited-result inspection. Preparing or cancelling makes no paid call. Ambiguous
  submissions retain their request identity. Review remains a permanent human
  decision only: no memory edit, installation or additional action is authorized.

### Verification and deployment

- Required disposable PostgreSQL tests passed for the store, runtime and scoped
  service paths, including competing paid claims, requester revocation, immutable
  retries, source/configuration changes, stale-result fencing and terminal
  uncertainty. The separately invoked pinned, patched Pi test used a fake provider
  and restricted worker DB role: one generation completed, then a new queue-worker
  invocation returned idle without another generation.
- All 76 area partition/import/operator tests passed at the explicit schema-60
  catalog. Archive handling does not revive imported questions as authority.
  The ordinary runtime suite passed 62 tests with four opt-in tests ignored;
  the real-Pi queued-question test above was invoked separately.
- Scoped service/CLI tests, OpenAPI parity, lean CLI compilation, strict scoped
  Clippy, all 126 Web tests and the Web build, formatting and boundaries passed. The existing GraphTab
  size warning remains. This is not a new full-workspace, federation, four-area
  or customer-release acceptance run.
- The exact Workshop writer stopped before a schema-59 backup and migration to
  60. `before-questions-60.tar.gpg` was copied to the existing NAS backup directory
  and checksum-verified: SHA-256
  `3a85e2cb8931330241b33a99c5e47458ec0ccc77dd3f86363d54f54bba42b2ef`.
  It excludes provider credentials and has not been restore-rehearsed. The stopped
  old app remains retained; it must not be started against the upgraded database.
- Pinned app/worker/Web artifacts are under the private `questions-60` directory.
  The dedicated app still has neither provider key nor Docker socket, and the
  original services and source dataset are unchanged. The `memory-workshop`
  wrapper now selects this client. Only the separate trusted operator process
  loads the existing normal credential in memory; R3 is untouched.
- `memory-workshop-question-worker.service` is enabled and was verified active
  after an idle restart. It polls explicitly confirmed work, never invents
  questions, uses `Restart=no`, and allows seven minutes for controlled shutdown.
  Authority/storage/configuration failures require inspection, not a restart loop.
  The staged polling script SHA-256 is
  `d31ea610b1021c43367e52bd70279f455cc7c4913962c4a793301e7dc0be24f3`.
- Live Web and CLI preparation showed the same question digest, three sources,
  OpenAI/Sol destination and USD 2.50 / 20-call limit. Browser cancellation and
  empty-question listing passed. A fresh page load is required after the upgrade;
  an already-open SPA may continue to display the prior UI.
- Two consecutive real PTY checks prepared and cancelled a new question, with
  exact destination/model terms visible and no submission. A separate PTY check
  opened the saved cited answer and cancelled review. The terminal observer now
  allows ten seconds for clean runtime shutdown without resending input.
- Live checking found intermittent authentication-GET transport failures during
  TUI startup. The local preview relay now permits 32 rather than 16 concurrent
  Web sockets, retaining the five-minute idle timeout, fixed loopback listeners,
  fixed Docker destination and unchanged 128-PID/thread container limit. This
  leaves room for browser and TUI reads without the 64-slot trial's worst-case
  thread pressure. Repeated rehearsals passed after the adjustment; saturation
  itself was suspected, not conclusively captured in half-second samples. The
  original relay is retained locally in `before-questions-60/loopback-relay.mjs`
  (added after the encrypted backup was made). The deployed template SHA-256 is
  `5a408a42aeb23ea9333d2dce1ab3e2447b1c1ba82a2e02cad51e972420e44d43`.
  Four Node helper/relay tests and two terminal-observer tests pass. Restarting
  the relay stops its dependent question worker; explicit start was verified
  after the relay and app were healthy again.
- Live post-upgrade counts were zero questions, one earlier model attempt, one
  saved proposal and zero human reviews. No additional live model call or human
  review was performed during implementation verification.

The standing source grant expires at **2026-09-19 13:46:41 UTC (14:46 London)**.
Neither deploying the worker nor confirming a question extends it. An operator
must review and explicitly renew authority before further disclosure after expiry.
An idle worker process does not establish a healthy model or an autonomous agent.

The [demo runbook](../../user/workshop-preview.md) is the operator-facing entry
point. The parent four-area deployment remains open: this is Workshop-local
interaction, not cross-area communication, unrestricted chat or autonomous
knowledge maintenance.

The dev Memory completion check verified all seven items and archived plan
`ba3dc547-0162-4fcd-9c71-1c31eda7772e`, recording implementation
`d8fd082d-bb86-4776-9c14-cd3253e4e193`. This closes this interaction increment
only; it does not close the parent four-area contract or renew disclosure consent.
