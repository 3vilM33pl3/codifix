// SPDX-License-Identifier: AGPL-3.0-or-later
// Synthetic temporary approvals and key only. fetch is mocked before any owner
// is opened; this suite cannot be evidence of real approval or provider quality.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { withApprovedProvider } from './approved-provider.mjs';
import { openRequestMediator } from './request-mediator.mjs';
import { dispatchModel } from './isolation/model-dispatch.mjs';
import { taskTokenLimit } from './provider-profile.mjs';
import { withApprovedCampaignSession } from './approved-campaign-session.mjs';

const memory = process.env.MEMORY_R3_CLI_BIN, budget = process.env.MEMORY_R3_BUDGET_BIN;
const available = !!memory && !!budget;
if (process.env.MEMORY_R3_REQUIRE_PROVIDER_OWNER === '1' && !available) throw Error('required provider owner binaries missing');
const hash = value => createHash('sha256').update(value).digest('hex');
async function setup(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-approved-provider-'));
  t.after(() => fs.rm(root, { recursive: true }));
  const pack = path.join(root, 'pack'), manifestPath = path.join(pack, 'campaign.json');
  const env = { PATH: '/usr/bin:/bin', MEMORY_LAYER_PROFILE: 'prod' };
  const built = spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', pack, '--one-family-per-track'], { env, encoding: 'utf8' });
  assert.equal(built.status, 0, built.stderr);
  const manifest = JSON.parse(await fs.readFile(manifestPath));
  manifest.label_status = 'reviewed'; // Synthetic fixture approval, never a source-suite edit.
  manifest.model = { provider: 'openai', resolved_id: 'gpt-5.4-mini-2026-03-17', reasoning: 'medium', embedding_id: 'text-embedding-3-small',
    max_input_tokens: 400000, max_output_tokens: 4096, input_microusd_per_million: 750000,
    output_microusd_per_million: 4500000, cached_input_microusd_per_million: 75000 };
  manifest.provider_budget = { max_calls_per_task: 3 };
  manifest.limits = { requests: 500, tokens: 10000000, microusd: 10000000, task_seconds: 300 };
  manifest.preparation_budget = { requests: 100, tokens: 819200, microusd: 20000, task_seconds: 300 };
  const source = await fs.readFile(new URL('./context-reservation.mjs', import.meta.url));
  const pin = { path: 'private/context-reservation.mjs', sha256: hash(source) };
  await fs.writeFile(path.join(pack, pin.path), source);
  manifest.assets.push({ ...pin, kind: 'configuration' });
  const ownerSource = await fs.readFile(new URL('./approved-campaign-session.mjs', import.meta.url));
  await fs.writeFile(path.join(pack, 'private/approved-campaign-session.mjs'), ownerSource);
  manifest.assets.push({ path: 'private/approved-campaign-session.mjs', sha256: hash(ownerSource), kind: 'configuration' });
  manifest.provider_profile = { version: 2, protocol: 'openai-text-v1', input_accounting: 'full_context_reservation',
    responses_tokenizer: { id: 'full-context-responses-v1', asset: pin },
    embedding: { resolved_id: 'text-embedding-3-small', dimensions: 1536, max_input_tokens: 8192,
      input_microusd_per_million: 20000, tokenizer: { id: 'full-context-embeddings-v1', asset: pin } }, configuration_evidence: pin };
  await fs.writeFile(manifestPath, JSON.stringify(manifest));
  const planned = spawnSync(memory, ['eval', 'campaign', 'plan', '--manifest', manifestPath], { env, encoding: 'utf8' });
  assert.equal(planned.status, 0, planned.stderr);
  const { manifest_sha256: manifestSha256 } = JSON.parse(planned.stdout);
  const approvals = [];
  for (const purpose of ['labels', 'development']) {
    const file = path.join(root, purpose + '.json');
    await fs.writeFile(file, JSON.stringify({ purpose, manifest_sha256: manifestSha256,
      reviewer: 'synthetic-test-only-no-human-authority', approved_at: '2026-01-01T00:00:00Z', maximum_microusd: 10000000 }));
    approvals.push(file);
  }
  const baoBinary = path.join(root, 'fake-bao'), secretRead = path.join(root, 'secret-read');
  await fs.writeFile(baoBinary, `#!${process.execPath}\nrequire('node:fs').writeFileSync(${JSON.stringify(secretRead)}, 'read');process.stdout.write('synthetic-owner-key');`, { mode: 0o700 });
  const options = { manifest: manifestPath, memoryBinary: memory, baoBinary, manifestSha256, split: 'development', approvals,
    secret: { address: 'https://unreachable.example.test', mount: 'secret', path: 'synthetic/r3', field: 'OPENAI_API_KEY', version: 1 } };
  return { root, pack, manifest, options, secretRead };
}

