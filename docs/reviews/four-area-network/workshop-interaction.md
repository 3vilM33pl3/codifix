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
- [ ] Explicit bounded question submission and isolated recoverable execution.
- [ ] Fake-provider, database, authorization, replay and frontend verification.
- [ ] Update the independent preview, rehearse the workflow and record evidence.

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

Next: persist bounded user questions with exact-input identities; extend the
separate operator worker to claim/recover that work; verify fake-provider,
revocation, expiry and uncertain-billing behavior; then expose explicit confirmed
submission in all three clients. The app must not gain a Docker socket or
provider key. A chat-looking UI alone would not satisfy this remaining gate.
