# M2 — Agentic Enterprise financial models

**Status:** completed financial preparation · 2026-09-15

[Notion business summary](https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a)
· [model tooling and evidence](m2-model/README.md)
· [verified refresh procedure](m2-model/refresh-guide.md)

Two alternative company bases deliver the same Codifix product and global
customer workload. These are conditional planning models, not company actuals,
customer commitments, validated prices or investment offers. Do not add their
forecasts together. [M1 commercial basis](m1-finance-pitch.md) remains the source
of product positioning and the separation of customer value from vendor cost.

## Workbooks and historical evidence

| Company base | Working model (existing URL preserved) | Dated pre-M2 backup |
| --- | --- | --- |
| London | [London model](https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit) | [2026-09-15 backup](https://docs.google.com/spreadsheets/d/1u14XLZQm8YsMyeYpLUdjerVUTZIaR4x_z7aTqm6aBnw/edit) |
| New York | [New York model](https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit) | [2026-09-15 backup](https://docs.google.com/spreadsheets/d/1ZfEpxpD-tQTY1AtSoYNgknsithA3QbxqF8h9ao6qu5U/edit) |

The bounded native source snapshots in [m2-model](m2-model/) preserve the original
Assumptions and Team values, formulas and displayed values. The full native
backups preserve the earlier forecasts. No source claim becomes validated merely
because it appeared in an old model.

| Observed legacy problem | M2 treatment |
| --- | --- |
| London 175 seats × 4,000 operations/month × 12 × GBP0.35/1,000 = GBP2,940, independent of GBP80,000 ACV | GBP80,000 becomes an explicit platform-package hypothesis; legacy operations do not establish a billable question or token |
| New York 100,000 operations/seat/month, 25 times London; USD94,500 usage equivalent independent of USD100,000 ACV | Both regions use identical workload and commercial assumptions, with stated FX |
| 140% stated renewal factor versus approximately 137.966% from monthly expansion/churn formulas | Renewal-anniversary arithmetic: 95% retained accounts × 145% retained-account value = 137.75% |
| Mid-year ARR expansion recognised revenue without a matching contract/invoice bridge | One cohort ledger owns annual contract value, recognised revenue and invoices |
| Sensitivity used a quarterly approximation that differed from the monthly forecast | Every saved case runs through the same live monthly engine |
| 90%/70% gross-margin assumptions manufactured costs | Direct costs and payroll allocation determine gross margin |
| Use of funds contained operating expenses only | Total operating cash spending includes delivery costs, with funding and collections shown separately |
| Hardcoded calendar and sales gating dropped delayed additions | Real dates and a shifted pilot funnel preserve pending activity |
| Named design-partner conversion and self-serve growth lacked customer evidence | Preserve as historical hypotheses; enterprise funnel contains no named customer commitment |

## Approved commercial defaults

- Forecast begins 1 October 2026 and spans 36 months.
- One paying enterprise account includes two independently administered deployments.
- GBP80,000 annual platform fee; GBP is the common commercial comparison currency.
- Initial USD/GBP planning rate 1.3476, dated 14 September 2026. It is a fixed,
  editable snapshot, not a live FX feed or forecast. [Bank of England source](https://www.bankofengland.co.uk/boeapps/database/Rates.asp?into=GBP&rateview=A)
- Existing quarterly acquisition volumes become provisional pilot-start volumes;
  pilots last two months and convert at an unvalidated 50% Base rate. First pilot
  month five is conditional on technical/customer readiness, not evidence the
  gates have passed.
- Base includes no pilot, onboarding or usage-overage receipts. Separate controls
  allow those pricing hypotheses to be tested. Actual delivery workload still
  creates cost. Self-serve is excluded from the active enterprise forecast.
- Legacy 45% retained-account expansion and 5% logo churn remain visible
  hypotheses, applied at annual renewal rather than continuously each month.
- Existing regional salary, hiring and overhead assumptions remain provisional.
  Roles are planned positions, not employment commitments or actual pay records.
- Codifix funds Memory service costs in the Base assumption. Ownership controls
  distinguish customer-paid costs; customers' existing agent runtimes remain
  outside Codifix delivery revenue and cost.

## Financial definitions

Recurring contracted platform and additional-deployment fees form ARR. Pilot and
onboarding fees are non-recurring; uncommitted usage overages do not form ARR.
Annual invoices cover a service year and platform revenue is recognised monthly
across that year. Onboarding and pilot fees have separately identified service
timing. This is an explicit management-model convention pending actual contract
terms, not an assertion of audited accounting compliance.

Accounts may be fractional because this is an expected-value forecast. Retention
and expansion operate on a cohort at its renewal anniversary. The assumptions
register must distinguish commercial price/value expansion from physical
workload growth so additional value does not imply free delivery capacity.

Receivables reconcile opening balance + invoices − collections. Deferred revenue
reconciles unearned annual commitments to service delivered. Closing cash equals
opening cash + collections + financing − cash outflows. No financing plug hides a
cash shortfall; timing assumptions remain visible.

Direct delivery cost includes deployment, identity/key lifecycle, gateway/PDS,
database/compute, inference and failed attempts, replica storage, transfer,
recovery, security review, curation, administration and multi-party support.
Allocate payroll once; model extra workload above funded delivery capacity as
additional cost rather than charging the same salary twice. Replication can
increase cost without producing a second sale of the same knowledge.

For the funding calculation, let `C[t]` be closing cash without the proposed
raise, including stated opening cash. Let `buffer` be operating cash outflows in
months 25–27. The required net funding is:

```text
max(0, -min(C[1..24]), buffer - C[24])
```

Gross up for explicit financing fees; retain an editable financing month and
show any shortfall before financing arrives. Compare this requirement with the
historical GBP3m/USD4.5m raise assumptions. Gross use-of-funds percentages use
total cash spending, including delivery; net funding need also accounts for
collections and the cash buffer.

## Evidence rules and handoffs

Every input records a value, unit, source, date, owner role and evidence status.
Legacy values and new workload/effort estimates are labelled hypotheses. Unknown
values cannot silently become zero. No calculated customer ROI follows from this
vendor forecast; recovered customer staff capacity is not vendor revenue or
customer cash savings.

## Captured Base comparison

London capture: 2026-09-15 17:55:19 UTC; New York: 17:56:34 UTC. Both use
`M2-2026-09-15-v1`, identical formula fingerprint `fnv1a32:15dd894c`, and the
commercial defaults above. These are calculated hypotheses, not customer-backed
forecasts. Exact inputs and outputs are in the regional scenario JSON files.

| Measure | London GBP | New York USD | New York in GBP |
| --- | ---: | ---: | ---: |
| Month 36 ARR | 2,240,301 | 3,019,030 | 2,240,301 |
| Revenue across 36 months | 2,198,035 | 2,962,072 | 2,198,035 |
| Revenue-weighted gross margin, 36 months | 70.56% | 68.05% | 68.05% |
| Cash spending, months 1–24 | 3,622,437 | 5,576,433 | 4,138,048 |
| Customer collections, months 1–24 | 1,060,400 | 1,428,995 | 1,060,400 |
| Month 24 operating buffer | 683,948 | 1,037,881 | 770,170 |
| Required net funding | 3,245,985 | 5,185,319 | 3,847,817 |
| Financing fees at 2% of gross | 66,245 | 105,823 | 78,527 |
| Required gross funding | 3,312,229 | 5,291,142 | 3,926,344 |
| Closing cash at month 36 | (31,712) | (412,870) | (306,375) |

Both have 22.38 expected paying accounts, eight active pilots and 18 planned staff
at month 36. Both first have negative cash in month 31: **30 months of modelled
runway**. The funding requirement covers the approved first 24 months and buffer;
it does not finance the whole 36-month horizon. Base financing arrives in month
one, with zero opening unrestricted cash as a visible hypothesis.

The historical £3m/$4.5m inputs are **net raise references**. Required net funding
exceeds them by £245,984.68/$685,318.74. They must not be compared directly with
gross funding before fees.

### Regional bridge in common GBP

New York needs £614,115.03 more gross funding: £515,611.20 additional spending
through month 24, £86,221.53 additional operating buffer, and £12,282.30 additional
fees. The spending difference comprises payroll £393,103.67, recruitment
£72,658.41, and other overheads/contingency £49,849.12. Common-GBP customer
collections and external service workload costs are identical. This isolates
regional hiring/overhead assumptions and FX from commercial demand.

The first 24 months' cash spending includes all delivery costs. For London it
reconciles as £2,120,750 product payroll + £432,000 GTM payroll + £232,250 delivery
payroll + £54,943.62 external delivery + £227,400 recruitment/commissions +
£555,093 other operating costs = £3,622,436.62. New York uses the same categories
and reconciles to $5,576,433.24. Cash spending less collections plus the operating
buffer gives the net funding figures above; financing fees are separate.

## Workbook map and operating conventions

| Tab | Purpose |
| --- | --- |
| README | Editing instructions, evidence boundaries and accounting conventions |
| Assumptions | Base/Downside/Upside input register; B3 selects the live scenario; E is active |
| Team | Regional salaries, hire months, delivery allocation and available delivery capacity |
| Cohorts | Acquisition-month annual commitments, accounts, renewal, ARR, invoices and service revenue |
| Revenue | Pilot funnel, deployment/workload counts, billing, receipts and working-capital balances |
| Costs | Resource meters, direct service costs, human workload, reserved payroll and contractor shortfall |
| P&L & Cash | Cost-derived gross margin, expenses, operating cash, financing and closing balances |
| Summary | Annual results, 24-month funding, gross uses, GBP comparison outputs and charts |
| Sensitivity | Dated captures from the same live engine, full captured inputs and stale status |
| Checks | Fourteen mandatory reconciliations/input checks plus explicit scenario warnings |

There are 328 supported editable input cells and 10,743 formulas per workbook.
The source register supplies unit, source, date, accountable owner role and
evidence status; all unsupported cost and workload figures remain visible
hypotheses. Team hire months are in AM, delivery shares in AN. Monthly columns
are C:AL. Frozen date/header rows and blue input text support navigation.

Annual services begin on conversion and renew on the acquisition anniversary.
The model records usage overages per account after its own allowance, invoices
them the following month, and tracks earned but unbilled usage separately. Pilot
fees are recognised across pilot operation; onboarding is recognised at paid
conversion. These service conventions require validation against actual terms.

Physical workload growth is independent of price expansion. Active pilots consume
deployment and human capacity even where they generate no receipts. Failed and
retried questions remain in resource meters. Salaried delivery capacity is paid
even when underused; extra contractor cost covers only the unmet workload.

The optional customer agent-runtime cost rate stays `PENDING` while customers
own that spend. Assigning any share to Codifix requires a numeric rate; required
missing inputs generate errors instead of zero. Opening receivables and deferred
revenue are explicitly fixed at zero for this new-company forecast. These are
pre-tax management models: VAT/sales tax, debt, dividends and historical opening
working capital are outside scope. Monthly payment timing is simplified.

### Interfaces to the next milestones

- **M3:** validate provider rates, workload, support capacity, packaging, payment
  terms and sensitivity assumptions; recalculate funding before hiring decisions.
- **M4:** use the reconciled financial outputs and explicit assumptions in the
  deck; avoid presenting scenarios as forecasts validated by customers.
- **M5:** replace hypotheses with discovery, signed terms, deployed-pilot meters,
  invoices, observed conversion and renewal evidence.

See [execution and validation evidence](m2-execution.md). M2 establishes usable,
reconciled financial preparation; subsequent milestones must replace hypotheses
with external evidence before financial commitments.
