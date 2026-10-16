// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted fake adapters check contracts; they are not native isolation evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { PassThrough, Readable } from 'node:stream';
import { createKnowledgeRunner, createNativeKnowledgeScorer } from './knowledge.mjs';
import { nativeProfileHash } from './native-windows.mjs';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

function fixture() {
  const node = { path: 'C:\\private\\tool\\node.exe', sha256: 'a'.repeat(64) };
  const guest = { path: 'C:\\private\\tool\\guest.mjs', sha256: 'b'.repeat(64) };
  const scorer = { path: 'C:\\private\\tool\\r3_score.exe', sha256: hash('reviewed scorer') };
  const profile = { schema_version: 1, helper: { path: 'C:\\private\\helper.exe', sha256: 'c'.repeat(64) },
    io_root: 'C:\\private\\runs', node, guest, toolchains: [{ root: 'C:\\private\\tool', files: [node, guest, scorer] }],
    search_path: ['C:\\private\\tool'], library_path: [] };
  const sources = [{ id: 'public/source.md', text: 'History is retained.' },
    { id: 'public/unselected.md', text: 'OTHER_SOURCE_PRIVATE' }];
  return { execution: { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) },
    scoreExecutable: scorer, argv: [node.path, '-e', 'reviewed deterministic answer command'], sources,
    context: [sources[0]], question: 'What is retained?', manifestSha256: 'd'.repeat(64), timeoutMs: 1000,
    work: { case_id: 'knowledge-case', condition: 'lexical', repeat: 0 },
    rubric: { case_id: 'knowledge-case', question: 'What is retained?',
      criteria: ['factual_correctness', 'required_evidence', 'appropriate_refusal'].map((dimension, i) => ({
        id: `criterion-${i}`, dimension, instruction: 'PRIVATE_RUBRIC_ONLY', critical: false })),
      minimum_citations: 1, citations_critical: true } };
}
const answer = { text: 'History is retained.', refused: false,
  citations: [{ source_id: 'public/source.md', quote: 'History is retained.' }] };
function pending(input) {
  return { assessment: { packet_sha256: 'e'.repeat(64), success: false, unresolved: true, citations_valid: true, critical_failure: false, criteria: {} },
    evidence: { packet: { manifest_sha256: input.manifest_sha256, rubric: input.rubric, sources: input.sources, answer: input.answer }, ballots: [], history: [] } };
}
function adapters({ corruptReceipt = false, resultFiles, scoreChange } = {}) {
  const calls = [], scores = [];
  const run = createKnowledgeRunner({ runExecution: async input => {
    calls.push(structuredClone(input));
    assert.deepEqual(Object.keys(input.files), ['input.json']);
    const publicInput = JSON.parse(input.files['input.json']);
    assert.deepEqual(Object.keys(publicInput).sort(), ['context', 'question']);
    assert(!input.files['input.json'].includes('PRIVATE_RUBRIC_ONLY'));
    assert(!input.files['input.json'].includes('OTHER_SOURCE_PRIVATE'));
    return { version: 1, exitCode: 0, stdout: '', stderr: '', elapsedMs: 1,
      files: resultFiles ?? { 'answer.json': Buffer.from(JSON.stringify(answer)).toString('base64') },
      execution: { kind: 'windows-native-v1', profile_sha256: corruptReceipt ? '0'.repeat(64) : input.execution.profile_sha256,
        descriptor_sha256: '1'.repeat(64), receipt_sha256: '2'.repeat(64), disk_hard_limit_enforced: false } };
  }, scoreNative: async (scorer, input) => {
    assert.equal(calls.length, 1, 'private scorer must follow validated answer execution');
    scores.push(structuredClone({ scorer, input }));
    const value = pending(input); scoreChange?.(value); return value;
  } });
  return { run, calls, scores };
}

test('native knowledge discloses only selected context and retains pending host evidence', async () => {
  const value = fixture(), runner = adapters();
  const result = await runner.run(value);
  assert.equal(result.kind, 'native-windows-knowledge-smoke');
  assert.equal(result.officialEvidence, false);
  assert.equal(result.execution.profile_sha256, value.execution.profile_sha256);
  assert.deepEqual(result.scorer, value.scoreExecutable);
  assert.equal(result.image, undefined);
  assert.deepEqual([result.providerRequests, result.newProviderRequests, result.executionMicrousd], [0, 0, 0]);
  assert.equal(result.assessment.unresolved, true); assert.equal(result.assessment.success, false);
  assert.deepEqual(runner.scores[0].input.sources, value.sources);
  assert.deepEqual(runner.scores[0].input.rubric, value.rubric);
  assert.deepEqual(result.evidence.ballots, []);
});

test('native knowledge rejects unpinned scorers, mixed backends and model callbacks before execution', async () => {
  for (const change of [
    value => { value.scoreExecutable.sha256 = '0'.repeat(64); },
    value => { value.scoreExecutable.path = 'C:\\private\\outside\\r3_score.exe'; },
    value => { value.scoreExecutable.path = 'C:\\private\\tool\\..\\tool\\r3_score.exe'; },
    value => { value.scoreExecutable.path += ':stream'; },
    value => { value.runAnswer = async () => { throw Error('must not run'); }; },
    value => { value.image = `sha256:${'a'.repeat(64)}`; },
    value => { value.scoreBinary = '/linux/scorer'; },
    value => { value.work.condition = 'no-memory'; },
  ]) {
    const value = fixture(), runner = adapters(); change(value);
    await assert.rejects(runner.run(value));
    assert.equal(runner.calls.length, 0); assert.equal(runner.scores.length, 0);
  }
});

