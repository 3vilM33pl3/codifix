# M2 execution and validation evidence

**Status:** completed financial preparation · approved and delivered 2026-09-15

## Approved-plan checklist

- [x] Back up both native workbooks, capture source assumptions and retain original URLs.
- [x] Build shared editable commercial assumptions, pilot/cohort/renewal engine and regional team/overhead inputs, with sources and hypotheses.
- [x] Reconcile annual platform revenue, invoices, collections, receivables, deferred revenue, onboarding and usage; derive consistent retention and ARR.
- [x] Build all direct federation costs and allocate payroll once; calculate real gross margin and operating cash.
- [x] Calculate 24-month funding plus months 25–27 cash buffer, financing timing/fees and complete use of funds.
- [x] Implement live scenarios and verified sensitivity refresh with original-state restoration, stale detection and regional comparison.
- [x] Validate contract/accounting, cost/billing, zero/delay/failure/support/collections cases, scenario parity and visually inspect both workbooks.
- [x] Publish repository specification/evidence and Notion M2 summary, update roadmap handoffs, make incremental/final commits and verify approved plan.

## Sources and safeguards

The dev Memory Layer accepted the eight-item plan before implementation. Native
workbook backups were created before changing the working models. Source
Assumptions/Team snapshots and sheet metadata are in [m2-model](m2-model/).
See the [model specification](m2-financial-models.md) for working and backup URLs.

This work changes financial preparation and documentation only. It does not
close deployment, customer-readiness, pricing validation or hiring/spending gates.

## Validation record

The checks below ran against the native Google Sheets engine. Fictional financial
fixtures establish calculation behaviour; they are not commercial/customer data.

| Validation | Observed result | Evidence |
| --- | --- | --- |
| Shared formula structure | 10,743 identical formulas, 328 uniquely captured editable cells per region; no dependency cycles | `model_static_check.py`; both generated manifests |
| Source preservation | All 18 regional salaries and hire months plus 14 regional overhead inputs retained; source URLs preserved and dated backups created first | Legacy/metadata JSON and linked native backups |
| Annual contract | One M5 pilot converts at M7; annual invoices, service-year recognition, ARR and deferred revenue reconcile | `single_contract` fixture |
| Renewal and cash collection | Anniversary churn/expansion and lagged cash reconcile independently | `renewal_and_collections_lag` fixture |
| Separate fees and usage | Pilot service, onboarding, monthly usage, AR and unbilled usage reconcile; allowance remains per account | `pilot_onboarding_usage`, `overage_allowance_per_account` |
| Deployments and replication | Additional deployments affect recurring commitment and applicable costs; replicas affect traffic/storage without a second knowledge sale | `deployment_and_replica`; `extra_replica` sensitivity |
| Failed exchanges | Attempts and support effort include failures/retries | `failed_exchanges`; doubled-failure sensitivity |
| Delay, zero sales and readiness | Delays shift starts, pending/out-of-horizon activity stays visible; zero sales and closed gate remain cost-bearing | `pipeline_beyond_horizon`, `earliest_start_shifts_pipeline`, `funding_zero_sales`; readiness/zero/delay sensitivities |
| Funding | Opening cash, fees, peak interim deficit, capex exclusion from operating buffer and late financing all tested | Five `funding_*` fixtures and `financing_arrival_too_late` |
| Cash and allocation | Fourteen mandatory checks include cash, AR, deferred revenue, service recognition, payroll allocation, retention and sources/uses | Checks B5:B18, tolerance 0.00001 |
| Commercial sensitivities | 17 cases per workbook, including doubled delivery effort and slower collections, each through the live engine | Regional `*-scenarios.json` |
| Base parity | Captured ARR and other Base outputs equal the restored live forecast exactly | Scenario Base rows and `*-final-summary.json` |
| Failed refresh and missing input | Deliberate mismatch prevents publication and restores inputs; missing required price raises native N/A rather than zero | `native-safety.json` |
| Stale result detection | Price mutation displays STALE; restoring original controls returns CURRENT INPUTS; both retained Base snapshots | `native-safety.json` |

Each workbook passed **13 fixtures with 111 independent expected-value checks**,
plus 14 mandatory reconciliations in every fixture: 293 verified checks per
workbook. Each also passed 17 sensitivity cases × 14 reconciliations = 238 checks.
All captures restored inputs without conflicts. The 17 local refresh tests cover
transport errors, concurrent edits, formula drift, uncertain publication,
restoration failure, current-baseline transforms and nullable gross margin.

Snapshot formula fingerprint: `fnv1a32:15dd894c` in both regions. London capture
`2026-09-15T17:55:19.163Z`, input signature `fnv1a32:d2de28fa`; New York capture
`2026-09-15T17:56:34.048Z`, signature `fnv1a32:046ada8d`. Inputs are retained in
the JSON; row input maps are sparse overrides relative to `originalInputValues`.
The native stale flag covers editable inputs; formula verification is performed
during refresh, not continuously by the input stale flag.

## Visual inspection and delivery

All ten tabs in both working books were inspected in the native Sheets UI at
100% zoom: README, Summary, Assumptions, Team, Revenue, Costs, Cohorts, P&L & Cash,
Sensitivity and Checks. The inspection confirmed readable headers/labels, blue
editable inputs, actual monthly dates, regional currencies, negative-number and
percentage formats, explicit current snapshot status and check residuals.

Two original charts in each Summary were retargeted to all 36 months of live ARR
and financed closing cash, positioned below the tables, and inspected visually.
Chart currencies and dates match the workbook. Cash charts show the later
shortfall. Final cosmetic changes corrected retention precision, snapshot column
widths/wrapping and the numeric recalculation sentinel; no formulas or economic
inputs changed after verification. Temporary compact toolbar views were restored.

Published [M2 business summary](https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a)
under the existing pitch workspace. The parent links M2; the earlier financial
page retains its contents with a superseded/historical banner. All three pages
were fetched after publication to verify structure, links and content. The exact
new-page source is [notion-summary.md](m2-model/notion-summary.md).

## Outcome and next work

The [specification and regional comparison](m2-financial-models.md) report Base
gross funding of £3,312,229.27 and $5,291,141.57. This covers the approved
24-month need and operating buffer, not 36 months: both first have negative cash
in month 31. M3 validates inputs/packaging and recalculates funding; M4 revises
the deck; M5 supplies customer evidence. No runtime changes, hiring, spending,
customer readiness or pricing acceptance is inferred.

Incremental commits preserve source audit, refresh logic, model generation,
native validation and final publication separately. Approved-plan completion is
verified through the dev Memory Layer before final delivery. The completion
command returned `verified_complete: true`, eight completed items and no remaining
items; it archived plan `70260f4e-cfd1-4026-9025-7567560ca7e1` and recorded
implementation `f8671780-2e7e-4c5f-86c6-d4fc3e12513c`.
