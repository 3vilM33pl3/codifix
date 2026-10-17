// SPDX-License-Identifier: AGPL-3.0-or-later
// Fake trusted adapters test dispatch only, never native OS isolation evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createRehearsalExecutorFactory, createProviderExecutor } from './rehearsal-task.mjs';
import { nativeProfileHash } from './native-windows.mjs';
import { syntheticEmbeddingModel } from '../synthetic-embedding.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const source = 'Documented history is retained.';
const originalCode = 'fn main() { println!("original"); }';
const sourceId = 'public/source.md';
const conditions = ['handover', 'lexical', 'vector', 'full-memory', 'no-memory'];

async function fixture(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-native-dispatch-'));
  t.after(() => fs.rm(root, { recursive: true }));
  const file = name => ({ path: `C:\\private\\tools\\${name}`, sha256: hash(name) });
  const node = file('node.exe'), guest = file('guest.mjs'), compiler = file('rustc.exe'), nativeScorer = file('r3_score.exe');
  const nodePath = node.path;
  const profile = { schema_version: 1, helper: file('helper.exe'), io_root: 'C:\\private\\runs', node, guest,
    toolchains: [{ root: 'C:\\private\\tools', files: [node, guest, compiler, nativeScorer] }],
    search_path: ['C:\\private\\tools'], library_path: [] };
  const execution = { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) };
  const assets = {
    [sourceId]: source, 'public/unselected.md': 'UNSELECTED_SOURCE_ONLY',
    'private/families.json': JSON.stringify([{ id: 'knowledge', track: 'knowledge' },
      { id: 'coding', track: 'coding', fixture: 'fixtures/coding', stages: 'private/stages.json', submission_paths: ['src/main.rs'] }]),
    'private/rubrics/question.json': JSON.stringify({ case_id: 'question', question: 'What history is retained?',
      criteria: [{ instruction: 'PRIVATE_RUBRIC_ONLY' }] }),
    'private/stages.json': JSON.stringify([1,2,3].map(index => ({ id: `step-${index}`, prompt: `Public step ${index}`,
      probes: [{ id: `probe-${index}`, argv: [], stdin: '', expectedStdout: `PRIVATE_EXPECTED_${index}` }] }))),
    'fixtures/coding/src/main.rs': originalCode,
  };
  for (const [name, bytes] of Object.entries(assets)) {
    await fs.mkdir(path.dirname(path.join(root, name)), { recursive: true });
    await fs.writeFile(path.join(root, name), bytes);
  }
  const manifest = { smoke_only: true, label_status: 'draft', limits: { microusd: 0, task_seconds: 60 },
    semantic_time: '2026-10-17T00:00:00Z', model: { provider: 'synthetic', resolved_id: 'fake', reasoning: 'none', embedding_id: syntheticEmbeddingModel },
    assets: Object.entries(assets).map(([name, bytes]) => ({ path: name, sha256: hash(bytes) })),
    cases: [{ id: 'question', cluster: 'knowledge', track: 'knowledge', sources: [sourceId, 'public/unselected.md'] },
      { id: 'sequence', cluster: 'coding', track: 'coding', sources: [sourceId] }] };
  const saveManifest = () => fs.writeFile(path.join(root, 'campaign.json'), JSON.stringify(manifest));
  await saveManifest();
  const calls = [], retrievals = [], queries = [], sessions = [], scores = [];
  const snapshot = hash('prepared snapshot');
  const options = { configuration: { root, execution, nativeScorer, compiler, retrieverBinary: path.join(root, 'retriever-host'),
    manifestSha256: hash('manifest'), driverSha256: hash('driver'), configurationSha256: hash('configuration'), serviceBuild: 'fake-native-dispatch-test' },
  session: { preparation: { official_evidence: false }, withInstance: async (project, invoke, { queryEmbedding }) => {
    sessions.push(project);
    const binding = { project, source: sourceId, source_sha256: hash(source), memory_id: 'memory-1', canonical_id: 'canonical-1', version_no: 1 };
    const value = await invoke({ bindings: [binding], query: async question => {
      queries.push({ project, question });
      await queryEmbedding(question);
      return { results: [{ project, memory_id: binding.memory_id, canonical_id: binding.canonical_id, version_no: binding.version_no }] };
    } });
    return { value, beforeSha256: snapshot, afterSha256: snapshot };
  } } };
  let changedIdentity = false;
  const build = createRehearsalExecutorFactory({
    retrieve: async (binary, config, request) => {
      retrievals.push({ binary, configuration: JSON.parse(await fs.readFile(config, 'utf8')), request });
      return { results: [{ id: sourceId, text: source, citations: [sourceId] }] };
    },
    runExecution: async input => {
      calls.push(structuredClone(input));
      assert.equal(input.image, undefined);
      assert.equal(input.argv[0], nodePath);
      assert(!JSON.stringify(input).includes('/usr/local/'));
      assert(!JSON.stringify(input).includes('PRIVATE_RUBRIC_ONLY'));
      assert(!JSON.stringify(input).includes('UNSELECTED_SOURCE_ONLY'));
      assert(!JSON.stringify(input).includes('PRIVATE_EXPECTED_'));
      let stdout = '', exports = {};
      if (input.files['input.json']) {
        assert.deepEqual(Object.keys(input.files), ['input.json']);
        const selected = JSON.parse(input.files['input.json']).context[0];
        const answer = selected ? { text: selected.text, refused: false, citations: [{ source_id: selected.id, quote: selected.text }] }
          : { text: 'Insufficient supplied evidence.', refused: true, citations: [] };
        exports['answer.json'] = Buffer.from(JSON.stringify(answer)).toString('base64');
      } else if (input.files['task.json']) {
        const index = Number(JSON.parse(input.files['task.json']).prompt.split(' ').at(-1));
        assert.equal(input.files['src/main.rs'], originalCode + [1,2,3].slice(0,index - 1).map(n => `\n// export ${n}`).join(''));
        // Deliberately distinct fake exports prove each later call receives the
        // previous allowlisted bytes, without executing an actual agent here.
        exports['src/main.rs'] = Buffer.from(input.files['src/main.rs'] + `\n// export ${index}`).toString('base64');
      } else {
        assert(input.files['harness/run.cjs'].includes(JSON.stringify(compiler.path)));
        stdout = input.files['submission/src/main.rs'].includes('r3-toolchain-ready') ? 'r3-toolchain-ready\n' : 'original\n';
      }
      return { version: 1, exitCode: 0, stdout, stderr: '', elapsedMs: 1, files: exports,
        execution: { kind: execution.kind, profile_sha256: changedIdentity ? hash('wrong') : execution.profile_sha256,
          descriptor_sha256: hash(JSON.stringify(input)), receipt_sha256: hash(String(calls.length)), disk_hard_limit_enforced: false } };
    },
    scoreNative: async (scorer, input) => {
      scores.push({ scorer, input });
      return { assessment: { success: false, unresolved: true, citations_valid: true, critical_failure: false, packet_sha256: hash('pending') },
        evidence: { packet: { manifest_sha256: input.manifest_sha256, rubric: input.rubric, sources: input.sources, answer: input.answer }, ballots: [], history: [] } };
    },
  });
  return { options, manifest, saveManifest, build, calls, scores, queries, sessions, retrievals,
    changeIdentity: () => { changedIdentity = true; } };
}

