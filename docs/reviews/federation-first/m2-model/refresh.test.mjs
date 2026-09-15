import test from 'node:test';
import assert from 'node:assert/strict';
import {fingerprint, refreshModel, snapshotIsCurrent, snapshotWrites} from './refresh.mjs';

const fixedNow = () => '2026-09-15T12:00:00.000Z';

function fixture(options = {}) {
  const cells = {
    'Assumptions!B3': 'Upside',
    'Assumptions!B4': 'test-v1',
    'Assumptions!B10': 80000,
    'Assumptions!B11': 0.5,
    'Team!B5': 120000,
    'Revenue!B2': '=Assumptions!B10*Assumptions!B11',
    'Checks!B2': '=0',
  };
  const manifest = {
    modelVersion: 'test-v1',
    editableCells: ['Assumptions!B3', 'Assumptions!B10', 'Assumptions!B11', 'Team!B5'],
    controlCells: ['Assumptions!B3', 'Assumptions!B10', 'Assumptions!B11'],
    baseControls: {'Assumptions!B3': 'Base'},
    formulaCells: {'Revenue!B2': cells['Revenue!B2'], 'Checks!B2': '=0'},
    immutableCells: {'Assumptions!B4': 'test-v1'},
    outputCells: {arr36: 'Revenue!B2'},
    checks: [{cell: 'Checks!B2', expected: 0, tolerance: 0.000001, label: 'cash bridge'}],
    cases: [
      {id: 'base', label: 'Base', inputs: {}},
      {id: 'price', label: 'Package +25%', inputTransforms: {'Assumptions!B10': {multiply: 1.25}}},
      {id: 'conversion', label: 'Conversion 25%', inputs: {'Assumptions!B11': 0.25}},
    ],
  };
  const writes = [];
  const publications = [];
  let currentCase = null;
  const adapter = {
    async readCells(requested, {mode}) {
      const values = {};
      for (const cell of requested) {
        values[cell] = mode === 'value' && cell === 'Revenue!B2'
          ? cells['Assumptions!B10'] * cells['Assumptions!B11']
          : mode === 'value' && cell === 'Checks!B2' ? 0 : cells[cell] ?? null;
      }
      if (options.read) await options.read({requested, mode, values, cells, currentCase});
      return values;
    },
    async writeCells(patch, meta) {
      if (meta.caseId) currentCase = meta.caseId;
      writes.push({patch: {...patch}, meta});
      Object.assign(cells, patch);
      if (options.write) await options.write({patch, meta, cells});
    },
    async publishSnapshot(snapshot) {
      publications.push(structuredClone(snapshot));
    },
  };
  return {cells, manifest, writes, publications, adapter};
}

test('all cases use the same live engine, reset overrides, restore original selection, and publish once', async () => {
  const f = fixture();
  const original = structuredClone(f.cells);
  const snapshot = await refreshModel({...f, now: fixedNow});
  assert.deepEqual(snapshot.rows.map(row => row.outputs.arr36), [40000, 50000, 20000]);
  assert.deepEqual(f.cells, original);
  assert.equal(f.publications.length, 1);
  assert.equal(snapshot.capturedAt, fixedNow());
  assert.equal(snapshot.restoration.restored, true);
  assert.equal(snapshot.sourceInputSignature, fingerprint(snapshot.originalInputValues));
  assert.equal(f.writes.at(-1).meta.purpose, 'restore');
});

test('a failed check restores inputs and never publishes partial comparisons', async () => {
  const f = fixture({read({mode, values, currentCase}) {
    if (mode === 'value' && currentCase === 'price') values['Checks!B2'] = 10;
  }});
  const original = structuredClone(f.cells);
  await assert.rejects(refreshModel({...f}), error => {
    assert.match(error.message, /failed cash bridge/);
    assert.equal(error.details.restoration.restored, true);
    assert.equal(error.details.published, false);
    return true;
  });
  assert.deepEqual(f.cells, original);
  assert.equal(f.publications.length, 0);
});

test('transport error after applying case values still restores attempted writes', async () => {
  const f = fixture({write({meta}) {
    if (meta.caseId === 'price') throw new Error('response lost after mutation');
  }});
  const original = structuredClone(f.cells);
  await assert.rejects(refreshModel({...f}), /response lost/);
  assert.deepEqual(f.cells, original);
  assert.equal(f.publications.length, 0);
});

test('formula drift fails before any write', async () => {
  const f = fixture();
  f.cells['Revenue!B2'] = '=999';
  await assert.rejects(refreshModel({...f}), /formulas or version differ/);
  assert.equal(f.writes.length, 0);
  assert.equal(f.publications.length, 0);
});

