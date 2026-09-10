# R3 execution and evidence ledger

Status: implementation in progress; label approval and paid runs not authorized.
Approved plan checkpoint: `r3-isolated-evaluation`, plan memory
`201e0e95-7691-4abc-924a-0cbe55ccc53b` (2026-09-10).

## Delivery gates

- [x] Checkpoint approved contract and inspect existing evaluation machinery.
- [x] Operator-only frozen semantic clock and no-learning service profile.
- [ ] Sealed/reset disposable datasets and restricted agent containers.
- [ ] Equal-information document, lexical, vector and full-Memory baselines.
- [ ] Synthetic development/held-out knowledge and coding review pack.
- [ ] Campaign plan/run/report, provenance, limits and fail-closed gates.
- [ ] Offline fault/isolation/statistical tests and integration evidence.
- [ ] Olivier approves labels; freeze suite only after approval.
- [ ] Approve and run paid development calibration.
- [ ] Separately approve and run official campaign; publish measured outcome.

## Contract

Knowledge: 40 development questions in 10 scenarios; 100 held-out questions in
25 scenarios. Coding: 4 development and 12 held-out three-step Rust maintenance
sequences. Entire scenario/repository families are disjoint. Three official
repeats; sequence children and repeats are not independent samples.

Conditions: maintained handover, simple lexical, simple vector, full Memory,
and diagnostic no-memory. Same facts, model, reasoning and task budgets. No
retrieval tuning or R4/R5/R6/F expansion. Synthetic outcomes are not customer ROI.

Defaults: knowledge >=90%, coding >=80%; >=5 percentage-point improvement over
each informed baseline with positive simultaneous 95% cluster-bootstrap lower
bounds per track. Zero designated critical failures. Cost and median latency
<=2x a development-selected, pre-frozen baseline. Missing evidence invalidates a
campaign. Valid negative/inconclusive results do not pass product quality.

## Initial coverage inventory

| Component | Existing behavior | R3 obligation |
| --- | --- | --- |
| `mem-search` recency | Rust wall clock | One semantic timestamp per query |
| Reinforcement ranking | SQL `now()` decay | Same timestamp as Rust; retain seeded scores |
| Query hooks | Reinforcement access and timeline writes | Suppress measurement-induced learning/context |
| Service workers | Background/scheduler/federation lifecycle | Disable during measurement, including reload |
| Existing Docker eval stack | Shared database, host CLI bootstrap | Dedicated resettable, sealed and restricted harness |
| Existing canary gate | Draft labels, mainly no-memory comparator | Preserve compatibility; not R3 evidence |
| Eval comparison | R2 parent-level costs | Preserve; cluster repeats/scenarios for inference |
| Agent sequence runner | Shared workspace inside sequence | Retain only within sequence; hidden independent grader |
| Artifacts | Optional configuration metadata | Required hashed campaign provenance; fail closed |

## Approval and safety boundaries

The offline harness must not load developer provider credentials or contact paid
providers. Snapshot provisioning uses disposable resources only. Operational
journals may change, but must not enter benchmark context; sealed knowledge and
reinforcement state must not change. Real clocks remain in authentication,
leases and latency measurement. No user query parameter can change semantic time.

Implementation completion, valid experiment completion and product-quality pass
are distinct. No official result may be claimed from fake/offline runs or draft
labels. No existing deployment is changed by this work.

## 2026-09-10 implementation increment

The service/search clock and no-learning changes are implemented and tested with
disposable PostgreSQL. Measurement startup also refuses privileged/unsealed DB
roles. Source/reinforcement DML is denied; allowed operational journals remain
outside benchmark content. No existing deployment was restarted or migrated.

Pure baseline comparators, campaign manifests/approval contracts, conservative
reservations, clustered reports and CLI plan/report interfaces are implemented.
An offline driver interface is present but **the full driver is not implemented**;
paid dispatch fails before execution. These are not complete R3 end-to-end results.

The draft review generator prepares all 156 tasks. All 48 coding reference
graders pass. The draft knowledge scorer still requires semantic/citation
adjudication; coding difficulty is explicitly limited to small functions.
See `evals/r3/README.md` for the remaining integration list. R3 stays unchecked.