function zeroUsage(observation) {
  assert.equal(observation.provider_requests, 0);
  assert.equal(observation.execution_microusd, 0);
  assert.deepEqual(observation.result.token_usage, { input_tokens: 0, output_tokens: 0, total_tokens: 0 });
  assert.equal(observation.result.metadata.claim, 'offline-synthetic-not-quality');
}

test('native rehearsal selects five contexts and preserves answer receipts with zero spend', async t => {
  const f = await fixture(t), run = await f.build(f.options);
  // Once constructed, caller mutation cannot change the reviewed native argv.
  f.options.configuration.execution.profile.node.path = 'C:\\wrong\\node.exe';
  for (const condition of conditions) {
    const observation = await run({ case_id: 'question', condition, repeat: 0 });
    zeroUsage(observation);
    assert.equal(observation.unresolved_rubric, true);
    assert.equal(observation.result.success, false);
    const identity = JSON.parse(observation.result.metadata.native_execution);
    assert.equal(identity.answer.receipt_sha256, hash(String(f.calls.length)));
    assert.equal(identity.answer.disk_hard_limit_enforced, false);
    assert.deepEqual(identity.scorer, f.options.configuration.nativeScorer);
    assert.equal(observation.isolation.before_sha256, observation.isolation.after_sha256);
    const context = JSON.parse(f.calls.at(-1).files['input.json']).context;
    assert.deepEqual(context, condition === 'no-memory' ? [] : [{ id: sourceId, text: source }]);
  }
  assert.deepEqual(f.retrievals.map(call => call.configuration.mode), ['handover', 'lexical', 'vector']);
  assert.deepEqual(f.retrievals.map(call => !!call.request.query_embedding), [false, false, true]);
  assert.equal(f.queries.length, 1);
  assert.equal(f.sessions.length, 5);
  assert.equal(f.scores.length, 5);
  assert.equal(f.scores[0].input.rubric.criteria[0].instruction, 'PRIVATE_RUBRIC_ONLY');
});

