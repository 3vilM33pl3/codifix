/**
 * Verified sensitivity capture through one live Google Sheets forecast.
 *
 * No network or Node dependencies: callers inject connector adapters. Run with
 * the workbook quiescent. Google Sheets has no cell compare-and-swap; the checks
 * below detect observable edits, but cannot lock out an edit between a read and
 * write. writeCells and publishSnapshot must each use one atomic batch request.
 * The latter is invoked only after every case passes and controls are restored.
 *
 * adapter.readCells(cells, {mode: 'formula' | 'value'}) -> {A1: scalar}
 *   Formula mode returns formula TEXT where present, otherwise the raw scalar.
 *   Return null for a blank cell; never omit requested keys.
 * adapter.writeCells({A1: scalar}, {purpose: 'case' | 'restore'})
 * adapter.recalculate?() -> optional calculation barrier (not a blind sleep)
 * adapter.publishSnapshot?(snapshot) -> one atomic, final publication batch
 */

export class RefreshError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = 'RefreshError';
    this.details = details;
  }
}

const owns = (object, key) => Object.prototype.hasOwnProperty.call(object, key);
const equal = (left, right) => left === right;

function ordered(object) {
  return Object.fromEntries(Object.keys(object).sort().map(key => [key, object[key]]));
}

/** A deterministic change detector, not a cryptographic security signature. */
export function fingerprint(values) {
  const text = JSON.stringify(ordered(values));
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `fnv1a32:${(hash >>> 0).toString(16).padStart(8, '0')}`;
}

/** Used by tests or an adapter; native stale formulas compare every input too. */
export function snapshotIsCurrent(snapshot, inputValues, modelVersion, formulaFingerprint) {
  if (snapshot.modelVersion !== modelVersion) return false;
  if (formulaFingerprint !== undefined && snapshot.formulaFingerprint !== formulaFingerprint) return false;
  const captured = snapshot.originalInputValues;
  if (!captured || Object.keys(captured).length !== Object.keys(inputValues).length) return false;
  return Object.keys(captured).every(cell => owns(inputValues, cell) && equal(captured[cell], inputValues[cell]));
}

function validateManifest(manifest) {
  const fail = message => { throw new RefreshError(`Invalid manifest: ${message}`); };
  if (!manifest?.modelVersion) fail('modelVersion is required');
  for (const field of ['editableCells', 'controlCells', 'cases']) {
    if (!Array.isArray(manifest[field]) || !manifest[field].length) fail(`${field} must be non-empty`);
  }
  for (const field of ['editableCells', 'controlCells']) {
    if (new Set(manifest[field]).size !== manifest[field].length) fail(`${field} contains duplicates`);
  }
  if (!Object.keys(manifest.formulaCells ?? {}).length) fail('formulaCells is required');
  if (!Object.keys(manifest.outputCells ?? {}).length) fail('outputCells is required');
  const editable = new Set(manifest.editableCells);
  const controls = new Set(manifest.controlCells);
  for (const cell of controls) {
    if (!editable.has(cell)) fail(`control ${cell} is not editable`);
  }
  for (const cell of Object.keys(manifest.baseControls ?? {})) {
    if (!controls.has(cell)) fail(`baseControls contains unsupported control ${cell}`);
  }
  function validateControlValue(cell, value) {
    if (value === undefined || (value !== null && !['number', 'string', 'boolean'].includes(typeof value))) {
      fail(`control ${cell} must be a raw scalar or null`);
    }
    if (typeof value === 'number' && !Number.isFinite(value)) fail(`control ${cell} must be finite`);
  }
  for (const [cell, value] of Object.entries(manifest.baseControls ?? {})) validateControlValue(cell, value);
  const ids = new Set();
  for (const scenario of manifest.cases) {
    if (!scenario.id || ids.has(scenario.id)) fail('case IDs must be present and unique');
    ids.add(scenario.id);
    for (const cell of Object.keys(scenario.inputs ?? {})) {
      if (!controls.has(cell)) fail(`case ${scenario.id} changes unsupported control ${cell}`);
      validateControlValue(cell, scenario.inputs[cell]);
    }
    for (const [cell, transform] of Object.entries(scenario.inputTransforms ?? {})) {
      if (!controls.has(cell)) fail(`case ${scenario.id} transforms unsupported control ${cell}`);
      if (owns(scenario.inputs ?? {}, cell)) fail(`case ${scenario.id} both sets and transforms ${cell}`);
      if (!transform || !Object.keys(transform).length) fail(`case ${scenario.id} has empty transform for ${cell}`);
      for (const [key, value] of Object.entries(transform)) {
        if (!['multiply', 'add', 'min', 'max'].includes(key) || typeof value !== 'number' || !Number.isFinite(value)) {
          fail(`case ${scenario.id} has invalid transform for ${cell}`);
        }
      }
      if (transform.min !== undefined && transform.max !== undefined && transform.min > transform.max) {
        fail(`case ${scenario.id} has reversed transform bounds for ${cell}`);
      }
    }
  }
}

