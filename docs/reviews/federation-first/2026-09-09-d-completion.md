# Stage D completion: private-profile trust boundary

Continued from `790a100` under the [D completion contract](d-completion-contract.md).
The first implementation commit is `ace0bfb`; subsequent boundary/test/docs commits
complete D1–D4. This is implementation and controlled verification, not permission
to run customer data or a claim that the full roadmap is complete.

## Delivered evidence

| Item | Implementation | Executable evidence |
| --- | --- | --- |
| D1 | Dedicated gateway executable, restricted non-owner SQL role, gateway-only PDS credentials, fixed TLS proxies and isolated space/instance networks | Both participant containers reach their permitted gateway and own database; wrong tokens, other spaces, raw CAR/repo/firehose/app routes, direct backend/PDS/database IPs and secret mounts are denied; gateway-to-PDS TLS and denied SQL privileges are positive/negative controls |
| D2 | Independent signed receipts, chained complete-delivery bindings, fresh nonce-bound pages, explicit key cutovers and writer fencing | Adversarial page/key/signature tests; three-database workflow; receiver reconstruction into a fresh DB; corrupted gateway receipt rejected at startup; real PDS signed repository verification |
| D3 | Frozen acceptance-time publication policy intersected with current gateway policy and local provider/read/export permissions; scoped untrusted context | Exact reviewed evidence round-trip and raw-source exclusion; malicious evidence stays quoted; provider counter unchanged on denied policy, revoked access or disconnection |
| D4 | Independent immutable receipts/reviews/intents/history, authority-only hold/release/erasure-request decisions and explicit offline/decommission boundaries | Unauthorized hold release and ordinary deletion denied; revocation effective before PDS delivery; offline copies remain locally authorized; releasing a hold never authorizes physical erasure |

Migration **0036** adds security enrollment, acceptance-time policy, independent
audit, receiver trust/proofs and retention decisions. Existing unsigned published
state is refused, not silently certified. Gateway startup checks retained receipt
signatures before enrolling a new writer epoch. Audit-key windows are independent
of PDS migration. No `mem-record` or neutral reducer change was needed; runtime
cryptography and provider policy remain outside the Lean model.

## Local validation, 2026-09-09

- Workspace tests: **708 passed**, with a disposable required pgvector database,
  isolated application configuration/cache/data/state and one test thread.
- Focused neutral/AT/verifier/store/runtime suite: **96 passed**: shared traces,
  property tests, round-trips/syntax/XRPC/MST, malicious CARs, receipts, rollover,
  policy and transactions. Final recovery additions were rerun in the affected
  store/runtime suite.
- `cargo fmt --check`, workspace Clippy `-D warnings`, and boundary checks passed.
- `lake build && lake test`: **14 shared traces passed**. Existing Lean linter
  warnings remain; no new `sorry`, axioms or widened proof claim.
- Real pinned-PDS runtime: C/D workflows passed with independent upstream schema,
  signed CAR and wrong-key verification. Existing PDS prerequisite/cold recovery
  and three backup-negative tests passed again, including fresh-volume restore
  after source removal preserving account/key/credentials/commit/records/blobs.
- `node tests/federation-isolation/run.mjs` passed for two private spaces,
  including actual TLS, direct-IP bypass checks and restricted SQL roles. Testing
  fixed fixture proxy file-capability/directive-order issues and a CA-as-server
  certificate error without disabling certificate or PDS security checks.

CI runs isolation in the scoped federation bucket. These are locally observed
results and CI wiring, **not a remotely observed CI run**. All test databases,
containers, volumes, images and generated credentials are disposable.

## Remaining scope

E still owns independently administered Memory installations, Web/CLI/TUI parity,
publication/proposal journeys, retrieval integration and the concrete tool-free
model adapter. The guarded provider API uses a passive test provider; no commercial
provider was enabled. Namespace ownership and production publication remain gated.
No Notion/startup document, live infrastructure, DNS or customer data was changed.

The [security contract](../../developer/architecture/federation-security.md) and
[operator configuration](../../user/federation-runtime.md) describe key distribution,
revocation/in-flight requests, plaintext administrators, retained offline bytes,
holds and explicit decommission review. Epochs do not fence independently cloned
databases. Full deployment recovery/stale-writer containment, offsite backups,
security review, legal retention policy, sizing, customer outcomes and pilot release
gates remain open. O(history) replay/PDS proof limits remain. Native Spaces,
Neuralese and Aeneas remain deferred.
