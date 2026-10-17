// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { openProviderCampaignSession } from './provider-campaign-session.mjs';
import { createProviderBindings } from './provider-profile.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const names = ['CODIFIX_TEST_DATABASE_URL', 'MEMORY_R3_SNAPSHOT_BIN', 'MEMORY_R3_CLI_BIN', 'MEMORY_R3_BUDGET_BIN',
  'MEMORY_R3_TEST_IMAGE', 'MEMORY_R3_RUST_IMAGE', 'MEMORY_R3_SCORE_BIN', 'MEMORY_R3_RETRIEVER_BIN'];
const available = names.every(name => process.env[name]);
if (process.env.MEMORY_R3_REQUIRE_PROVIDER_SESSION === '1' && !available) throw Error('required provider-session configuration missing');

for (const seconds of [120, 300]) test(`live ${seconds === 120 ? 'window' : 'reduced call allocation'} exhaustion is explicit negative evidence`, { skip: !available, timeout: 180000 }, async t => {
  const f = await setup(t), manifest = f.manifest;
  manifest.limits.task_seconds = seconds;
  await fs.writeFile(path.join(f.root, 'campaign.json'), JSON.stringify(manifest));
  const counter = manifest.provider_profile.responses_tokenizer;
  const provider = { bind: (mediator, taskId) => createProviderBindings({ manifest, mediator, taskId,
    responsesCounter: { id: counter.id, sha256: counter.asset.sha256, countInputTokens: () => 64 },
    embeddingCounter: { id: counter.id, sha256: counter.asset.sha256, countInputTokens: () => 64 },
    sendResponses: () => assert.fail('no new model call when a full window cannot fit'),
    sendEmbeddings: async body => JSON.stringify({ model: body.model, object: 'list',
      data: [{ object: 'embedding', index: 0, embedding: [1, 0, 0] }], usage: { prompt_tokens: 1, total_tokens: 1 } }),
  }) };
  const owner = await openProviderCampaignSession({ ...f.options, offline: false, provider });
  try {
    const prepared = await owner.prepare({ directory: await f.directoryFor('preparation'), task_id: 'preparation',
      manifest_sha256: 'a'.repeat(64), limits: manifest.preparation_budget, task_limits: manifest.preparation_budget });
    for (const [index, condition] of (seconds === 120 ? ['no-memory', 'vector'] : ['vector']).entries()) {
      const taskId = `work-${String(index).padStart(5, '0')}`;
      const limits = { requests: seconds === 120 ? 8 : 1, tokens: manifest.model.max_input_tokens + manifest.model.max_output_tokens,
        microusd: 0, task_seconds: seconds };
      const result = await owner.execute({ case_id: 'durable-importer', condition, repeat: 0 },
        { directory: await f.directoryFor(taskId), task_id: taskId, manifest_sha256: 'a'.repeat(64), limits, task_limits: limits });
      assert.equal(result.result.success, false);
      assert(result.result.sub_results.every(s => !s.success));
      assert.equal(result.provider_requests, condition === 'vector' ? 1 : 0);
      assert.equal(result.execution_microusd, 0);
      assert.equal(result.uncertain_charge, false);
      assert.equal(result.isolation.after_sha256, prepared.snapshot_sha256);
    }
  } finally { await owner.close(); }
});

