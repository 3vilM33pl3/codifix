# M2 sensitivity refresh

The comparison tables are **captured results from the workbook's single live forecast**. They do not contain a second forecast engine. Editable assumptions recalculate the main forecast immediately; the comparison table needs this verified refresh before its results are current.

## Files and connector contract

- [refresh.mjs](refresh.mjs) runs the cases and handles restoration. It has no network, credential or Node dependencies.
- [connector-adapter.mjs](connector-adapter.mjs) uses the existing Google Drive/Sheets connector. No Apps Script deployment, new authentication or add-on is required.
- The region's generated `manifest-london.json` or `manifest-newyork.json` supplies the observed workbook and sheet IDs, expected formula text, editable cells, controls, cases, checks and publication layout.
- [refresh.test.mjs](refresh.test.mjs) tests failure handling and the adapter contract with a fake workbook. These tests complement the separately recorded native workbook validation.

Adapters return every requested cell, using `null` for a blank. Formula-mode reads return the entered formula text or raw scalar; value-mode reads return the effective result. Spreadsheet errors must be raised, rather than converted to zero. Writes use one native `batchUpdate` containing only the requested cells; `null` clears a cell.

## Before running

1. Coordinate a **quiescent workbook**: nobody should edit its inputs or formulas while cases run. Google Sheets offers no cell compare-and-swap lock. The routine detects observable concurrent edits and preserves conflicting user values, but cannot eliminate a race between a read and a write.
2. Use the manifest belonging to the workbook. Check its `spreadsheetId` and sheet name/ID mapping against live metadata. Never substitute the other region's workbook ID.
3. Keep the original manifest formula version. A formula change requires review, a regenerated manifest and validation; accepting the changed formula text as expected merely to bypass an error defeats verification.
4. Read the forecast's validation and funding warnings. A reconciliation failure prevents capture. A financing-timing warning is meaningful scenario evidence and must remain visible in the main forecast.

## Execute through Codex

The following is an execution recipe for `functions.exec`. The helper reads repository text in bounded chunks so a large formula manifest cannot be silently truncated by shell output limits. It evaluates only these trusted repository modules, never spreadsheet content. Change `region` to `newyork` for the other workbook.

```javascript
// @exec: {"yield_time_ms": 1000, "max_output_tokens": 2000}
const directory = '/home/olivier/Projects/memory/docs/reviews/federation-first/m2-model/';
const region = 'london';
const shellQuote = value => "'" + value.replaceAll("'", "'\"'\"'") + "'";
async function readRepositoryText(filename) {
  const source = 'import json,pathlib,sys; s=pathlib.Path(sys.argv[1]).read_text(); start=int(sys.argv[2]); end=min(start+16000,len(s)); print(json.dumps({"text":s[start:end],"next":end,"total":len(s)}))';
  let offset = 0;
  let text = '';
  for (;;) {
    const response = await tools.exec_command({
      cmd: 'python3 -c ' + shellQuote(source) + ' ' + shellQuote(directory + filename) + ' ' + offset,
      max_output_tokens: 20000,
    });
    if (response.exit_code !== 0) throw new Error(response.output);
    const page = JSON.parse(response.output);
    text += page.text;
    if (page.next === page.total) return text;
    if (page.next <= offset) throw new Error('Repository read did not advance');
    offset = page.next;
  }
}
const refreshSource = await readRepositoryText('refresh.mjs');
const adapterSource = await readRepositoryText('connector-adapter.mjs');
const manifest = JSON.parse(await readRepositoryText('manifest-' + region + '.json'));
const {refreshModel, snapshotWrites} = new Function(
  refreshSource.replace(/^export /gm, '') + '\nreturn {refreshModel, snapshotWrites};'
)();
const {createConnectorAdapter} = new Function(
  adapterSource.replace(/^export /gm, '') + '\nreturn {createConnectorAdapter};'
)();
const adapter = createConnectorAdapter(tools, manifest, snapshotWrites);
const snapshot = await refreshModel({manifest, adapter});
store('m2-refresh-' + region, snapshot);
text({
  region, cases: snapshot.rows.length, capturedAt: snapshot.capturedAt,
  sourceInputSignature: snapshot.sourceInputSignature,
  formulaFingerprint: snapshot.formulaFingerprint,
  modelVersion: snapshot.modelVersion, restoration: snapshot.restoration,
});
```