async function read(adapter, cells, mode = 'formula') {
  const unique = [...new Set(cells)];
  const values = await adapter.readCells(unique, {mode});
  const result = {};
  for (const cell of unique) {
    if (!owns(values ?? {}, cell) || values[cell] === undefined) {
      throw new RefreshError(`Read omitted ${cell}`, {cell, mode});
    }
    const value = values[cell];
    if (value !== null && !['string', 'number', 'boolean'].includes(typeof value)) {
      throw new RefreshError(`Read returned a non-scalar value for ${cell}`, {cell, mode});
    }
    if (typeof value === 'number' && !Number.isFinite(value)) {
      throw new RefreshError(`Read returned a non-finite value for ${cell}`, {cell, mode});
    }
    result[cell] = value;
  }
  return result;
}

function differences(actual, expected) {
  return Object.keys(expected).filter(cell => !owns(actual, cell) || !equal(actual[cell], expected[cell]));
}

async function verifyModel(adapter, manifest) {
  const expected = {...manifest.formulaCells, ...manifest.immutableCells};
  const actual = await read(adapter, Object.keys(expected));
  const changed = differences(actual, expected);
  if (changed.length) throw new RefreshError('Model formulas or version differ from the manifest', {cells: changed});
  return fingerprint(manifest.formulaCells);
}

function checkResults(values, checks, caseId) {
  for (const check of checks) {
    const actual = values[check.cell];
    const pass = typeof check.expected === 'number'
      ? typeof actual === 'number' && Math.abs(actual - check.expected) <= (check.tolerance ?? 0)
      : equal(actual, check.expected);
    if (!pass) {
      throw new RefreshError(`Case ${caseId} failed ${check.label ?? check.cell}`, {
        caseId, cell: check.cell, actual, expected: check.expected,
      });
    }
  }
}

/**
 * Required manifest fields:
 * {modelVersion, editableCells, formulaCells:{A1:formula},
 *  immutableCells?:{A1:scalar}, controlCells, baseControls?:{A1:scalar},
 *  cases:[{id,label,inputs?:{A1:scalar},inputTransforms?:{A1:{multiply?,add?,min?,max?}},
 *          checks?:[{cell,expected,tolerance?,label?}],notes?}],
 *  outputCells:{metric:A1},
 *  checks?:[{cell,expected,tolerance?,label?}], nullableOutputKeys?:[metric]}
 *
 * Every case starts from the captured LIVE control inputs, with sparse
 * baseControls overrides (normally only the scenario selector set to Base).
 * Do not place generator price/rate defaults in baseControls: doing so would
 * silently replace edited working assumptions. Relative transforms always use
 * the captured baseline, so neither defaults nor prior-case overrides leak.
 * All result metrics must be finite numbers, except explicitly nullable ratios
 * whose blank results represent an undefined denominator. Checks use raw values.
 */
