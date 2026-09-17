# Four-area internal Memory network

Status: implementation started 2026-09-17. Not deployed; no release published.

The [approved execution contract](contract.md) is the source of scope and gates.
Trust runs on the console-administered CA; Homelab, Workshop and General run on
Monolith, each with independent application/database boundaries. Supervisors use
`gpt-5.6-sol` under a $10/day fleet ceiling, never the R3 key or allowance.

## Execution evidence

- Approved-plan workflow succeeded on 2026-09-17 at 08:24:15 UTC on
  `codex/federation-v1`, starting from `2da2005`.
- Plan task: `76b69037-b150-4df7-9ebd-d36f0a5c4ee4`; raw capture:
  `a764afa5-2532-48c9-8fd1-5cfcef9eb985`. The checkpoint has 52 checklist items.
- R3 is paused, not complete. Frozen evaluation artifacts, original ballots,
  AI diagnoses and remaining-budget evidence are retained. No R3 launcher was
  found running in the process inventory at execution start.
- The initial source inspection found 105 projects, 3,367 memory versions and
  2,244 captures. These are a live baseline, not a migration snapshot checksum.
- Current service identities on Monolith are distinct, but that is not proof
  their data stores are distinct. Resolve actual database bindings before cutover.
- Read-only connection inventory at 08:50 UTC found 13 local connections and two
  from the CA address (`10.22.6.2`) to the source `memory` database. Application
  names were blank, so the precise remote process is not inferred. A final
  Monolith-only writer stop is insufficient until the CA binding is reconciled.
- Both system and user `memory-layer.service` are active. The user watcher
  manager, Homelab, SCTP, SCTP-conformance, Soundmind, tb-olivier-rnd and a transient
  dish2-tools watcher were active. None were stopped by this implementation.
  The global DuckDB offline store has a live owner and WAL; a project-local dev
  DuckDB also exists. Neither is claimed as a consistent captured offline store.
  The final cutover must fence these writers and capture/reconcile pending work.
- Fresh online baseline encrypted directly to the operator's existing GPG key,
  with selected operator-readable configuration files separately encrypted.
  Protected local directory: `/data/tmp/memory-four-area-baseline-20260917T0831`;
  independently retained NAS copy: `/media/bunnychow/backups/memory-four-area-20260917`.
  The complete database ciphertext SHA-256 is
  `4600e7062ca5dc3a5e4f4850fabb4b9d73b79cb56dc1b18e30a86ca04f832b1c`.
- Isolated database restore passed: **104 tables and 2,331,495 rows** match the
  archive's per-table row-multiset hashes, with zero invalid indexes or
  unvalidated constraints. The sandbox has no network or published ports and
  runs no Memory application, workers, federation or providers. Evidence lives
  in `restore-evidence.json` beside both backup copies. Historical evaluation
  namespace `mab_full_20260516_150014` is included, not silently omitted.
- Initial row hashes differed because source timestamps rendered in
  `Europe/London` and the sandbox defaulted to UTC. Matching the archive timezone
  produced exact hashes without ignoring or normalizing any fields. New captures
  explicitly pin and record UTC. This is database restore evidence, **not**
  application startup, complete host recovery or final writer-stop evidence.
- Root filesystem free space was approximately 3 GB at execution start. New
  build caches and sensitive recovery artifacts belong on `/data`, not the repo.
- Complete protected partition inventory collected from the verified restore:
  104 tables, 2,331,495 rows, 105 public projects and two schemas. Every table's
  before/stream/after row counts agree in one read-only snapshot. The 1,018,087,569
  byte file has SHA-256
  `0b13c72d8075ae76f5f88920943fddf513605f005255eb0251677020c6622905`;
  `partition-inventory.json.collector.json` records its exact serialization and
  provenance. Both are mode 0600 in the protected baseline directory.
- The first real planner validation took 123.86 seconds and peaked at
  1,811,012 KiB RSS. It reported 100 absent-project references across historical
  `loop_produced_memory` (21), `memory_access_events` (12), `memory_score_audit`
  (32) and `procedural_utility_audit` (35). Read-only database checks confirm the
  references are present in the source history and have no declared foreign keys.
  They must remain visible with their original IDs in the protected archive;
  they must not be reinterpreted as current project ownership or erased.
