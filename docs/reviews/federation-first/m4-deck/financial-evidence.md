# M4 financial evidence

Verified **17 September 2026, 11:47:38 UTC** through read-only native Google Sheets connector calls. Live financial reads completed at 11:45:28 UTC. No workbook input, formula, refresh or publication changes were made.

**Result:** Both workbooks remain Base, `M2-2026-09-15-v3-assurance`. Planned gross funding is **£3.23m London or $4.91m New York**, including 2% financing fees. These are alternative company bases for the same commercial forecast. They are not additive and do not set a fixed fundraising ask.

## Live sources and freshness

| Item | London | New York |
| --- | --- | --- |
| Workbook | [London M2](https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit?ouid=117996706485490200256) | [New York M2](https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit?ouid=117996706485490200256) |
| Source capture, `Sensitivity!B4` | 2026-09-15T20:38:21.652Z | 2026-09-15T20:38:16.635Z |
| Drive modified time | 2026-09-15T20:52:45.765Z | 2026-09-15T20:52:47.691Z |
| Source input signature, `Sensitivity!B5` | `fnv1a32:0c1db4bc` | `fnv1a32:352e5e9c` |
| Active case, `Assumptions!B3` / `Summary!B3` | Base | Base |
| Capture stale flag, `Sensitivity!B3` | CURRENT INPUTS / FORMULAS VERIFIED AT REFRESH | CURRENT INPUTS / FORMULAS VERIFIED AT REFRESH |
| Live formulas independently fingerprinted | 12,354 | 12,354 |
| Live formula fingerprint | `fnv1a32:d74d6a28` | `fnv1a32:d74d6a28` |
| Inputs matching saved v3 capture | 371 / 371 | 371 / 371 |
| Base outputs matching saved scenario and final capture | 25 / 25 | 25 / 25 |
| Live reconciliations within 0.00001 | 23 / 23 | 23 / 23 |

The source captures remain dated 15 September; this review does not replace those timestamps with today's date. Every live formula matches the current generated manifest fingerprint. All inputs match the saved captures, so the captures are not stale relative to current formulas and inputs. This does not validate the commercial or supplier assumptions.

Formula/version sources: `Summary!B4`, `Assumptions!B4`, `Sensitivity!B6` and `Sensitivity!O8`. Input capture comparisons occupy `Sensitivity!AA8:AC378`. Reconciliations are `Checks!B5:B18` and `Checks!B28:B36`. FNV-1a32 is a deterministic change detector, not a cryptographic guarantee.

## Funding and forecast traceability

Values below are rounded to the nearest reporting-currency unit. Exact native values and formulas are retained in [financial-evidence.json](financial-evidence.json).

| Measure | London GBP | New York USD | Exact source cell in each workbook |
| --- | ---: | ---: | --- |
| Gross funding incl. 2% fees | 3,226,268 | 4,913,193 | `Summary!B23` |
| Net financing | 3,161,743 | 4,814,929 | `Summary!B22` |
| Financing fees | 64,525 | 98,264 | `Summary!B24` |
| Months 25–27 operating buffer | 504,025 | 743,765 | `Summary!B25` |
| M24 earmarked AI cash | 420,000 | 565,992 | `Summary!B74` |
| M24 closing cash | 924,025 | 1,309,757 | `Summary!B27` |
| M36 closing cash | 1,062,469 | 1,238,160 | `Summary!B28` |
| Cash uses, M1–24 | 3,298,118 | 4,934,167 | `Summary!B42` |
| Collections, M1–24 | 1,060,400 | 1,428,995 | `Summary!B43` |
| AI expense, M1–24 | 420,000 | 565,992 | `Summary!B73` |
| Counsel fees, M1–24 (already included) | 120,000 | 180,000 | `Summary!B58` |
| G&A payroll, M1–24 (already included) | 286,000 | 435,417 | `Summary!B48` |
| G&A AI, M1–24 (already included) | 55,000 | 74,118 | `Summary!B59` |
| Recognised revenue, M1–36 | 2,198,035 | 2,962,072 | `Summary!B50` |
| ARR, M36 | 2,240,301 | 3,019,030 | `Summary!E6` |
| Gross margin, 36-month revenue weighted | 65.69% | 63.21% | `Summary!B51` |
| Expected paying accounts, M36 | 22.38 | 22.38 | `Summary!E7` |
| Live deployments including pilots, M36 | 60.76 | 60.76 | `Summary!E8` |
| Year-end employee headcount | 8 / 9 / 9 | 8 / 9 / 9 | `Summary!C18:E18` |

Gross funding equals net financing divided by 98%. The net requirement funds the first 24 months and the M24 target of months 25–27 operating outflows plus unused AI allowance through M24, after opening cash and collections. Formula chain: `Summary!B22:B25`, `Summary!B74`, `Summary!B84`, `'P&L & Cash'!C27:Z27`. Unused AI cash is retained cash, not another expense.

New York gross funding in common GBP is **£3,645,884** at the model's **$1.3476/£** rate (`Summary!B32`, `Assumptions!E11`). Its GBP M36 ARR is the same **£2,240,301** (`Summary!B52`). Do not add either pair of regional figures.

## Annual forecast

Periods are **Oct 2026–Sep 2027 / Oct 2027–Sep 2028 / Oct 2028–Sep 2029**, from `Summary!C5:E5`.

