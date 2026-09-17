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

## In progress

- Offline fail-closed schema/partition inventory contracts: 15 tests pass.
  No live importer yet; unknown tables and unresolved ownership block planning.
- Debian install hooks no longer rotate credentials or restart unrelated
  services; opt-in named-instance units have separate protected configuration
  and pinned verified executables. Lifecycle regression tests pass. Actual-host
  sandbox, signature verification, installation and activation gates remain open.
- Pure supervisor action/policy/budget contracts: 21 tests pass. These are not a running Pi
  agent, persistent scheduler, API, or deployed maintenance authority.
- Both new pure crates pass strict Clippy, formatting and dependency boundaries.
  The encrypted backup/restore tooling has ten passing tests.

Every release and live activation requires evidence in this ledger. Do not mark
the contract complete based on scaffolding or synthetic tests alone. CA console
steps, operator privileges, restore verification and model credential binding
remain explicit operational gates.
