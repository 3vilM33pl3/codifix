# M2 model tooling and evidence

The [London](https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit) and [New York](https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit) workbooks model alternative company bases for the same product and global workload. Their forecasts are not additive. Commercial inputs remain planning hypotheses.

## Use and maintain the models

Edit the workbook's Assumptions and Team inputs and select Base, Downside or Upside. The main forecast recalculates immediately. Follow the [refresh guide](refresh-guide.md) to update the captured sensitivity comparisons; their visible stale flag identifies changed inputs.

| Tool | Purpose and write boundary |
| --- | --- |
| [build_model.py](build_model.py) | Generates regional formula manifests and native bootstrap requests locally; makes no remote writes |
| [model_static_check.py](model_static_check.py) | Checks regional formula parity, input coverage, scalar formula structure and dependency cycles |
| [refresh.mjs](refresh.mjs) and [connector-adapter.mjs](connector-adapter.mjs) | Run cases through the existing live engine, restore controls and publish one complete capture through the Google Sheets connector |
| [refresh.test.mjs](refresh.test.mjs) | Tests restoration, concurrent edits, formula drift, stale captures and publication with a fake adapter |
| [polish_model.py](polish_model.py) | Generates formatting and chart updates locally; does not alter model formulas or input values |

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

| Evidence | Meaning |
| --- | --- |
| [London legacy inputs](legacy-london.json), [New York legacy inputs](legacy-newyork.json) | Preserved original workbook cells and formulas used for the audit and regional cost starting points |
| [London metadata](metadata-london.json), [New York metadata](metadata-newyork.json) | Observed original workbook metadata; new M2 tabs are described by the generated manifests |
| [validation-cases.json](validation-cases.json) | Explicit fictional financial fixtures and expected values, separate from commercial assumptions |
| [London validation](london-validation.json), [New York validation](newyork-validation.json) | Native-engine captures for 13 financial fixtures in each region, including check results and restoration evidence |
| [London scenarios](london-scenarios.json), [New York scenarios](newyork-scenarios.json) | Native-engine captures for 17 commercial scenarios in each region, with original inputs, formula fingerprint and capture time |
| [Native safety cases](native-safety.json) | Deliberate failed refresh, missing required input, observed stale warning and verified restoration |
| [London final summary](london-final-summary.json), [New York final summary](newyork-final-summary.json) | Native Base Summary cells captured after restoration; later cosmetic number-format changes do not change these values |
| [Notion summary source](notion-summary.md) | Published business summary matching the repository dossier |

Captured evidence describes its recorded inputs and version. Later workbook edits do not update these files automatically and may make the native comparison table stale. These captures establish model behaviour; they do not establish customer ROI, pricing acceptance or deployment readiness.

Local checks, after manifest generation:

```bash
python3 /home/olivier/Projects/memory/docs/reviews/federation-first/m2-model/model_static_check.py /tmp/m2-london-manifest.json /tmp/m2-newyork-manifest.json
node --test /home/olivier/Projects/memory/docs/reviews/federation-first/m2-model/refresh.test.mjs
```
