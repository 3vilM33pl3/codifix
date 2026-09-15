# M2 — Agentic Enterprise financial models

**Status:** implementation and validation in progress · 2026-09-15

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

- **M3:** validate provider rates, workload, support capacity, packaging, payment
  terms and sensitivity assumptions; recalculate funding before hiring decisions.
- **M4:** use the reconciled financial outputs and explicit assumptions in the
  deck; avoid presenting scenarios as forecasts validated by customers.
- **M5:** replace hypotheses with discovery, signed terms, deployed-pilot meters,
  invoices, observed conversion and renewal evidence.

See [execution and validation evidence](m2-execution.md). M2 completion requires
native-workbook checks, scenario restoration/staleness tests, visual inspection,
publication and approved-plan verification.