| Measure | London GBP: Y1 / Y2 / Y3 | New York USD: Y1 / Y2 / Y3 | Source |
| --- | ---: | ---: | --- |
| Ending recurring ARR | 160,000 / 900,400 / 2,240,301 | 215,616 / 1,213,379 / 3,019,030 | `Summary!C6:E6` |
| Recognised revenue | 42,000 / 551,855 / 1,604,180 | 56,599 / 743,680 / 2,161,793 | `Summary!C9:E9` |
| Ending funded cash | 2,002,128 / 924,025 / 1,062,469 | 3,020,946 / 1,309,757 / 1,238,160 | `Summary!C17:E17` |
| Expected employee AI spending | 157,500 / 262,500 / 270,000 | 212,247 / 353,745 / 363,852 | `Summary!C19:E19` |
| Annual gross margin | -80.80% / 52.08% / 74.20% | -101.42% / 48.02% / 72.74% | `Summary!C11:E11` |

ARR is recurring annual commitment at period end; it is not recognised revenue. Customer counts are expected cohort outputs, not signed customers. The third-year gross margin differs from the 36-month revenue-weighted margin.

## Matching commercial controls

All **304 shared editable inputs match exactly**. The 67 documented regional inputs are employee salaries, the historical raise reference, regional employment/overhead rows 70–83 and counsel fees. Formula structures match across both regions.

| Common Base control | Live value | Active source |
| --- | ---: | --- |
| Annual platform package | £80,000 per account | `Assumptions!E12` |
| Included / actual deployments | 2 / 2 | `Assumptions!E13:E14` |
| Additional deployment fee | £10,000 per year | `Assumptions!E15` |
| Onboarding / pilot / overage fees | 0 / 0 / 0 | `Assumptions!E16:E17`, `E20` |
| Earliest pilot / pilot duration | M5 / 2 months | `Assumptions!E21`, `E24` |
| Conditional readiness switch | 1 | `Assumptions!E23` |
| Pilot-to-paid conversion | 50% | `Assumptions!E25` |
| Annual logo churn / retained commitment expansion | 5% / 45% | `Assumptions!E26:E27` |
| Derived net revenue retention | 137.75% | `Assumptions!E109` |
| Receipt lag | 1 month | `Assumptions!E29` |
| Quarterly pilot start plan | 0, 1, 3, 4, 4, 4, 5, 5, 6, 7, 7, 8 | `Assumptions!E95:E106` |
| Customer runtime spend / Codifix ownership | PENDING / 0% | `Assumptions!E63:E64` |

The readiness switch is a forecast convention. It does not evidence operational readiness. The package, conversion, expansion and cohort volumes remain unvalidated commercial hypotheses.

## Employee AI and staffing

| Common AI policy | Value | Source |
| --- | ---: | --- |
| Monthly allowance per included employee | £5,000 | `Assumptions!E119` |
| Expected utilisation | 50% | `Assumptions!E120` |
| Expected expense per employee-month | £2,500 | `'Employee AI'!C9` |
| Subscription allocation / incremental metered allocation | £250 / £2,250 | `'Employee AI'!C10:C11` |
| Unused allowance per employee-month | £2,500 | `'Employee AI'!C12` |
| Employee-months, M1–24 / M1–36 | 168 / 276 | sums of `'Employee AI'!C5:Z5` / `C5:AL5` |
| Total allowance, M1–24 / M1–36 | £840,000 / £1,380,000 | sums of `'Employee AI'!C28:Z28` / `C28:AL28` |
| Expected spending, M1–24 / M1–36 | £420,000 / £690,000 | sums of `'Employee AI'!C26:Z26` / `C26:AL26` |
| Unused allowance, M1–24 / M1–36 | £420,000 / £690,000 | sums of `'Employee AI'!C27:Z27` / `C27:AL27` |

Both regions have identical GBP AI economics. NY reporting amounts use the same planning FX rate. Spending is allocated once across delivery, product, GTM and G&A. Earmarked unused allowance does not incur an additional expense or payment. These amounts do not prove engineering productivity or capacity.

## Counsel and assurance

| Item | London | New York | Source |
| --- | ---: | ---: | --- |
| Fractional counsel monthly retainer | £5,000 | $7,500 | `Assumptions!E126` |
| Counsel start / inclusion | M1 / 1 | M1 / 1 | `Assumptions!E127:E128` |
| Accounting/payroll excluding counsel | £2,500/month | $3,500/month | `Assumptions!E78` |
| Head of Security & AI Governance salary | £130,000/year | $190,000/year | `Team!B46` |
| Lead start / inclusion | M3 / 1 | M3 / 1 | `Team!AM46`, `Team!AQ46` |
| Lead delivery allocation / cost function | 0% / G&A | 0% / G&A | `Team!AN46:AO46` |
| Loaded salary factor | 1.20 | 1.25 | `Assumptions!E70` |
| Historical initial external-assurance allowance | £45,000 in M14 | $60,000 in M14 | `Assumptions!E79:E80` |

The plan has nine employees plus fractional counsel. Counsel is a purchased service and is excluded from employee headcount and employee AI. The assurance lead is included in both. Full initial and recurring ISO 27001, ISO 42001 and SOC 2 Type II fees remain **unvalidated** (`Summary!B70`, `Assumptions!A130:B130`). Staffing and a cost allowance do not establish certification or a SOC report.

## Captured stress and boundaries

The existing combined three-month launch delay, doubled delivery effort and 100% AI utilisation capture requires gross funding of **£3,734,377 / $5,597,920**, yet ends M36 with **£120,768 / $356,370 shortfalls**. Sources: `Sensitivity!A32:C32`, `G32`, `I32`, `O32:P32`. This is a retained scenario capture, not a newly executed stress run.

No discrepancies were found against the saved v3 Base outputs, source signatures, current formula manifests or the proposed £3.23m/$4.91m funding figures. This review does not rerun the historical 42 fixtures or 28 scenarios, validate vendor quotes or commercial outcomes, or check final deck transcription. The final deck should be checked against this evidence after generation.