test('formula drift during a run prevents publication after restoring controls', async () => {
  const f = fixture({read({mode, cells, currentCase}) {
    if (mode === 'value' && currentCase === 'price') cells['Revenue!B2'] = '=999';
  }});
  await assert.rejects(refreshModel({...f}), /formulas or version differ/);
  assert.equal(f.cells['Assumptions!B3'], 'Upside');
  assert.equal(f.cells['Assumptions!B10'], 80000);
  assert.equal(f.publications.length, 0);
});

test('concurrent control edit is preserved while other refresh controls restore', async () => {
  let changed = false;
  const f = fixture({read({mode, cells, currentCase}) {
    if (mode === 'value' && currentCase === 'price' && !changed) {
      cells['Assumptions!B10'] = 123456;
      changed = true;
    }
  }});
  await assert.rejects(refreshModel({...f}), error => {
    assert.match(error.message, /Concurrent input edit/);
    assert.deepEqual(error.details.restoration.conflicts, ['Assumptions!B10']);
    assert.deepEqual(error.details.restoration.notRestored, ['Assumptions!B10']);
    return true;
  });
  assert.equal(f.cells['Assumptions!B10'], 123456);
  assert.equal(f.cells['Assumptions!B3'], 'Upside');
  assert.equal(f.cells['Assumptions!B11'], 0.5);
  assert.equal(f.publications.length, 0);
});

test('concurrent Team edit survives and invalidates the snapshot', async () => {
  const f = fixture({read({mode, cells, currentCase}) {
    if (mode === 'value' && currentCase === 'price') cells['Team!B5'] = 150000;
  }});
  await assert.rejects(refreshModel({...f}), /Concurrent input edit/);
  assert.equal(f.cells['Team!B5'], 150000);
  assert.equal(f.cells['Assumptions!B3'], 'Upside');
  assert.equal(f.cells['Assumptions!B10'], 80000);
  assert.equal(f.publications.length, 0);
});

test('restoration write failure is explicit and carries original controls for recovery', async () => {
  const f = fixture({write({meta, cells}) {
    if (meta.purpose === 'restore') {
      cells['Assumptions!B3'] = 'Base';
      throw new Error('restoration unavailable');
    }
  }});
  await assert.rejects(refreshModel({...f}), error => {
    assert.match(error.message, /restoration unavailable/);
    assert.equal(error.details.restoration.restored, false);
    assert.equal(error.details.originalControls['Assumptions!B3'], 'Upside');
    return true;
  });
  assert.equal(f.publications.length, 0);
});

test('staleness includes all editable inputs, scalar type, model version and formulas', async () => {
  const f = fixture();
  const snapshot = await refreshModel({...f, now: fixedNow});
  const live = {...snapshot.originalInputValues};
  assert.equal(snapshotIsCurrent(snapshot, live, 'test-v1', snapshot.formulaFingerprint), true);
  for (const cell of Object.keys(live)) {
    const changed = {...live, [cell]: `${live[cell]} changed`};
    assert.equal(snapshotIsCurrent(snapshot, changed, 'test-v1'), false, cell);
  }
  assert.equal(snapshotIsCurrent(snapshot, {...live, 'Assumptions!B11': '0.5'}, 'test-v1'), false);
  assert.equal(snapshotIsCurrent(snapshot, live, 'test-v2'), false);
  assert.equal(snapshotIsCurrent(snapshot, live, 'test-v1', 'different'), false);
});

test('non-finite output or spreadsheet error does not become a zero result', async () => {
  const f = fixture({read({mode, values}) {
    if (mode === 'value') values['Revenue!B2'] = '#DIV/0!';
  }});
  await assert.rejects(refreshModel({...f}), /non-numeric arr36/);
  assert.equal(f.cells['Assumptions!B3'], 'Upside');
  assert.equal(f.publications.length, 0);
});

test('a manifest cannot mutate an input outside its declared controls', async () => {
  const f = fixture();
  f.manifest.cases[0].inputs = {'Team!B5': 0};
  await assert.rejects(refreshModel({...f}), /unsupported control/);
  assert.equal(f.writes.length, 0);
});

test('refresh starts from edited live assumptions and applies relative sensitivity to that baseline', async () => {
  const f = fixture();
  f.cells['Assumptions!B10'] = 120000;
  f.cells['Assumptions!B11'] = 0.75;
  const snapshot = await refreshModel({...f});
  assert.deepEqual(snapshot.rows.map(row => row.outputs.arr36), [90000, 112500, 30000]);
  assert.equal(snapshot.rows[1].inputs['Assumptions!B10'], 150000);
  assert.equal(f.cells['Assumptions!B10'], 120000);
  assert.equal(f.cells['Assumptions!B11'], 0.75);
  assert.equal(snapshotIsCurrent(snapshot, snapshot.originalInputValues, f.manifest.modelVersion), true);
});

