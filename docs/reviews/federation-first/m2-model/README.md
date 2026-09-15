# M2 model tooling and evidence

The [London](https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit) and [New York](https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit) workbooks model alternative company bases for the same product and global workload. Their forecasts are not additive. Commercial inputs remain planning hypotheses.

## Use and maintain the models

Current specification: [eight-person team and employee AI revision](../m2-employee-ai-revision.md).
The original M2 specification and unprefixed JSON captures remain historical v1
evidence. Current revision captures use `ai-`; both workbook URLs are unchanged.

Edit the workbook's Assumptions and Team inputs and select Base, Downside or Upside. The main forecast recalculates immediately. Follow the [refresh guide](refresh-guide.md) to update the captured sensitivity comparisons; their visible stale flag identifies changed inputs.

| Tool | Purpose and write boundary |
| --- | --- |
| [build_model.py](build_model.py) | Generates regional formula manifests and native bootstrap requests locally; makes no remote writes |
| [model_static_check.py](model_static_check.py) | Checks regional formula parity, input coverage, scalar formula structure and dependency cycles |
| [refresh.mjs](refresh.mjs) and [connector-adapter.mjs](connector-adapter.mjs) | Run cases through the existing live engine, restore controls and publish one complete capture through the Google Sheets connector |
| [refresh.test.mjs](refresh.test.mjs) | Tests restoration, concurrent edits, formula drift, stale captures and publication with a fake adapter |
| [polish_model.py](polish_model.py) | Generates formatting and chart updates locally; does not alter model formulas or input values |
| [migrate_ai_revision.py](migrate_ai_revision.py) | One-time reviewed v1-to-v2 delta, requiring preflight manifests and current input captures; never a routine refresh |
| [build_ai_validation.py](build_ai_validation.py) and [AI fixtures](ai-validation-cases.json) | Reproduce 17 AI/staffing fixtures and adapt the original 13 checks without overwriting historical evidence |
| [Employee AI sources](employee-ai-sources.md) | Dated official rates, vendor usage and strategic ambition, separate from founder budget hypotheses |

Generate the manifests with the actual supported command:

```bash
python3 /home/olivier/Projects/memory/docs/reviews/federation-first/m2-model/build_model.py --region both --output-dir /tmp
```

This emits `m2-london-manifest.json`, `m2-newyork-manifest.json` and matching `m2-*-batch.json` files under `/tmp`. Generated manifests are not stored in this directory. The generator reads the committed legacy and metadata snapshots beside its source; it does not read or reset current workbook inputs.

**Bootstrap batches must not be reapplied to the existing rebuilt workbooks.** They contain `addSheet`, unmerge and content-clearing requests intended for the original rebuild. Routine refresh uses only the manifest and the refresh adapter, preserving current economic inputs and formulas.

To regenerate optional display updates after the manifests exist:

```bash
python3 /home/olivier/Projects/memory/docs/reviews/federation-first/m2-model/polish_model.py
```

This emits `/tmp/m2-london-polish.json` and `/tmp/m2-newyork-polish.json`. Applying them is a separate connector action that changes formatting and chart specifications/positions. Verify the current sheet and chart IDs before applying; the script's chart IDs were observed on 15 September 2026. Polish is not required for an ordinary sensitivity refresh.

## Evidence index

### Current employee AI revision — v2

The revision is complete. Both native workbooks have passed 30 financial
fixtures and 920 numerical checks per region, including 17 employee AI/staffing
fixtures and the 13 original cases run under historical staffing with zero
employee AI allowance. All 25 business comparisons passed 21 reconciliations per
case and restored inputs; final summaries, safety and publication evidence follow.

| Evidence | Meaning |
| --- | --- |
| [Pre-revision backups and preflight](ai-pre-revision-backups.json) | Dated native v1 backups and verified formula/input state before the employee AI revision |
| [AI and staffing fixtures](ai-validation-cases.json), [fixture builder](build_ai_validation.py) | Seventeen fictional cases with 179 independent arithmetic checks; builder adapts the original thirteen cases without changing their 111 expected checks or overwriting v1 evidence |
| [London revision validation](ai-london-validation.json), [New York revision validation](ai-newyork-validation.json) | Passed native captures for all 30 fixtures and 920 numerical checks per region, including reconciliations and input restoration |
| [London revision scenarios](ai-london-scenarios.json), [New York revision scenarios](ai-newyork-scenarios.json) | Verified captures for 25 comparisons through the revised live engine, with full input restoration |
| [Revision safety cases](ai-native-safety.json) | Verified failed-refresh restoration, stale detection, required-input failures, Base parity and shared regional inputs |
| [London revision final summary](ai-london-final-summary.json), [New York revision final summary](ai-newyork-final-summary.json) | Restored Base captures, including employee AI expense, earmarked cash, payroll savings and revised funding |
| [Revision Notion summary source](ai-notion-summary.md) | Published matching business summary in the existing Codifix pitch workspace |
| [Publication and visual review](ai-publication-evidence.json) | Notion readback, native PDF inspection coverage, format corrections and capture timestamps |

### Historical source and original M2 evidence — v1

These unprefixed captures describe the earlier model. They remain available for
audit and comparison and do not validate the current employee AI formulas.

| Evidence | Historical meaning |
| --- | --- |
| [London legacy inputs](legacy-london.json), [New York legacy inputs](legacy-newyork.json) | Original pre-M2 workbook cells and formulas used for the audit and regional cost starting points |
| [London metadata](metadata-london.json), [New York metadata](metadata-newyork.json) | Original workbook metadata before the initial M2 rebuild; current tabs are described by generated manifests |
| [Original financial fixtures](validation-cases.json) | Thirteen fictional v1 financial cases and expected values, preserved unchanged |
| [London v1 validation](london-validation.json), [New York v1 validation](newyork-validation.json) | Original native-engine captures for thirteen financial fixtures in each region |
| [London v1 scenarios](london-scenarios.json), [New York v1 scenarios](newyork-scenarios.json) | Original captures for seventeen commercial comparisons per region, with recorded inputs, version and capture time |
| [V1 native safety cases](native-safety.json) | Original failed-refresh, missing-input, stale-warning and restoration evidence |
| [London v1 final summary](london-final-summary.json), [New York v1 final summary](newyork-final-summary.json) | Original Base captures before the staffing and employee AI revision |
| [V1 Notion summary source](notion-summary.md) | Historical business summary matching the original M2 dossier |

Captured evidence describes its recorded inputs and version. Later workbook edits do not update these files automatically and may make the native comparison table stale. These captures establish model behaviour; they do not establish customer ROI, pricing acceptance or deployment readiness.

Local checks, after manifest generation:

```bash
python3 /home/olivier/Projects/memory/docs/reviews/federation-first/m2-model/model_static_check.py /tmp/m2-london-manifest.json /tmp/m2-newyork-manifest.json
node --test /home/olivier/Projects/memory/docs/reviews/federation-first/m2-model/refresh.test.mjs
```