function wire(t) {
  const calls = [];
  const localFetch = globalThis.fetch.bind(globalThis);
  t.mock.method(globalThis, 'fetch', async (url, options) => {
    if (!String(url).startsWith('https://api.openai.com/')) {
      assert.match(String(url), /^http:\/\/127\.0\.0\.1:[0-9]+\//, 'only disposable loopback service traffic may use real fetch');
      return localFetch(url, options);
    }
    assert.equal(options.headers.Authorization, 'Bearer synthetic-owner-key');
    assert.equal(options.redirect, 'error');
    assert(options.signal instanceof AbortSignal);
    const body = JSON.parse(options.body); calls.push(url);
    const response = url === 'https://api.openai.com/v1/embeddings'
      ? { model: body.model, object: 'list', data: [{ object: 'embedding', index: 0, embedding: Array(1536).fill(0.5) }], usage: { prompt_tokens: 1, total_tokens: 1 } }
      : { id: 'resp_synthetic', object: 'response', status: 'completed', model: body.model, error: null, incomplete_details: null,
        output: [{ type: 'message', role: 'assistant', status: 'completed', content: [{ type: 'output_text', annotations: [],
          text: body.text.format.name === 'r3_isolated_action_v1' ? '{"action":{"kind":"finish"}}'
            : '{"text":"Insufficient evidence","refused":true,"citations":[]}' }] }],
        usage: { input_tokens: 100, output_tokens: 12, total_tokens: 112, input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } };
    assert(['https://api.openai.com/v1/embeddings', 'https://api.openai.com/v1/responses'].includes(url));
    return new Response(JSON.stringify(response), { headers: { 'Content-Type': 'application/json' } });
  });
  return calls;
}
const payload = manifest => ({ model: { provider: 'openai', resolved_id: manifest.model.resolved_id, reasoning: 'medium' },
  input: { question: 'Synthetic question', context: [] }, response_schema: 'r3-grounded-answer-v1', tools: [] });

test('reviewed provider owner connects exact approvals, fake secret, budget and both API codecs', { skip: !available }, async t => {
  const calls = wire(t), f = await setup(t), directory = path.join(f.root, 'budget');
  await fs.mkdir(directory, { mode: 0o700 });
  const limits = { requests: 3, tokens: taskTokenLimit(f.manifest), microusd: 1000000, task_seconds: 30 };
  const mediator = await openRequestMediator({ binary: budget, directory, manifestSha256: f.options.manifestSha256, limits, taskLimits: limits });
  let retained, owner;
  try {
    const result = await withApprovedProvider(f.options, async provider => {
      owner = provider; retained = provider.bind(mediator, 'work-00000');
      assert.equal((await retained.embed('synthetic source', 'query')).length, 1536);
      const answer = await dispatchModel({ mediator, prepareRequest: retained.prepareRequest, taskId: 'work-00000',
        requestId: 'answer', payload: payload(f.manifest), timeoutMs: 10000 });
      assert.equal(answer.output.refused, true);
    });
    assert.deepEqual(result, { manifest_sha256: f.options.manifestSha256, credential_used: true });
    assert.equal(calls.length, 2);
    assert.equal((await mediator.audit()).audit.ledger.microusd, 130);
    assert.throws(() => owner.bind(mediator, 'work-00001'), /closed/);
    await assert.rejects(retained.prepareRequest(payload(f.manifest)), /not permitted/);
  } finally { await mediator.close(); }
});

test('draft approvals and unreviewed counter bytes never reach key access or provider egress', { skip: !available }, async t => {
  const calls = wire(t);
  for (const mode of ['approval', 'counter']) {
    const f = await setup(t);
    if (mode === 'approval') await fs.writeFile(f.options.approvals[1], '{}');
    else await fs.appendFile(path.join(f.pack, 'private/context-reservation.mjs'), '\n// changed');
    await assert.rejects(withApprovedProvider(f.options, () => assert.fail('no provider')));
    await assert.rejects(fs.stat(f.secretRead), { code: 'ENOENT' });
  }
  assert.deepEqual(calls, []);
});

test('approval revocation between calls poisons the owner and retains an uncertain reservation', { skip: !available }, async t => {
  const calls = wire(t), f = await setup(t), directory = path.join(f.root, 'budget');
  await fs.mkdir(directory, { mode: 0o700 });
  const limits = { requests: 3, tokens: taskTokenLimit(f.manifest), microusd: 1000000, task_seconds: 30 };
  const mediator = await openRequestMediator({ binary: budget, directory, manifestSha256: f.options.manifestSha256, limits, taskLimits: limits });
  try {
    await withApprovedProvider(f.options, async provider => {
      const binding = provider.bind(mediator, 'work-00000');
      const send = requestId => dispatchModel({ mediator, prepareRequest: binding.prepareRequest, taskId: 'work-00000',
        requestId, payload: payload(f.manifest), timeoutMs: 10000 });
      await send('before');
      await fs.unlink(f.options.approvals[1]);
      await assert.rejects(send('revoked'), /uncertain/);
      assert.throws(() => provider.bind(mediator, 'work-00001'), /closed/);
      assert.deepEqual((await mediator.status()).uncertain_ids, ['revoked']);
    });
    assert.equal(calls.length, 1);
  } finally { await mediator.close(); }
});

test('controller cancellation aborts in-flight transport and retains the charge without retry', { skip: !available }, async t => {
  const f = await setup(t), directory = path.join(f.root, 'budget'), stop = new AbortController();
  await fs.mkdir(directory, { mode: 0o700 });
  let entered; const started = new Promise(resolve => { entered = resolve; });
  let calls = 0, aborted = false;
  t.mock.method(globalThis, 'fetch', async (url, options) => {
    assert.equal(url, 'https://api.openai.com/v1/responses');
    calls++; entered();
    return new Promise((_resolve, reject) => {
      options.signal.addEventListener('abort', () => { aborted = true; reject(Error('private response body')); }, { once: true });
    });
  });
  const limits = { requests: 3, tokens: taskTokenLimit(f.manifest), microusd: 1000000, task_seconds: 30 };
  const mediator = await openRequestMediator({ binary: budget, directory, manifestSha256: f.options.manifestSha256, limits, taskLimits: limits });
  try {
    await withApprovedProvider(f.options, async provider => {
      const binding = provider.bind(mediator, 'work-00000');
      const result = dispatchModel({ mediator, prepareRequest: binding.prepareRequest, taskId: 'work-00000',
        requestId: 'interrupted', payload: payload(f.manifest), timeoutMs: 10000 });
      const rejected = assert.rejects(result, /uncertain/);
      await started; stop.abort(); await rejected;
      assert(aborted);
      assert.throws(() => provider.bind(mediator, 'work-00001'), /closed/);
      assert.deepEqual((await mediator.status()).uncertain_ids, ['interrupted']);
      assert.equal((await mediator.status()).ledger.microusd, 318432);
    }, stop.signal);
    assert.equal(calls, 1);
  } finally { await mediator.close(); }
});

test('already cancelled owner does not inspect configuration or read credentials', async () => {
  await assert.rejects(withApprovedProvider({}, () => assert.fail('owner entered'), AbortSignal.abort()), /cancelled/);
});

const sessionNames = ['MEMORY_LAYER_TEST_DATABASE_URL', 'MEMORY_R3_SNAPSHOT_BIN', 'MEMORY_R3_SCORE_BIN', 'MEMORY_R3_RETRIEVER_BIN',
  'MEMORY_R3_TEST_IMAGE', 'MEMORY_R3_RUST_IMAGE'];
const sessionAvailable = available && process.env.MEMORY_LAYER_TEST_REQUIRE_DB === '1' && sessionNames.every(name => process.env[name]);
if (process.env.MEMORY_R3_REQUIRE_APPROVED_SESSION === '1' && !sessionAvailable) throw Error('required approved-session configuration missing');
test('approved owner prepares a sealed service and executes knowledge and a complete coding sequence',
  { skip: !sessionAvailable, timeout: 300000 }, async t => {
    const calls = wire(t), f = await setup(t);
    const profile = f.manifest.provider_profile;
    const configuration = { root: f.pack, manifestSha256: f.options.manifestSha256, budgetBinary: budget,
      nodeImage: process.env.MEMORY_R3_TEST_IMAGE, rustImage: process.env.MEMORY_R3_RUST_IMAGE,
      scoreBinary: process.env.MEMORY_R3_SCORE_BIN, retrieverBinary: process.env.MEMORY_R3_RETRIEVER_BIN,
      driverSha256: f.manifest.assets.find(a => a.path === 'private/approved-campaign-session.mjs').sha256,
      configurationSha256: profile.configuration_evidence.sha256, serviceBuild: 'synthetic-approved-session-test' };
    const session = { offline: false, split: 'development', configuration,
      memoryBinary: memory, snapshotBinary: process.env.MEMORY_R3_SNAPSHOT_BIN, databaseUrl: process.env.MEMORY_LAYER_TEST_DATABASE_URL };
    const contract = async (id, limits) => {
      const directory = path.join(f.pack, 'run', id, 'provider');
      await fs.mkdir(directory, { recursive: true, mode: 0o700 });
      return { directory, manifest_sha256: f.options.manifestSha256, task_id: id, limits, task_limits: limits };
    };
    await withApprovedCampaignSession({ credential: f.options, session }, async active => {
      const receipt = await active.prepare(await contract('preparation', f.manifest.preparation_budget));
      assert(receipt.audit.ledger.requests > 0);
      for (const [index, track, condition] of [[0, 'knowledge', 'full-memory'], [1, 'coding', 'handover']]) {
        const item = f.manifest.cases.find(item => item.track === track);
        const limits = { requests: 3, tokens: taskTokenLimit(f.manifest), microusd: 955296, task_seconds: 300 };
        const observation = await active.execute({ case_id: item.id, condition, repeat: 0 }, await contract(`work-0000${index}`, limits));
        assert.equal(observation.isolation.snapshot_sha256, receipt.snapshot_sha256);
        assert.equal(observation.isolation.before_sha256, observation.isolation.after_sha256);
        assert.equal(observation.provider_requests, index === 0 ? 2 : 3);
        assert(observation.execution_microusd > 0, 'simulated usage at locked rates, not a paid call');
        if (track === 'coding') assert.equal(observation.result.sub_results.length, 3);
      }
      assert.equal(calls.length, receipt.audit.ledger.requests + 5);
    });
  });

test('session identity mismatch fails before loading credentials or contacting any service', async () => {
  await assert.rejects(withApprovedCampaignSession({ credential: { manifest: '/wrong', manifestSha256: 'a'.repeat(64), split: 'development' },
    session: { offline: false, split: 'development', configuration: { root: '/unused', manifestSha256: 'a'.repeat(64) } } },
  () => assert.fail('must not start session')), /identity mismatch/);
});