async function setup(t) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-provider-session-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const root = path.join(directory, 'pack');
  const built = spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', root, '--one-family-per-track'],
    { env: { PATH: process.env.PATH, HOME: directory }, encoding: 'utf8' });
  assert.equal(built.status, 0, built.stderr);
  const manifestPath = path.join(root, 'campaign.json');
  const manifest = JSON.parse(await fs.readFile(manifestPath));
  manifest.model.provider = 'openai';
  manifest.model.resolved_id = 'independent-answer-fixture';
  manifest.model.embedding_id = 'independent-three-dimensional-fixture';
  manifest.provider_budget = { max_calls_per_task: 8 };
  manifest.preparation_budget = { requests: 100, tokens: 10000, microusd: 0, task_seconds: 600 };
  manifest.limits.requests = 1000;
  const counterSource = 'export const countInputTokens = () => 64;\n';
  const asset = { path: 'private/counter.mjs', sha256: hash(counterSource) };
  await fs.writeFile(path.join(root, asset.path), counterSource);
  manifest.assets.push({ ...asset, kind: 'configuration' });
  const tokenizer = { id: 'fixture-only-counter', asset };
  manifest.provider_profile = { version: 1, protocol: 'openai-text-v1', responses_tokenizer: tokenizer,
    embedding: { resolved_id: manifest.model.embedding_id, dimensions: 3, max_input_tokens: 8192, input_microusd_per_million: 0, tokenizer },
    configuration_evidence: asset };
  await fs.writeFile(manifestPath, JSON.stringify(manifest));
  const directoryFor = async id => {
    const dir = path.join(root, 'run', id, 'provider'); await fs.mkdir(dir, { recursive: true, mode: 0o700 }); return dir;
  };
  return { root, manifest, directoryFor, options: {
    offline: true, split: 'development', databaseUrl: process.env.CODIFIX_TEST_DATABASE_URL,
    snapshotBinary: process.env.MEMORY_R3_SNAPSHOT_BIN ?? '/unused/snapshot', memoryBinary: process.env.MEMORY_R3_CLI_BIN ?? '/unused/memory',
    configuration: { root, manifestSha256: 'a'.repeat(64), budgetBinary: process.env.MEMORY_R3_BUDGET_BIN ?? '/unused/budget',
      scoreBinary: process.env.MEMORY_R3_SCORE_BIN, retrieverBinary: process.env.MEMORY_R3_RETRIEVER_BIN,
      nodeImage: process.env.MEMORY_R3_TEST_IMAGE, rustImage: process.env.MEMORY_R3_RUST_IMAGE,
      driverSha256: asset.sha256, configurationSha256: asset.sha256, serviceBuild: 'disposable-provider-session-test' } } };
}

test('changed preparation assets poison the session before any provider or database activity', async t => {
  const f = await setup(t);
  let binds = 0;
  const owner = await openProviderCampaignSession({ ...f.options, provider: { bind() { binds++; assert.fail('must not bind'); } } });
  const contract = { directory: await f.directoryFor('preparation'), task_id: 'preparation', manifest_sha256: 'a'.repeat(64),
    limits: f.manifest.preparation_budget, task_limits: f.manifest.preparation_budget };
  await assert.rejects(owner.execute({}, {}), /not ready/);
  await fs.appendFile(path.join(f.root, f.manifest.cases[0].sources[0]), '\nchanged');
  await assert.rejects(owner.prepare(contract), /changed campaign asset/);
  await assert.rejects(owner.prepare(contract), /cannot be repeated/);
  await assert.rejects(owner.execute({}, {}), /not ready/);
  assert.equal(binds, 0);
  await owner.close();
});

test('invalid provider bindings fail preparation without creating a snapshot or invoking a model',
  { skip: !process.env.MEMORY_R3_BUDGET_BIN }, async t => {
    const f = await setup(t);
    let binds = 0;
    const owner = await openProviderCampaignSession({ ...f.options,
      // These deliberately nonexistent executables would fail differently if
      // invalid bindings reached snapshot creation or the service boundary.
      snapshotBinary: '/unused/r3-invalid-snapshot', memoryBinary: '/unused/r3-invalid-memory',
      provider: { bind() {
        binds++;
        return { prepareRequest() { assert.fail('must not answer'); }, embed() { assert.fail('must not embed'); },
          embeddingProfile: { provider: 'openai', model: f.manifest.model.embedding_id, dimensions: 16,
            normalization: 'float32-rounding-v1', document_purpose: 'document', query_purpose: 'query', metric: 'cosine' } };
      } } });
    try {
      const contract = { directory: await f.directoryFor('preparation'), task_id: 'preparation', manifest_sha256: 'a'.repeat(64),
        limits: f.manifest.preparation_budget, task_limits: f.manifest.preparation_budget };
      await assert.rejects(owner.prepare(contract), /locked embedding profile/);
      await assert.rejects(owner.prepare(contract), /cannot be repeated/);
      await assert.rejects(owner.execute({}, {}), /not ready/);
      assert.equal(binds, 1);
    } finally { await owner.close(); }
  });