export async function refreshModel({manifest, adapter, now = () => new Date().toISOString()}) {
  validateManifest(manifest);
  if (typeof adapter?.readCells !== 'function' || typeof adapter?.writeCells !== 'function') {
    throw new RefreshError('readCells and writeCells adapters are required');
  }
  const formulaFingerprint = await verifyModel(adapter, manifest);
  const originalInputValues = await read(adapter, manifest.editableCells);
  const originalControls = Object.fromEntries(manifest.controlCells.map(cell => [cell, originalInputValues[cell]]));
  const baselineControls = {...originalControls, ...manifest.baseControls};
  const controlSet = new Set(manifest.controlCells);
  let ownedControls = {...originalControls};
  let pendingControls = null;
  let failure = null;
  let restoration = null;
  const rows = [];

  async function verifyInputs(expectedControls) {
    const current = await read(adapter, manifest.editableCells);
    const changed = differences(current, {...originalInputValues, ...expectedControls});
    if (changed.length) throw new RefreshError('Concurrent input edit detected; snapshot was not published', {cells: changed});
    return current;
  }

  async function restore() {
    const current = await read(adapter, manifest.editableCells);
    const conflicts = [];
    const patch = {};
    for (const cell of manifest.editableCells) {
      if (equal(current[cell], originalInputValues[cell])) continue;
      if (!controlSet.has(cell)) {
        conflicts.push(cell);
      } else if (equal(current[cell], ownedControls[cell]) || (pendingControls && equal(current[cell], pendingControls[cell]))) {
        patch[cell] = originalControls[cell];
      } else {
        // A user owns this new value. Do not restore over it.
        conflicts.push(cell);
      }
    }
    if (Object.keys(patch).length) await adapter.writeCells(patch, {purpose: 'restore'});
    if (adapter.recalculate) await adapter.recalculate();
    const after = await read(adapter, manifest.editableCells);
    const notRestored = differences(after, originalInputValues);
    const result = {restored: notRestored.length === 0, conflicts, notRestored};
    if (notRestored.length) {
      throw new RefreshError('Controls could not all be restored without overwriting concurrent edits', result);
    }
    return result;
  }

  try {
    for (const scenario of manifest.cases) {
      await verifyInputs(ownedControls);
      const controls = {...baselineControls, ...scenario.inputs};
      for (const [cell, transform] of Object.entries(scenario.inputTransforms ?? {})) {
        const baseline = baselineControls[cell];
        if (typeof baseline !== 'number' || !Number.isFinite(baseline)) {
          throw new RefreshError(`Case ${scenario.id} cannot transform non-numeric ${cell}`);
        }
        let value = baseline * (transform.multiply ?? 1) + (transform.add ?? 0);
        if (transform.min !== undefined) value = Math.max(value, transform.min);
        if (transform.max !== undefined) value = Math.min(value, transform.max);
        if (!Number.isFinite(value)) throw new RefreshError(`Case ${scenario.id} transform overflow for ${cell}`);
        controls[cell] = value;
      }
      const patch = Object.fromEntries(Object.entries(controls).filter(([cell, value]) => !equal(ownedControls[cell], value)));
      if (Object.keys(patch).length) {
        // Keep both old and attempted values until the mutation is verified.
        // A transport error may have occurred after some/all writes applied.
        pendingControls = controls;
        await adapter.writeCells(patch, {purpose: 'case', caseId: scenario.id});
        await verifyInputs(controls);
        ownedControls = controls;
        pendingControls = null;
      }
      if (adapter.recalculate) await adapter.recalculate();
      const metrics = Object.values(manifest.outputCells);
      const checks = [...(manifest.checks ?? []), ...(scenario.checks ?? [])];
      const values = await read(adapter, [...metrics, ...checks.map(check => check.cell)], 'value');
      checkResults(values, checks, scenario.id);
      const outputs = {};
      for (const [metric, cell] of Object.entries(manifest.outputCells)) {
        if ((manifest.nullableOutputKeys ?? []).includes(metric) && (values[cell] === null || values[cell] === '')) {
          outputs[metric] = null;
          continue;
        }
        if (typeof values[cell] !== 'number' || !Number.isFinite(values[cell])) {
          throw new RefreshError(`Case ${scenario.id} returned a non-numeric ${metric}`, {caseId: scenario.id, cell, value: values[cell]});
        }
        outputs[metric] = values[cell];
      }
      await verifyInputs(ownedControls);
      rows.push({
        id: scenario.id, label: scenario.label ?? scenario.id, inputs: controls, outputs,
        verifiedChecks: checks.map(check => ({...check, actual: values[check.cell]})),
        notes: scenario.notes ?? '',
      });
    }
  } catch (error) {
    failure = error;
  } finally {
    try {
      restoration = await restore();
    } catch (error) {
      restoration = {restored: false, error: error.message, ...error.details};
      if (!failure) failure = error;
    }
  }

  if (failure) {
    throw new RefreshError(failure.message, {
      ...failure.details, restoration, originalControls, completedCases: rows.map(row => row.id),
      published: false,
    });
  }
  await verifyModel(adapter, manifest);
  await verifyInputs(originalControls);
  const snapshot = {
    modelVersion: manifest.modelVersion,
    formulaFingerprint,
    sourceInputSignature: fingerprint(originalInputValues),
    capturedAt: now(),
    originalInputValues,
    rows,
    restoration,
  };
  // Publication adapters must overwrite the whole prior comparison and capture
  // area in a single batch. They must never append partial case results.
  if (adapter.publishSnapshot) {
    try {
      await adapter.publishSnapshot(snapshot);
    } catch (error) {
      throw new RefreshError('Snapshot publication failed; read the capture metadata to resolve the final write status', {
        cause: error.message, restored: true, publicationStatus: 'unknown',
        capturedAt: snapshot.capturedAt, sourceInputSignature: snapshot.sourceInputSignature,
      });
    }
  }
  return snapshot;
}

