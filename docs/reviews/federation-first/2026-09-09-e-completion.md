# Stage E — controlled end-to-end knowledge federation

Completed implementation and controlled integration evidence, 2026-09-09, on
`codex/federation-v1`. This closes E1–E4 at the engineering/integration scope; it
does **not** authorize a customer deployment or close the full roadmap.

Incremental commits: `a1ff236` completion contract, `2031fb8` shared workflow API
and transactions, `0ab1669` Web/CLI/TUI controls, `a932276` isolated services,
real-PDS/client verification and CI. The earlier API increment remains recorded
in [its historical checkpoint](2026-09-09-e-service.md).

## Delivered

- Configured-space discovery and bounded retained stream/member/proposal views;
  caller-scoped local grants, subscriptions and immutable review metadata.
- Authority-only remote capability enrollment/revocation, distinct from local
  application permissions and explicit local grants.
- Exact source/evidence review drafts, server-computed canonical digests,
  explicit future-update consent and retry-safe submission.
- Origin-directed correction proposals and idempotent origin approval/rejection.
  Publisher, original author, source/shared versions, evidence, locator and digest
  remain attributable in selected shared context.
- Consent pause, bounded catch-up, verified full resynchronization, authorized
  retries, and honest local queued/accepted/published-and-replicated observations.
- Web **More → federation**, `memory federation --project <slug> ...`, and
  `memory federation --project <slug> tui`, all using the same local API.

See [workflow and trust limits](../../user/federation-workflow.md),
[CLI/TUI commands](../../user/cli/federation.md), and
[OpenAPI](../../api/openapi.yaml).

## Important approval semantics

Approval advances an origin-owned **shared** version; it does not copy imported
correction text into ordinary local memory, implicit retrieval or model pipelines.
The original local source reference is preserved as the source basis. The
corrector's identity/time remain in the accepted projection and citation.

The resolution intent and pause of the local source subscription commit together.
Outstanding work that can advance that stream must first be accepted/observed.
Unrelated queued or failed streams do not freeze its review. A fresh explicit
review must echo the current shared version, reconcile counters, and intentionally
select local content before automatic updates resume. The UI shows both current
shared and selected local content. This prevents an unnoticed overwrite of an
approved correction, while keeping imported sharing restrictions intact.

Pausing does not erase already reviewed backlog; disabling the local grant stops
sending it. A queued action is not globally accepted. Conflicting request-ID reuse
fails closed. Failed or stale immutable work is visible and cannot be silently
rewritten into a different publication.

## Measured evidence

| Gate | Observed result |
| --- | --- |
| Two independent instances | Two separate Memory service OS processes, different local sessions/tokens and non-superuser DB credentials, separate databases; one app cannot connect to the other's database or authenticate to its HTTP API. |
| Enrollment/publication | Authority HTTP membership actions, explicit local grants, reviewed publication through the actual configured gateway and pinned upstream PDS. |
| User correction loop | Real browser sessions review/publish, recipient reads/cites, proposes correction, origin approves v2; local source stays v27 and unchanged, shared v1 history/evidence remain. |
| Terminal/CLI parity | A real private-socket tmux PTY retrieves context and submits a confirmed correction; CLI discovery/context/proposal inspection and idempotent rejection succeed. Forms use the same policy-checked API. |
| Delivery faults | Gateway disconnection preserves retained reads and local source/review queueing; reconnect catches up. Repeated full replay preserves the snapshot. |
| Revocation | Future gateway pull/resync and publication denied; accepted sequence does not advance for the revoked writer. Existing retained copies remain locally readable until the current local grant is disabled. |
| Browser safety | Explicit confirmation, unchanged retries after uncertain replies, cancellation on project switch, read-only/forbidden states, escaped imported script text; desktop/mobile layout and no page errors. |
| Network/DB boundary | Separate-space TLS isolation harness passes raw-route/token/space denial, PDS/database IP isolation, private mounts and the gateway's restricted SQL role. |

The E fixture's application roles can create objects **in their own databases**
for normal migration startup. The separate isolation harness verifies the narrower
gateway six-table role, including denial of the new local action table. Sessions
and service tokens are fixture-issued; this is **not** evidence of customer OIDC
login. All identities/content/keys are synthetic. Both application processes run
on one controlled host; no real organizations or customer networks were enrolled.

### Validation run

- `cargo test --workspace --all-targets --locked -- --test-threads=1` with required
  disposable pgvector: **717 passed, 0 failed, 0 ignored**, 38 test suites.
- `npm --prefix web run test`: **66 passed**, 19 files; production Web build passed.
- `MEMORY_FEDERATION_E_CLIENTS=1 node tests/federation-pds/run.mjs --runtime`:
  C/D runtime tests and E process/browser/CLI/PTY journey passed against the real
  pinned PDS, followed by independent upstream Lexicon/signed-CAR verification.
- `node tests/federation-isolation/run.mjs`: both space probes, TLS positive
  control and restricted database-role checks passed.
- `cargo fmt --check`, workspace Clippy with `-D warnings`, boundary script and
  OpenAPI/router inventory passed. OpenAPI and CI YAML parse successfully.
- `lake build && lake test`: **14 shared traces passed**. Existing Lean linter
  suggestions remain; no neutral reducer/proof changes or new proof claims.
- Explicitly lean CLI build check passed; existing non-TUI dead-code warnings
  remain outside this change.

The browser/CLI/PTY gate is now required by the scoped CI PDS job on relevant
Rust, CLI and Web changes. These are **local results**, not a claim that remote
CI has run. Reproduction and fixture cleanup are documented in the
[PDS harness guide](../../../tests/federation-pds/README.md).

## Remaining release boundaries

Production namespace ownership, customer SSO/deployment, security review,
encrypted offsite recovery of the entire publication state, legal retention/hold
policy, held-out answer quality, supportability and paid-pilot outcomes remain
separate roadmap gates. The PDS cold-restore prerequisite was not re-run as part
of this E pass and is not a full Memory disaster-recovery demonstration.

No customer/provider/public-network publication occurred. Imported context is
still explicit and tool-free, not automatically mixed into ordinary query or
external-model pipelines. Signatures authenticate origin/integrity, not truth.
Plaintext remains readable by administrators and admitted recipients; revocation
does not recall bytes. Native Spaces interoperability, public Bluesky, Neuralese,
Aeneas, peer consensus and automatic multi-region failover remain deferred.

No Notion or startup documents were changed. Test-owned PDS/isolation containers,
networks, volumes, app processes, private configuration and PTY sockets were
removed; only local synthetic screenshots and validation logs were retained.
