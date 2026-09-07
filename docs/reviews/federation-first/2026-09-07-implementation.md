# Federation-first implementation checkpoint — 2026-09-07

Status: **first increment complete; full roadmap and product release incomplete**.
The [approved checklist](README.md) remains the execution authority. A1–A4 and
R2 are complete at their stated scope. B1/B3 have an attribution increment, not
the full production domain. No customer or production federation is enabled.

## Committed evidence

| Commit | Verified increment |
| --- | --- |
| `4cec6c3` | Checkpointed the approved replacement roadmap before implementation |
| `3287c04` | Encrypted PDS cold backup; remove original test volume; restore and authenticate/publish from a fresh volume |
| `59a5638` | Pin PDS/SDK/specification inputs, enforce image lock, exercise generated SDK requests and document schema/namespace policy |
| `4e11288` | Count evaluation task costs once, separate step diagnostics, reject legacy mixed comparisons at release gates |
| `a27fec6` | Authenticate publisher separately from original author in the reducer; preserve attribution through replay; add two shared traces and a membership-authority theorem |
| `e9962cc` | Align Lake package/root naming and configuration with the pinned action's checker discovery |

### PDS prerequisite

The isolated upstream-PDS runner passed provisioning, activation, actual adapter
schema validation, signed CAR verification, wrong-key rejection, migration/key
rotation and encrypted cold recovery. Restoration verified pre-backup account
passwords, access/refresh credentials, signing key, commit, records and blob bytes,
then performed administration and a new signed publication. Corrupted archives
and wrong keys failed before extraction. Generated Docker resources were removed.

This is a cold, bounded synthetic-fixture test, not offsite backup operations,
point-in-time recovery, full Memory restoration or stale-writer fencing. PDS raw
reads remain public inside the private network: gateway isolation is mandatory.
See [fixture details](../../../tests/federation-pds/README.md).

### Domain and conformance

The neutral crate remains runtime/database/AT-type free. `AuthenticatedRecord`
is a caller-established identity boundary, not a credential verifier. New paths
use publisher capabilities while preserving original authorship; accepted records
cannot be reattributed to another publisher. Authenticated replay retains both.
The legacy spike convenience API still assumes author equals publisher and must
not be used for production ingestion.

Nine common JSON traces pass in Rust and Lean, including an unenrolled original
expert whose record is published by an enrolled instance, duplicate replay,
subsequent publisher revocation and rejected forged authority authorship. The
new Lean theorem requires root publisher identity for unseen membership changes.
Existing proof boundaries remain: no cryptography, HTTP, runtime or deployment
proof. The extra Rust reattribution defense is not separately formalized.

### Evaluation accounting

A parent reporting 30 tokens with 10- and 20-token steps contributes 30, not 60,
to the comparison. Whole-task outcomes, sample counts and durations no longer
mix with child diagnostics. `step:` groups are explicitly non-additive; legacy
comparison JSON must be regenerated from original runs before gating. This fixes
accounting, not benchmark validity or missing provider-usage capture.

## Local validation

- `node tests/federation-pds/backup.test.cjs`: three tests pass.
- `node tests/federation-pds/run.mjs`: full disposable PDS gate passes, including
  image-lock enforcement and cold recovery after original-volume removal.
- `cargo test -p mem-federation -p mem-atproto --all-targets --locked --offline`:
  25 neutral tests, 24 adapter tests and the nine-fixture conformance harness pass.
- `cargo test -p mem-eval --all-targets --locked --offline`: 21 tests pass.
- `cargo test -p mem-cli --bin memory --locked --offline eval`: five tests pass.
- Workspace tests pass with isolated temporary XDG config/state directories,
  `MEMORY_LAYER_CLIENT_TOKEN` and database variables unset, and
  `--test-threads=1`; loopback test servers require execution outside the socket
  sandbox. **Database-gated tests did not exercise PostgreSQL.**
- `cargo fmt --check`, workspace Clippy with `-D warnings`,
  `scripts/check-boundaries.sh` and `git diff --check` pass.
- Lean 4.33.1 `lake build`, `lake test` and `lake env leanchecker` pass. Existing
  proof-linter warnings remain; no `sorry` or new axiom was added.
- Remote CI and the independent nanoda run have **not** been executed here. The
  pinned official action remains configured with `nanoda-allow-sorry: false`.

Initial broad tests exposed host-token/config contamination and prohibited
loopback sockets, not failures in the new attribution/accounting paths. Isolation
and the scoped socket permission rerun resolved those failures. An empty test
database variable is not equivalent to an unset variable; use `/usr/bin/env -u`
for non-database checks. No real user config, service token or database was changed.

## Next implementation boundary

Continue B1/B2/B3: stable instance/space identity, origin-qualified streams,
shared versions independent of local versions, state revisions, reviewed evidence,
proposals, relation retraction and authority ordering with explicit permission
mapping and expanded common traces. Then implement the transactional foundations
in C/R1 and exercise them against a disposable real database before gateway and UI
integration. The in-memory spike reducer is not a production persistence design.

Durable jobs/outbox/inbox, gateway isolation/authentication, two-instance workflows,
Web/CLI/TUI parity, held-out quality evaluations, full restore and release gates
remain open. Native Spaces runtime compatibility is untested and restricted to
disposable data. Financial models, pitch deck and startup documents are unchanged;
commercial validation and paid pilot outcomes have not been fabricated. No live
deployment, DNS/Lexicon publication, customer commitment, hiring or spend occurred.