If the execution yields a running cell, use `functions.wait` for that cell in bounded waits. Keep the workbook quiescent until execution finishes. A successful return means the complete publication request was acknowledged; read back the Sensitivity metadata, Base result and live stale flag to confirm the published state.

For a local harness, import `refreshModel` and `snapshotWrites` as ES modules and supply the same adapter contract. Native validation fixtures can append `scenario.checks` and omit `adapter.publishSnapshot`; this tests the live engine and restores inputs without replacing the commercial sensitivity table.

## What is verified

- Before any mutation, entered formula text and immutable model version must exactly match the manifest. The formula fingerprint is a deterministic change identifier, not a cryptographic security claim.
- Every editable Assumptions and Team cell is captured. Each case starts from those **current live values**, with only intentional sparse controls such as the Base scenario selector reset. Generator price defaults cannot replace edited working assumptions.
- Relative `inputTransforms` apply to the captured baseline, not the preceding case. For example, a 25% price increase uses the currently edited price multiplied by `1.25`.
- Each case recalculates the same live engine, reads finite numeric outputs and checks reconciliation cells. Case-specific financial fixtures can add expected values and tolerances. Their actual values are retained in `verifiedChecks`.
- A gross-margin ratio with no revenue has no meaningful denominator. Only the manifest's explicitly nullable output keys accept blank/null; the comparison displays `n.a.`. Spreadsheet errors and other missing outputs still fail capture.
- Input checks surround case writes and output reads. Formula and input verification run again after all original controls have been restored. The snapshot is published only after the complete run passes.

## Native publication and stale state

`snapshotWrites(snapshot, manifest)` prepares a single atomic batch:

| Native area | Purpose |
| --- | --- |
| `Sensitivity!A8:Q31` | Complete case table; unused result rows are cleared |
| `Sensitivity!B4` | Capture timestamp |
| `Sensitivity!B5` | Captured source-input signature |
| `Sensitivity!B6` | Verified formula fingerprint |
| `Sensitivity!AA8:AA…` | Generator-owned source-cell labels |
| `Sensitivity!AB8:AB…` | Captured values for every editable input |
| `Sensitivity!AC8:AC…` | Generator-owned, type-aware live-versus-captured comparisons |
| `Sensitivity!B3` | Generator-owned visible stale flag |

The writer changes result values, capture values and metadata; it preserves the native stale formulas. Each result includes its scenario inputs and model version. A timestamp alone never establishes freshness: changing any supported editable input makes the capture stale until another successful refresh. Formula drift is separately checked against the manifest during refresh; the input stale flag is not a full continuous formula audit.

## Failure and recovery

- A failed case or read prevents publication. `finally` attempts to restore original controls even when a write returned an error after applying its values.
- Restoration writes only values still recognisable as refresh-owned. A concurrent user edit is left intact; the error reports conflicts, cells not restored and the captured original controls. Reconcile those cells before another run. Do not copy all original inputs over the workbook.
- If restoration cannot be verified, treat the workbook as needing recovery. Read the reported cells, inspect the preserved control values and restore only values whose ownership is clear. Retain the failure record with validation evidence.
- A timeout on the final atomic publication has an **unknown acknowledgement status**: the complete write may already have succeeded. Controls are restored. Read the timestamp, signature, fingerprint and rows before deciding whether to rerun.
- After recovery, rerun the entire refresh. Never combine partial case results from different runs or publish them individually.

Run the focused local checks with:

```bash
node --test docs/reviews/federation-first/m2-model/refresh.test.mjs
```