test('explicitly nullable ratio allows zero-revenue n.a.; other outputs still require numbers', async () => {
  const f = fixture({read({mode, values}) {
    if (mode === 'value') values['Revenue!B2'] = '';
  }});
  f.manifest.nullableOutputKeys = ['arr36'];
  const snapshot = await refreshModel({...f});
  assert.deepEqual(snapshot.rows.map(row => row.outputs.arr36), [null, null, null]);
});

test('snapshotWrites builds one complete batch and preserves native stale-check formulas', async () => {
  const f = fixture();
  f.manifest.publication = {
    sheet: 'Sensitivity', resultStartRow: 8, resultEndRow: 12,
    resultColumns: ['label', 'capturedAt', 'caseInputs', 'arr36', 'modelVersion', 'status', 'notes'],
    captureCells: Object.fromEntries(f.manifest.editableCells.map((cell, index) => [cell, `Sensitivity!AB${index + 8}`])),
    capturedAtCell: 'Sensitivity!B4', signatureCell: 'Sensitivity!B5', formulaFingerprintCell: 'Sensitivity!B6',
  };
  const snapshot = await refreshModel({...f, now: fixedNow});
  const writes = snapshotWrites(snapshot, f.manifest);
  assert.equal(writes["'Sensitivity'!D8"], 40000);
  assert.equal(writes["'Sensitivity'!D9"], 50000);
  assert.equal(writes["'Sensitivity'!A11"], null);
  assert.equal(writes["'Sensitivity'!AB8"], 'Upside');
  assert.equal(writes["'Sensitivity'!B4"], fixedNow());
  assert.equal(Object.keys(writes).some(cell => cell.includes('!AC') || cell.endsWith('!B3')), false);
  assert.throws(() => snapshotWrites({...snapshot, rows: snapshot.rows.slice(1)}, f.manifest), /complete, restored/);
});

test('final publication transport uncertainty leaves restored controls and explicit reread instruction', async () => {
  const f = fixture();
  f.adapter.publishSnapshot = async () => { throw new Error('network timeout'); };
  await assert.rejects(refreshModel({...f}), error => {
    assert.match(error.message, /read the capture metadata/);
    assert.equal(error.details.publicationStatus, 'unknown');
    assert.equal(error.details.restored, true);
    return true;
  });
  assert.equal(f.cells['Assumptions!B3'], 'Upside');
  assert.equal(f.cells['Assumptions!B10'], 80000);
});

test('per-case expected financial values are checked and captured with evidence', async () => {
  const f = fixture();
  f.manifest.cases[0].checks = [{cell: 'Revenue!B2', expected: 40000, tolerance: 0.01, label: 'single contract ARR'}];
  const snapshot = await refreshModel({...f});
  assert.equal(snapshot.rows[0].verifiedChecks.length, 2);
  assert.deepEqual(snapshot.rows[0].verifiedChecks[1], {
    cell: 'Revenue!B2', expected: 40000, tolerance: 0.01, label: 'single contract ARR', actual: 40000,
  });
  f.manifest.cases[0].checks[0].expected = 41000;
  await assert.rejects(refreshModel({...f}), /failed single contract ARR/);
});

test('visible case inputs show labelled changes instead of the full control JSON', async () => {
  const f = fixture();
  f.cells['Assumptions!B3'] = 'Base';
  f.manifest.scenarioCell = 'Assumptions!B3';
  f.manifest.inputLabels = {'Assumptions!B10': 'Annual platform package (GBP/year)'};
  f.manifest.cases.push({id: 'downside', inputs: {'Assumptions!B3': 'Downside'}});
  f.manifest.publication = {
    sheet: 'Sensitivity', resultStartRow: 8, resultEndRow: 12,
    resultColumns: ['label', 'capturedAt', 'caseInputs', 'arr36'],
    captureCells: Object.fromEntries(f.manifest.editableCells.map((cell, index) => [cell, `Sensitivity!AB${index + 8}`])),
  };
  const snapshot = await refreshModel({...f, now: fixedNow});
  const writes = snapshotWrites(snapshot, f.manifest);
  assert.equal(writes["'Sensitivity'!C8"], 'Base assumptions at capture');
  assert.equal(writes["'Sensitivity'!C9"], 'Annual platform package (GBP/year): 100000');
  assert.equal(writes["'Sensitivity'!C10"], 'Assumptions!B11: 0.25');
  assert.equal(writes["'Sensitivity'!C11"], 'Selected case: Downside');
  assert.equal(Object.keys(snapshot.rows[1].inputs).length, f.manifest.controlCells.length);
});