function columnName(index) {
  let result = '';
  for (let value = index + 1; value > 0; value = Math.floor((value - 1) / 26)) {
    result = String.fromCharCode(65 + (value - 1) % 26) + result;
  }
  return result;
}

function quoteCell(address) {
  const bang = address.lastIndexOf('!');
  if (bang < 1) throw new RefreshError(`Publication address must include a sheet: ${address}`);
  let sheet = address.slice(0, bang);
  if (sheet.startsWith("'") && sheet.endsWith("'")) sheet = sheet.slice(1, -1).replaceAll("''", "'");
  return `'${sheet.replaceAll("'", "''")}'!${address.slice(bang + 1)}`;
}

/**
 * Make the ONE publication batch. Null means clear the cell; formulas are never
 * included here. Native B3/AC stale formulas remain owned by the generator.
 *
 * manifest.publication = {
 *   sheet:'Sensitivity', resultStartRow:8, resultEndRow:30,
 *   resultColumns?:[label,capturedAt,caseInputs, ... metric keys ...],
 *   captureCells:{sourceA1:destinationA1},
 *   capturedAtCell, signatureCell, formulaFingerprintCell, modelVersionCell?
 * }
 */
export function snapshotWrites(snapshot, manifest) {
  const layout = manifest.publication;
  if (!layout?.sheet || !Number.isInteger(layout.resultStartRow) || !Number.isInteger(layout.resultEndRow)) {
    throw new RefreshError('Publication layout needs sheet and integer result start/end rows');
  }
  if (snapshot.rows.length !== manifest.cases.length || !snapshot.restoration?.restored) {
    throw new RefreshError('Only a complete, restored snapshot can be published');
  }
  if (snapshot.modelVersion !== manifest.modelVersion || snapshot.formulaFingerprint !== fingerprint(manifest.formulaCells)) {
    throw new RefreshError('Snapshot does not match the expected formula manifest');
  }
  if (layout.resultEndRow - layout.resultStartRow + 1 < snapshot.rows.length) {
    throw new RefreshError('Publication result area is too small');
  }
  const columns = layout.resultColumns ?? [
    'label', 'capturedAt', 'caseInputs', 'arr36', 'revenue36', 'grossMargin36',
    'requiredGrossFunding', 'minCash24', 'endingCash36', 'accounts36', 'staff36',
    'totalSpend24', 'gbpFunding', 'gbpARR36', 'modelVersion', 'status', 'notes',
  ];
  const writes = {};
  for (let row = layout.resultStartRow; row <= layout.resultEndRow; row += 1) {
    for (let col = 0; col < columns.length; col += 1) writes[quoteCell(`${layout.sheet}!${columnName(col)}${row}`)] = null;
  }
  snapshot.rows.forEach((row, index) => {
    const fields = {
      ...row.outputs, id: row.id, label: row.label, capturedAt: snapshot.capturedAt,
      caseInputs: JSON.stringify(ordered(row.inputs)), modelVersion: snapshot.modelVersion,
      status: 'Verified at capture; see live stale flag', notes: row.notes,
    };
    columns.forEach((key, col) => {
      if (!owns(fields, key)) throw new RefreshError(`Publication column has no snapshot value: ${key}`);
      writes[quoteCell(`${layout.sheet}!${columnName(col)}${layout.resultStartRow + index}`)] = fields[key] === null ? 'n.a.' : fields[key];
    });
  });
  for (const source of manifest.editableCells) {
    if (!owns(snapshot.originalInputValues, source) || !owns(layout.captureCells ?? {}, source)) {
      throw new RefreshError(`Publication capture omits editable input ${source}`);
    }
    writes[quoteCell(layout.captureCells[source])] = snapshot.originalInputValues[source];
  }
  for (const [field, value] of [
    ['capturedAtCell', snapshot.capturedAt], ['signatureCell', snapshot.sourceInputSignature],
    ['formulaFingerprintCell', snapshot.formulaFingerprint], ['modelVersionCell', snapshot.modelVersion],
  ]) {
    if (layout[field]) writes[quoteCell(layout[field])] = value;
  }
  return writes;
}