test('bad native answer receipts and malformed exports prevent private adjudication', async () => {
  for (const options of [{ corruptReceipt: true }, { resultFiles: {} },
    { resultFiles: { 'answer.json': Buffer.from('bad JSON').toString('base64') } },
    { resultFiles: { 'answer.json': Buffer.from(JSON.stringify({ text: 'missing fields' })).toString('base64') } }]) {
    const runner = adapters(options);
    await assert.rejects(runner.run(fixture()));
    assert.equal(runner.scores.length, 0);
  }
  for (const scoreChange of [value => { value.assessment.success = true; },
    value => { value.evidence.ballots = [{}]; }, value => { value.evidence.packet.answer.text = 'altered'; }]) {
    await assert.rejects(adapters({ scoreChange }).run(fixture()), /assessment binding/);
  }
});

function scorerProcess({ output, failure, corruptBytes = false } = {}) {
  const state = { closedHandles: 0, launched: 0, killed: 0, reaped: 0 };
  const binary = Buffer.from(corruptBytes ? 'changed' : 'reviewed scorer');
  let started;
  state.started = new Promise(resolve => { started = resolve; });
  const score = createNativeKnowledgeScorer({ platform: 'win32', systemRoot: 'C:\\Windows', fileSystem: {
    open: async () => ({ stat: async () => ({ isFile: () => true, size: binary.length }),
      createReadStream: () => Readable.from([binary]), close: async () => { state.closedHandles++; } }),
  }, spawnProcess: (exe, argv, options) => {
    state.launched++;
    assert.equal(exe, fixture().scoreExecutable.path); assert.deepEqual(argv, ['assess-stdin']);
    assert.equal(options.shell, false); assert.equal(options.windowsHide, true);
    assert.deepEqual(options.env, { SystemRoot: 'C:\\Windows', WINDIR: 'C:\\Windows', PATH: '' });
    assert.equal(options.cwd, 'C:\\private\\tool');
    const child = new EventEmitter(); child.stdin = new PassThrough(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
    const close = code => { state.reaped++; child.emit('close', code, null); };
    child.kill = signal => { assert.equal(signal, 'SIGKILL'); state.killed++; queueMicrotask(() => close(1)); return true; };
    child.stdin.on('finish', () => { started(); queueMicrotask(() => {
      if (failure === 'hang') return;
      if (failure === 'pipe') { child.stdin.emit('error', Error('EPIPE private input')); return; }
      if (failure === 'spawn') { child.emit('error', Error('private spawn detail')); return; }
      if (failure === 'overflow') { child.stdout.write(Buffer.alloc(2 * 1024 * 1024)); child.stderr.write(Buffer.alloc(2 * 1024 * 1024 + 1)); return; }
      child.stdout.write(output ?? '{}'); child.stderr.write('discarded private diagnostic'); close(0);
    }); });
    return child;
  } });
  return { score, state };
}

test('native trusted scorer pins actual bytes and uses only bounded private pipes', async () => {
  const process = scorerProcess({ output: '{"assessment":{},"evidence":{}}' });
  assert.deepEqual(await process.score(fixture().scoreExecutable, { private: 'rubric' }), { assessment: {}, evidence: {} });
  assert.equal(process.state.closedHandles, 1); assert.equal(process.state.reaped, 1);
  const changed = scorerProcess({ corruptBytes: true });
  await assert.rejects(changed.score(fixture().scoreExecutable, {}), /bytes changed/);
  assert.equal(changed.state.launched, 0); assert.equal(changed.state.closedHandles, 1);
  const oversized = scorerProcess();
  await assert.rejects(oversized.score(fixture().scoreExecutable, { private: 'x'.repeat(2 * 1024 * 1024) }), /invalid native scoring input/);
  assert.equal(oversized.state.launched, 0);
});

test('native trusted scorer sanitizes errors and reaps on EPIPE, spawn error and combined output overflow', async () => {
  for (const failure of ['pipe', 'spawn', 'overflow']) {
    const process = scorerProcess({ failure });
    await assert.rejects(process.score(fixture().scoreExecutable, {}), error => error.message === 'native knowledge adjudication process failed');
    assert.equal(process.state.killed, 1); assert.equal(process.state.reaped, 1);
  }
  await assert.rejects(scorerProcess({ output: 'malformed private output' }).score(fixture().scoreExecutable, {}), /invalid native knowledge assessment/);
});

test('native trusted scorer enforces a deadline and never falls back on another host', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const process = scorerProcess({ failure: 'hang' });
  const result = process.score(fixture().scoreExecutable, {});
  await process.state.started;
  t.mock.timers.tick(10000);
  await assert.rejects(result, /adjudication process failed/);
  assert.equal(process.state.killed, 1); assert.equal(process.state.reaped, 1);
  await assert.rejects(createNativeKnowledgeScorer({ platform: 'linux' })(fixture().scoreExecutable, {}), /no Linux fallback/);
});
