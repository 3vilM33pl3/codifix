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
- The existing PostgreSQL backup passed gzip integrity testing. Fresh encrypted
  capture and isolated restore remain required; no restore is claimed yet.
- Root filesystem free space was approximately 3 GB at execution start. New
  build caches and sensitive recovery artifacts belong on `/data`, not the repo.

## In progress

- Read-only, fail-closed schema/partition inventory contracts and tests.
- Debian install hooks that do not rotate credentials or restart unrelated
  services, plus separately configured named-instance units.
- Pure supervisor action/policy/budget contracts. These are not a running Pi
  agent, persistent scheduler, API, or deployed maintenance authority.

Every release and live activation requires evidence in this ledger. Do not mark
the contract complete based on scaffolding or synthetic tests alone. CA console
steps, operator privileges, restore verification and model credential binding
remain explicit operational gates.
