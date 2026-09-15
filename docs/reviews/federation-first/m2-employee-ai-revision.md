# M2 revision — Smaller team with substantial employee AI funding

**Historical v2 record:** the working models now include the
[legal and assurance leadership revision](m2-legal-assurance-revision.md).
The eight-person figures and `ai-` captures below remain the completed v2 audit
trail; they are not the current nine-person plus fractional-counsel forecast.

**Status:** completed and validated financial preparation.
**Formula version:** `M2-2026-09-15-v2-ai`.
**Planning date:** 15 September 2026. **Owner:** founder / finance.

This revision adds the cost of internal agentic engineering and business work to
the [M2 financial models](m2-financial-models.md). London and New York remain
alternative company bases for the same global product and customer workload;
their forecasts must not be added together. The smaller team, AI utilisation and
commercial forecast are operating hypotheses, not evidence of equivalent output
from fewer employees.

## Working models, backups and historical evidence

| Company base | Working workbook — original URL retained | Pre-revision M2 v1 backup |
| --- | --- | --- |
| London | [London financial model](https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit) | [15 September 2026 backup](https://docs.google.com/spreadsheets/d/1FgNSmHrD03JBAM0Zkd9raGMkFQR1sJxxoazmc_PDtuM/edit) |
| New York | [New York financial model](https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit) | [15 September 2026 backup](https://docs.google.com/spreadsheets/d/1teBkocvvZALGsP3HDZ5ldmXIlG9xD1OcEo_DVeQFNb4/edit) |

The [backup and preflight record](m2-model/ai-pre-revision-backups.json) confirms
zero formula/immutable-cell mismatches across 17,261 guarded cells per workbook
before revision. The original [London](m2-model/london-scenarios.json) and
[New York](m2-model/newyork-scenarios.json) scenario captures and their
[London](m2-model/london-validation.json) and
[New York](m2-model/newyork-validation.json) validation captures remain historical
v1 evidence; they do not validate the revised formulas.

## Staffing and evidence basis

| Approved active role | Hire month | Preserved salary row |
| --- | ---: | --- |
| Founder / CEO — product direction and early sales | 1 | Founder / CEO |
| Staff engineer — Rust/core platform | 1 | Founding engineer 1 |
| Platform engineer | 2 | Founding engineer 2 |
| Retrieval/ML engineer | 4 | Founding engineer 3 |
| Solutions / deployment engineer | 8 | Forward-deployed / solutions engineer |
| Enterprise / security engineer | 10 | Founding engineer 4 |
| Commercial lead | 12 | Account executive 1 |
| Customer success / support | 16 | Customer success / support |

The approved schedule gives 7 / 8 / 8 employees at year-end and 146 / 242 paid
person-months over 24 / 36 months. Existing regional salaries and employment-cost
factors remain provisional. Original roles and hire dates are preserved. The
historical profile runs through the same forecast and AI policy for comparison.
Explicit inclusion controls gate payroll, recruitment, delivery capacity,
software, workspace and employee AI. Payroll-only savings exclude AI, contractor
and overhead offsets.

[Employee AI sources](m2-model/employee-ai-sources.md) distinguish three kinds of
evidence: Anthropic's vendor-reported USD150–250 monthly developer usage;
Huang's token-spending ambition in the All-In publisher transcript; and the
founder-selected GBP5,000 monthly allowance. Neither external source validates
the allowance, 50% utilisation or staffing reduction. Dated official API rates
are workload references, not fixed 36-month supplier commitments.

## Calculation boundaries and controls

`Assumptions` rows 118–121 select staffing profile, GBP allowance, utilisation
and subscription allocation ceiling. `Team` retains regional salary inputs and
separate lean/historical hire and inclusion columns. The dedicated `Employee AI`
schedule starts each employee's allowance at their effective hire month.

- Expected expense = allowance × utilisation: GBP2,500 per employee/month in
  Base. Subscriptions receive the lesser of GBP250 and expected expense; the
  remainder is incremental metered usage, GBP2,250 in Base. The ceiling is an
  editable allocation hypothesis, not an executed fixed subscription contract.
- Subscription-included consumption is excluded from additional token charges.
  Supplier invoices, rather than API-equivalent estimates, will establish actual
  cash spending. Customer-facing Memory inference, customer-owned runtimes and
  ordinary software tools remain separate.
- Delivery, product and commercial shares allocate the expected AI expense once.
  The usual operating-expense contingency does not apply again to employee AI.
  Contractor costs still cover delivery hours above the salaried capacity.
- Unused allowance = allowance − expected expense. It remains earmarked cash,
  with no expense, supplier payment or accounting provision. The first 24 months'
  unused allowance increases the month-24 cash target. Full-horizon unused
  allowance is a separate disclosure.
- Both workbooks use the same GBP AI economics, converting through the editable
  USD1.3476/GBP planning rate. Existing revenue, invoice and collection mechanics
  remain unchanged.

| Period | Allowance, GBP | Expected expense, GBP | Unused allowance, GBP |
| --- | ---: | ---: | ---: |
| First 24 months | 730,000 | 365,000 | 365,000 |
| Full 36 months | 1,210,000 | 605,000 | 605,000 |

Let `C[t]` be cash before new financing, including opening cash and collections.
The operating buffer is months 25–27 cash operating outflows, excluding capex:

```text
Required net funding = max(
  0,
  -min(C[1] ... C[24]),
  operating buffer + unused AI allowance through month 24 - C[24]
)
Required gross funding = net funding / (1 - financing fee rate)
```

Financing fees and arrival timing remain explicit. Gross spending includes
expected AI expense once; earmarked AI cash appears within closing cash, with
other closing cash shown separately.

## Verified native Base comparison

Rounded to the nearest currency unit. These are captured outputs from the
revised native forecast, with exact Base/live agreement after restoration.
New York's GBP column translates the alternative company forecast.

| Measure | London, GBP | New York, USD | New York, GBP |
| --- | ---: | ---: | ---: |
| Required net financing | 2,577,868 | 3,941,332 | 2,924,704 |
| Required gross financing | 2,630,477 | 4,021,767 | 2,984,392 |
| Months 25–27 operating buffer | 442,972 | 652,076 | 483,879 |
| AI earmark at month 24 | 365,000 | 491,874 | 365,000 |
| Closing cash at month 24 | 807,972 | 1,143,950 | 848,879 |
| Closing cash at month 36 | 1,190,628 | 1,439,111 | 1,067,906 |
| Total cash spending, first 24 months | 2,830,296 | 4,226,377 | 3,136,225 |
| Payroll-only saving, first 24 months | 1,022,500 | 1,591,667 | 1,181,112 |
| Payroll-only saving, full 36 months | 2,265,500 | 3,495,313 | 2,593,731 |
| Revenue-weighted gross margin, 36 months | 65.69% | 63.21% | 63.21% |

Both Base cash paths remain non-negative within the 36-month horizon;
this does not establish indefinite runway. Conditional ARR at month 36 remains
GBP2,240,301 and recognised revenue over 36 months remains GBP2,198,035 in both
locations' common GBP view. The approximately GBP353,915 greater New York gross
funding requirement reflects regional costs and FX, not an additional market or
different employee AI allowance.

Historical net raise references remain £3m/$4.5m. Revised Base net requirements
are £422,132/$558,668 below those references. Compare net with net: the displayed
gross requirement includes the separate 2% financing fee.

### Selected sensitivities in common GBP

All rows run through each workbook's single live engine. Rows change only the
named controls relative to captured Base. Full inputs and all 25 cases are in
the scenario evidence linked below.

| Case | London gross funding, GBP | New York gross funding, GBP | AI spending / headroom through M24, GBP |
| --- | ---: | ---: | ---: |
| Base: eight people, GBP5k allowance, 50% utilisation | 2,630,477 | 2,984,392 | 365,000 / 365,000 |
| 25% utilisation | 2,599,865 | 2,953,780 | 182,500 / 547,500 |
| 100% utilisation | 2,691,702 | 3,045,617 | 730,000 / 0 |
| GBP2k allowance, 50% utilisation | 2,146,804 | 2,500,719 | 146,000 / 146,000 |
| GBP10k allowance, 50% utilisation | 3,436,600 | 3,790,515 | 730,000 / 730,000 |
| Historical staffing, same GBP5k / 50% AI policy | 4,654,066 | 5,268,181 | 595,000 / 595,000 |
| Launch +3 months, delivery effort doubled, 100% utilisation | 3,130,933 | 3,484,848 | 730,000 / 0 |

Moving from 50% to full utilisation consumes the cash already earmarked for
first-24-month AI headroom. It does not add that amount to funding a second time.
At the selected hires, the following three-month operating buffer increases by
GBP60,000; gross funding increases by GBP61,224 after fees.

The combined stress ends M36 with £84,891 in London and **a $50,981 shortfall in
New York**, despite the larger calculated 24-month funding. Its revenue-weighted
gross margins fall to 33.93%/30.64%. The reduced team's delivery capacity and
output remain unproven. The forecast buys contractor hours above salaried
capacity; it does not model automatic engineering productivity gains from tokens.

## Verification, refresh and M3 handoff

The [verified refresh](m2-model/refresh-guide.md) runs each comparison through the
live engine, verifies the formula version, restores original inputs and then
publishes. New cases cover utilisation 25%/50%/100%, allowance
GBP2,000/5,000/10,000, historical staffing under the same AI policy and a combined
three-month launch delay, doubled delivery effort and full utilisation stress.

| Evidence | Result |
| --- | --- |
| Native financial fixtures: [London](m2-model/ai-london-validation.json), [New York](m2-model/ai-newyork-validation.json) | 30 cases and 920 checks per region: original 13 cases retain their 111 expectations under historical staffing/zero AI; 17 new fixtures add 179 independent expectations; each case also passes 21 reconciliations |
| Native business comparisons: [London](m2-model/ai-london-scenarios.json), [New York](m2-model/ai-newyork-scenarios.json) | 25 cases × 21 mandatory checks per region; all inputs restored and all A:Y output columns published |
| [Safety and parity](m2-model/ai-native-safety.json) | Deliberate failed refresh rejected without publication; all 358 inputs restored; missing AI allowance raises native N/A; utilisation and inclusion edits mark captures STALE; restoring inputs returns CURRENT INPUTS; all 20 Base output metrics exactly match live cells |
| Shared assumptions and formulas | 295 non-regional inputs match exactly; 12,086 formulas match across regions; every editable cell is captured; no formula dependency cycles |
| Final Base summaries: [London](m2-model/ai-london-final-summary.json), [New York](m2-model/ai-newyork-final-summary.json) | Native formulas, effective values and formatted outputs after restoration |

London comparison captured **2026-09-15 19:56:49.378 UTC**, source-input
signature `fnv1a32:d23e75cf`; New York **19:58:25.004 UTC**, signature
`fnv1a32:486eab6a`. Both verify formula fingerprint `fnv1a32:48ad0678` and version
`M2-2026-09-15-v2-ai`. Local static checks and all 17 refresh failure-handling
tests pass. Signatures identify captures; they do not validate economic inputs.

Native PDF exports were rendered and visually inspected for the changed Summary,
Assumptions, Team, Employee AI, Costs, P&L & Cash, Sensitivity and Checks areas in
both regions, including the ARR and cash charts. Header contrast was corrected,
new controls use explicit zero/one formatting, and input/source notes remain
available. The browser connection was unavailable, so this was native export
inspection rather than an on-screen Sheets interaction test. The default PDF
paginator splits the charts across pages; their continuations were inspected.
These temporary inspection exports are not a redesigned print deliverable.

The [existing Notion M2 summary](https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a)
and pitch-workspace overview were updated and fetched again to verify content,
links and preserved child pages. [Published source](m2-model/ai-notion-summary.md).
The roadmap links this revision and expands the M3 evidence requirements.
[Publication and visual-inspection record](m2-model/ai-publication-evidence.json).

## Approved-plan completion

- [x] Create dated native backups and preserve v1 captures.
- [x] Implement eight-role hiring and all employee-cost inclusion controls.
- [x] Add employee AI allowance, spending split, allocation and unused cash headroom.
- [x] Include first-24-month headroom in funding and show regional GBP comparisons.
- [x] Extend verified sensitivities and publish all AI/staffing output columns.
- [x] Validate financial cases, restoration, staleness, parity and visual output.
- [x] Update repository, Notion and M3 handoff with incremental commits.

The dev Memory Layer completion check returned `verified_complete: true`, seven
completed items and no remaining items. It archived plan
`e811745e-4d5f-4943-a3d6-3df25f831aef` and recorded implementation
`f69a27fc-46ac-4076-94a9-d6bd06494d10`.

M3 must reconcile paid seats, included consumption, incremental credits/API
charges, actual contracted rates and FX with invoices. Measure engineering
throughput, accepted output, review/rework, defects and delivery capacity beside
per-person/per-workflow AI spending, including parallel agents and retries.
Validate model/tier and context mix before replacing the allowance/utilisation
hypotheses. M3 also retains the original federation delivery-cost and packaging
validation work. M4 updates the deck; M5 supplies customer evidence. This
revision authorises no hiring, procurement, spending or runtime changes.