test('provider session executes both tracks with an independent model and non-synthetic vector size', { skip: !available, timeout: 180000 }, async t => {
  const f = await setup(t), { manifest } = f;
  let answerCalls = 0, embeddingCalls = 0;
  const counter = expected => ({ id: expected.id, sha256: expected.asset.sha256, countInputTokens: () => 64 });
  const provider = { bind: (mediator, taskId) => createProviderBindings({ manifest, mediator, taskId,
    responsesCounter: counter(manifest.provider_profile.responses_tokenizer), embeddingCounter: counter(manifest.provider_profile.embedding.tokenizer),
    sendEmbeddings: async body => {
      embeddingCalls++; assert.equal(body.model, manifest.model.embedding_id); assert.equal(body.dimensions, 3);
      return JSON.stringify({ object: 'list', model: body.model,
        data: [{ object: 'embedding', index: 0, embedding: [0.25, 0.5, 1] }], usage: { prompt_tokens: 1, total_tokens: 1 } });
    },
    sendResponses: async body => {
      answerCalls++; assert.equal(body.model, manifest.model.resolved_id);
      const input = JSON.parse(body.input[0].content[0].text);
      const source = input.context?.[0];
      const output = body.text.format.name === 'r3_isolated_action_v1' ? { action: { kind: 'finish' } }
        : source ? { text: source.text, refused: false, citations: [{ source_id: source.id, quote: source.text }] }
          : { text: 'Insufficient evidence.', refused: true, citations: [] };
      return JSON.stringify({ id: `resp_fixture_${answerCalls}`, object: 'response', status: 'completed', model: body.model,
        error: null, incomplete_details: null, output: [{ type: 'message', role: 'assistant', status: 'completed',
          content: [{ type: 'output_text', annotations: [], text: JSON.stringify(output) }] }],
        usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2,
          input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } });
    } }) };
  const owner = await openProviderCampaignSession({ ...f.options, provider });
  try {
    const prep = { directory: await f.directoryFor('preparation'), task_id: 'preparation', manifest_sha256: 'a'.repeat(64),
      limits: manifest.preparation_budget, task_limits: manifest.preparation_budget };
    const preparation = owner.prepare(prep);
    await assert.rejects(owner.close(), /cancel and await/);
    await assert.rejects(owner.execute({}, {}), /not ready/);
    const prepared = await preparation;
    assert.equal(prepared.audit.ledger.requests, embeddingCalls);
    assert.equal(answerCalls, 0);
    await assert.rejects(owner.prepare(prep), /cannot be repeated/);
    const initialEmbeddings = embeddingCalls;
    let index = 0;
    for (const condition of ['handover', 'lexical', 'vector', 'full-memory', 'no-memory', 'full-memory']) {
      const coding = index === 5, taskId = `work-${String(index++).padStart(5, '0')}`;
      const limits = { requests: 8, tokens: manifest.model.max_input_tokens + manifest.model.max_output_tokens, microusd: 0, task_seconds: manifest.limits.task_seconds };
      const execution = owner.execute({ case_id: coding ? 'durable-importer' : 'supplier-handover-q1', condition, repeat: 0 },
        { task_id: taskId, directory: await f.directoryFor(taskId), manifest_sha256: 'a'.repeat(64), limits, task_limits: limits });
      await assert.rejects(owner.close(), /cancel and await/);
      await assert.rejects(owner.execute({}, {}), /not ready/);
      const result = await execution;
      assert.equal(result.isolation.before_sha256, prepared.snapshot_sha256);
      assert.equal(result.isolation.after_sha256, prepared.snapshot_sha256);
      assert.equal(result.model.resolved_id, manifest.model.resolved_id);
      assert.equal(result.provider_requests, (coding ? 3 : 1) + (['vector', 'full-memory'].includes(condition) ? 1 : 0));
      if (coding) assert(!result.result.success && result.result.sub_results.length === 3);
      else assert(result.unresolved_rubric);
    }
    assert.equal(answerCalls, 8);
    assert.equal(embeddingCalls - initialEmbeddings, 3);
    await fs.appendFile(path.join(f.root, manifest.cases[0].sources[0]), '\nchanged');
    await assert.rejects(owner.execute({ case_id: 'supplier-handover-q1', condition: 'handover', repeat: 1 }, {}), /changed rehearsal input/);
    await assert.rejects(owner.execute({}, {}), /not ready/);
    assert.equal(answerCalls, 8);
  } finally { await owner.close(); }
});