test('native rehearsal routes three Rust stages and retains every validated receipt', async t => {
  const f = await fixture(t), run = await f.build(f.options);
  const observation = await run({ case_id: 'sequence', condition: 'full-memory', repeat: 0 });
  zeroUsage(observation);
  assert.equal(observation.result.success, false);
  assert.equal(observation.result.sub_results.length, 3);
  assert.equal(f.calls.filter(call => call.files['task.json']).length, 3);
  assert.equal(f.calls.length, 12); // Three agents + readiness and cumulative 1/2/3 probes.
  const task = JSON.parse(observation.result.metadata.native_execution);
  assert.equal(task.profile_sha256, f.options.configuration.execution.profile_sha256);
  assert.deepEqual(task.compiler, f.options.configuration.compiler);
  for (const [index, step] of observation.result.sub_results.entries()) {
    assert.equal(step.success, false);
    assert.deepEqual(step.token_usage, { input_tokens: 0, output_tokens: 0, total_tokens: 0 });
    const receipts = JSON.parse(step.metadata.native_execution);
    assert.equal(receipts.agent.profile_sha256, task.profile_sha256);
    assert.equal(receipts.probes.length, index + 1);
    assert(receipts.readiness.receipt_sha256);
    assert(receipts.probes.every(probe => probe.execution.receipt_sha256));
  }
  assert(!JSON.stringify(observation).includes('PRIVATE_EXPECTED_'));
});

test('native rehearsal rejects mixed profiles, mediated wiring and non-smoke work before dispatch', async t => {
  const f = await fixture(t);
  for (const change of [
    config => { config.execution.profile_sha256 = hash('wrong'); },
    config => { config.execution.extra = true; },
    config => { config.execution.profile.node.extra = true; },
    config => { config.nodeImage = `sha256:${hash('docker')}`; },
    config => { config.rustImage = `sha256:${hash('docker')}`; },
    config => { config.scoreBinary = '/linux/scorer'; },
    config => { config.nativeScorer.sha256 = hash('wrong'); },
    config => { delete config.nativeScorer; },
    config => { config.compiler.sha256 = hash('wrong'); },
    config => { delete config.compiler; },
    config => { config.budgetBinary = '/linux/budget'; },
  ]) {
    const options = { ...f.options, configuration: structuredClone(f.options.configuration) };
    change(options.configuration);
    await assert.rejects(f.build(options));
  }
  await assert.rejects(f.build({ ...f.options, wireProvider: { bind() { throw Error('must not bind'); } } }), /deterministic commands/);
  for (const offline of [true, false]) await assert.rejects(createProviderExecutor({ ...f.options, offline, split: 'development' }), /native provider campaign/);
  for (const change of [
    manifest => { manifest.smoke_only = false; }, manifest => { manifest.label_status = 'approved'; },
    manifest => { manifest.limits.microusd = 1; }, manifest => { manifest.provider_budget = {}; },
  ]) {
    const prior = structuredClone(f.manifest); change(f.manifest); await f.saveManifest();
    await assert.rejects(f.build(f.options));
    Object.assign(f.manifest, prior); delete f.manifest.provider_budget;
  }
  assert.equal(f.calls.length, 0); assert.equal(f.sessions.length, 0); assert.equal(f.scores.length, 0);
});

test('changed native receipts fail before private scoring or an observation is returned', async t => {
  const f = await fixture(t), run = await f.build(f.options);
  f.changeIdentity();
  await assert.rejects(run({ case_id: 'question', condition: 'no-memory', repeat: 0 }), /native execution identity/);
  assert.equal(f.scores.length, 0);
});

test('Linux rehearsal keeps explicit image commands and existing result metadata', async t => {
  const f = await fixture(t), calls = [];
  const configuration = { ...f.options.configuration, nodeImage: `sha256:${hash('node image')}`,
    rustImage: `sha256:${hash('rust image')}`, scoreBinary: path.join(f.options.configuration.root, 'host-score') };
  delete configuration.execution; delete configuration.nativeScorer; delete configuration.compiler;
  const build = createRehearsalExecutorFactory({ runExecution: async input => {
    calls.push(input);
    assert.equal(input.execution, undefined);
    assert.equal(input.image, configuration.rustImage);
    assert.equal(input.argv[0], '/usr/local/bin/node');
    const ready = input.files['submission/src/main.rs']?.includes('r3-toolchain-ready');
    return { version: 1, image: input.image, exitCode: 0, stdout: ready ? 'r3-toolchain-ready\n' : 'original\n',
      stderr: '', elapsedMs: 1, files: Object.fromEntries(input.exportPaths.map(name => [name, Buffer.from(input.files[name]).toString('base64')])) };
  } });
  const run = await build({ ...f.options, configuration });
  const observation = await run({ case_id: 'sequence', condition: 'full-memory', repeat: 0 });
  assert.equal(calls.length, 12);
  zeroUsage(observation);
  assert.equal(observation.result.success, false);
  assert.equal(observation.result.metadata.native_execution, undefined);
  assert(observation.result.sub_results.every(step => step.metadata.native_execution === undefined));
});