- After adding strict typed retention notices, the unchanged real inventory
  **passes structural validation with zero blockers and 100 notices**. Active
  ownership and actual FK/logical dependencies still fail closed. Omitting or
  altering the notices in a reviewed manifest is rejected. Normalized inventory
  digest: `ca70bd2ed6eecaeead4d680832ce9b3a63e4a5ae285b1d6e55d644c3d394134f`.
  The private `partition-inventory-validation-20260917T091005Z.json` and metrics
  record 129.12 seconds and 1,810,140 KiB peak RSS in the debug build. This is not
  content classification, approval of an area assignment or import readiness.
- Private unresolved draft generated successfully:
  `partition-unresolved-draft-20260917T091005Z.json` (89,680,231 bytes, mode 0600).
  It contains 104 table policies, 3,107 unresolved canonical lineages, 449,441
  unresolved supporting-record decisions, zero project bindings and the same
  100 retention notices. Debug execution took 134.08 seconds and peaked at
  1,877,068 KiB RSS. **No ownership decision, target import or live cutover is
  implied by generating this draft.** Content/provenance review and safe mixed-
  source transformations remain required before an importer can be authorized.
- Closing health checks returned HTTP 200/database-up for the original Monolith
  instances on ports 4040 (`2.0.0`) and 4250 (`2.0.0-dev`), with their original
  identities. No Memory/CA service was stopped, restarted or reconfigured.

## In progress

- Offline fail-closed schema/partition inventory contracts include the exact
  optional eleven-table historical evaluation catalog, never a schema wildcard.
  All 23 planner tests pass, including archive notices and their CLI propagation.
  No live importer yet; unknown tables and unresolved ownership block planning.
- Debian install hooks no longer rotate credentials or restart unrelated
  services; opt-in named-instance units have separate protected configuration
  and pinned verified executables. Lifecycle regression tests pass. Actual-host
  sandbox, signature verification, installation and activation gates remain open.
- Pure supervisor action/policy/budget contracts: 21 tests pass. These are not a running Pi
  agent, persistent scheduler, API, or deployed maintenance authority.
- Both new pure crates pass strict Clippy, formatting and dependency boundaries.
  The encrypted backup/restore/inventory tooling has 22 passing tests.
- CA console preflight: eight stub tests pass. A real non-root run on Monolith
  correctly reports database inspection as permission-denied and binding as
  unknown; this is not a CA observation. The CA console report remains required.
- Release manifest/signature/staging tool: 17 offline tests pass, including
  disposable signing-subkey fixtures and staging-directory substitution tests.
  An independent reviewer reproduced a cleanup race in the initial code; the
  private-parent and directory/file-identity fix passed both regression tests
  and independent re-review before commit. No real release has been signed,
  published, installed or granted automatic-install authority. The initial
  four-area deployment is approval-required, not a maintenance update.
- Pi/OpenAI integration is paused at the explicit credential-selection gate.
  An unfinished three-file RPC-probe draft is preserved outside the active
  workspace at `/data/tmp/memory-supervisor-pi-paused-20260917`; it was not built,
  tested, committed or configured with credentials. No supervisor or evaluation
  model calls have been launched by this implementation.
  Reuse of the normal Memory key versus a separate supervisor key remains an
  operator choice. R3 credentials and allowances remain out of scope.

Next: review and classify the protected source by content/provenance; implement
and test lossless import/reconciliation and mixed-source handling; obtain the CA
console inventory and complete the writer-stop/host/offline backup gates. Model
integration additionally awaits the credential choice. Durable supervision,
Web/TUI agent workflows, real releases, enrollment and the live four-area demo
remain unimplemented or unverified. No full-workspace/database/browser/Lean
release-gate pass is claimed from the focused tests above.

Every release and live activation requires evidence in this ledger. Do not mark
the contract complete based on scaffolding or synthetic tests alone. CA console
steps, operator privileges, restore verification and model credential binding
remain explicit operational gates.
