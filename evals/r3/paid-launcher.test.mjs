// SPDX-License-Identifier: AGPL-3.0-or-later
// Only temporary synthetic approval records, fake Bao and mocked OpenAI traffic.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { packagePaidRuntime, verifyPaidRuntime, digest } from './paid-runtime.mjs';
import { runPaidCampaign } from './launch-paid.mjs';
import { inspectCampaignDispatch } from './campaign-dispatch.mjs';

const memory = process.env.MEMORY_R3_CLI_BIN;
const names = ['MEMORY_R3_BUDGET_BIN', 'MEMORY_R3_SNAPSHOT_BIN', 'MEMORY_R3_SCORE_BIN', 'MEMORY_R3_RETRIEVER_BIN',
  'MEMORY_R3_TEST_IMAGE', 'MEMORY_R3_RUST_IMAGE'];
const available = !!memory && names.every(n => process.env[n]);
const database = process.env.MEMORY_LAYER_TEST_DATABASE_URL;
const integration = available && !!database && process.env.MEMORY_LAYER_TEST_REQUIRE_DB === '1';
if (process.env.MEMORY_R3_REQUIRE_PAID_LAUNCHER === '1' && !integration) throw Error('required paid-launcher fixture configuration missing');
async function fixture(t, reviewed = true) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-paid-'));
  let keep = false;
  t.after(async () => { if (keep && process.env.MEMORY_R3_KEEP_REHEARSAL === '1') console.error(`Synthetic paid-path evidence retained at ${directory}`); else await fs.rm(directory, { recursive: true }); });
  const root = path.join(directory, 'pack'), file = path.join(root, 'campaign.json');
  const built = spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', root, '--one-family-per-track'], { encoding: 'utf8' });
  assert.equal(built.status, 0, built.stderr);
  const manifest = JSON.parse(await fs.readFile(file));
  manifest.model = { provider: 'openai', resolved_id: 'gpt-5.4-mini-2026-03-17', reasoning: 'medium', embedding_id: 'text-embedding-3-small',
    max_input_tokens: 400000, max_output_tokens: 4096, input_microusd_per_million: 750000,
    output_microusd_per_million: 4500000, cached_input_microusd_per_million: 75000 };
  manifest.provider_budget = { max_calls_per_task: 12 };
  manifest.limits = { requests: 500, tokens: 10000000, microusd: 10000000, task_seconds: 300 };
  manifest.preparation_budget = { requests: 100, tokens: 819200, microusd: 20000, task_seconds: 300 };
  const source = await fs.readFile(new URL('./context-reservation.mjs', import.meta.url));
  const pin = { path: 'private/context-reservation.mjs', sha256: digest(source) };
  await fs.writeFile(path.join(root, pin.path), source);
  manifest.assets.push({ ...pin, kind: 'configuration' });
  manifest.provider_profile = { version: 2, protocol: 'openai-text-v1', input_accounting: 'full_context_reservation',
    responses_tokenizer: { id: 'full-context-responses-v1', asset: pin },
    embedding: { resolved_id: 'text-embedding-3-small', dimensions: 1536, max_input_tokens: 8192,
      input_microusd_per_million: 20000, tokenizer: { id: 'full-context-embeddings-v1', asset: pin } }, configuration_evidence: pin };
  await fs.writeFile(file, JSON.stringify(manifest));
  const bao = path.join(directory, 'fake-bao'), secretRead = path.join(directory, 'secret-reads');
  await fs.writeFile(bao, `#!${process.execPath}\nrequire('node:fs').appendFileSync(${JSON.stringify(secretRead)}, 'read\\n');process.stdout.write('synthetic-paid-launcher');`, { mode: 0o700 });
  const runtime = await packagePaidRuntime(root, { binaries: { memory, bao, node: process.execPath,
    budget: process.env.MEMORY_R3_BUDGET_BIN, snapshot: process.env.MEMORY_R3_SNAPSHOT_BIN,
    score: process.env.MEMORY_R3_SCORE_BIN, retriever: process.env.MEMORY_R3_RETRIEVER_BIN },
    images: { node: process.env.MEMORY_R3_TEST_IMAGE, rust: process.env.MEMORY_R3_RUST_IMAGE } });
  const locked = JSON.parse(await fs.readFile(file));
  // Test-only approval. Production packaging never changes this status or writes approvals.
  if (reviewed) { locked.label_status = 'reviewed'; await fs.writeFile(file, JSON.stringify(locked)); }
  const plan = spawnSync(memory, ['eval', 'campaign', 'plan', '--manifest', file], { encoding: 'utf8' });
  assert.equal(plan.status, 0, plan.stderr);
  const identity = JSON.parse(plan.stdout).manifest_sha256, approvals = [];
  for (const purpose of ['labels', 'development']) {
    const approval = path.join(directory, `${purpose}.json`);
    await fs.writeFile(approval, JSON.stringify({ purpose, manifest_sha256: identity,
      reviewer: 'synthetic-launcher-test-only', approved_at: '2026-01-01T00:00:00Z', maximum_microusd: 10000000 }), { mode: 0o600 });
    approvals.push(approval);
  }
  const settings = path.join(directory, 'settings.json');
  await fs.writeFile(settings, JSON.stringify({ version: 1, approvals, split: 'development',
    disposable_database: true, database_url: database ?? 'postgresql://synthetic:synthetic@127.0.0.1:9/memory_test',
    secret: { address: 'https://unreachable.example.test', mount: 'secret', path: 'synthetic/evaluation', field: 'OPENAI_API_KEY', version: 1 } }), { mode: 0o600 });
  return { directory, root, settings, file, runtime, approvals, secretRead, keep: () => { keep = true; } };
}

function mockProvider(t, onCall) {
  const native = globalThis.fetch.bind(globalThis), calls = [];
  t.mock.method(globalThis, 'fetch', async (url, options) => {
    if (!String(url).startsWith('https://api.openai.com/')) {
      assert.match(String(url), /^http:\/\/127\.0\.0\.1:[0-9]+\//);
      return native(url, options);
    }
    assert.equal(options.headers.Authorization, 'Bearer synthetic-paid-launcher');
    assert.equal(options.redirect, 'error'); calls.push(url);
    if (onCall) await onCall(options, calls.length);
    const body = JSON.parse(options.body);
    const result = url === 'https://api.openai.com/v1/embeddings'
      ? { model: body.model, object: 'list', data: [{ object: 'embedding', index: 0,
        embedding: Array.from({ length: 1536 }, (_, i) => Math.fround(((i * 7919) % 65521 - 32760) / 1000003)) }],
        usage: { prompt_tokens: 1, total_tokens: 1 } }
      : { id: 'resp_synthetic', object: 'response', status: 'completed', model: body.model, error: null, incomplete_details: null,
        output: [{ type: 'message', role: 'assistant', status: 'completed', content: [{ type: 'output_text', annotations: [],
          text: body.text.format.name === 'r3_isolated_action_v1' ? '{"action":{"kind":"finish"}}' : '{"text":"Insufficient evidence","refused":true,"citations":[]}' }] }],
        usage: { input_tokens: 100, output_tokens: 12, total_tokens: 112, input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } };
    return new Response(JSON.stringify(result), { headers: { 'Content-Type': 'application/json' } });
  });
  return calls;
}

test('runtime packaging pins the closure and rejects tampering, repackaging and moved packs', { skip: !available }, async t => {
  const f = await fixture(t, false);
  await verifyPaidRuntime(f.root);
  const driver = await fs.readFile(path.join(f.root, 'runtime/approved-driver.mjs'), 'utf8');
  assert(driver.startsWith(`#!${f.root}/private/executables/node\n`));
  await assert.rejects(packagePaidRuntime(f.root, { binaries: {}, images: {} }));
  const file = path.join(f.root, 'runtime/context-reservation.mjs');
  await fs.chmod(file, 0o600); await fs.appendFile(file, '\n// changed');
  await assert.rejects(verifyPaidRuntime(f.root), /changed/);
  const moved = path.join(f.directory, 'moved'); await fs.rename(f.root, moved);
  await assert.rejects(verifyPaidRuntime(moved), /invalid locked runtime/);
});

test('draft or missing approval prevents secret access, driver start and provider calls', { skip: !available }, async t => {
  const calls = mockProvider(t);
  for (const draft of [true, false]) {
    const f = await fixture(t, !draft);
    if (!draft) await fs.unlink(f.approvals[1]);
    await assert.rejects(runPaidCampaign(f.root, f.settings));
    await assert.rejects(fs.stat(f.secretRead), { code: 'ENOENT' });
    await assert.rejects(fs.stat(path.join(f.root, 'run')), { code: 'ENOENT' });
  }
  assert.equal(calls.length, 0);
});

test('calibration preparation creates a locked draft without approving labels or reading a key', { skip: !available }, async t => {
  const f = await fixture(t, false), inputs = path.join(f.directory, 'runtime-inputs.json');
  await fs.writeFile(inputs, JSON.stringify({ binaries: Object.fromEntries(Object.entries(f.runtime.binaries)
    .map(([name, pin]) => [name, path.join(f.root, pin.path)])), images: f.runtime.images }));
  const out = path.join(f.directory, 'calibration');
  const result = spawnSync(process.execPath, ['evals/r3/prepare-paid-calibration.mjs', out, inputs], { encoding: 'utf8', timeout: 60000 });
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.equal(report.approval_created, false); assert.equal(report.provider_called, false);
  assert.equal(report.task_executions, 75); assert.equal(report.labels, 'draft');
  const { manifest } = await verifyPaidRuntime(out);
  assert.deepEqual([...new Set(manifest.cases.map(c => c.cluster))].sort(), ['artifact-origin', 'configuration-rollout']);
  assert.equal(manifest.label_status, 'draft'); assert.equal(manifest.limits.microusd, 10000000);
  await assert.rejects(fs.stat(f.secretRead), { code: 'ENOENT' });
  const again = spawnSync(process.execPath, ['evals/r3/prepare-paid-calibration.mjs', out, inputs], { encoding: 'utf8', timeout: 20000 });
  assert.equal(again.status, 1, 'never overwrite an existing pack');
});

test('approved runtime completes all 75 paid-path tasks with fake responses and private reviews pending',
  { skip: !integration, timeout: 1500000 }, async t => {
    const f = await fixture(t), calls = mockProvider(t); f.keep();
    const result = await runPaidCampaign(f.root, f.settings);
    assert.equal(result.execution_complete, true); assert.equal(result.execution_valid, true);
    assert.equal(result.task_executions, 75); assert.equal(result.knowledge_reviews_pending, 60);
    assert.equal(result.harness_valid, false); assert.equal(result.quality_passed, false);
    const artifact = JSON.parse(await fs.readFile(result.artifact));
    assert.equal(artifact.offline, false, 'non-offline accounting path, NOT a real paid evaluation');
    assert(artifact.preparation_microusd > 0);
    assert(artifact.observations.every(o => o.execution_microusd > 0 && o.isolation.before_sha256 === o.isolation.after_sha256));
    assert.equal(calls.length, artifact.preparation_receipt.audit.ledger.requests + artifact.observations.reduce((n, o) => n + o.provider_requests, 0));
    const journal = await inspectCampaignDispatch(path.join(f.root, 'private/dispatch'));
    assert.equal(journal.entries.length, 76); assert(journal.entries.every(e => e.status === 'completed'));
    await assert.rejects(runPaidCampaign(f.root, f.settings), /already started/);
    assert.equal(await fs.readFile(f.secretRead, 'utf8'), 'read\n');
  });

test('interrupting provider preparation aborts transport and refuses restart before a second secret read',
  { skip: !integration, timeout: 180000 }, async t => {
    const f = await fixture(t), abort = new AbortController();
    const calls = mockProvider(t, async options => {
      setImmediate(() => abort.abort());
      await new Promise((_resolve, reject) => options.signal.addEventListener('abort', () => reject(Error('synthetic lost response')), { once: true }));
    });
    await assert.rejects(runPaidCampaign(f.root, f.settings, abort.signal));
    assert.equal(calls.length, 1);
    const journal = await inspectCampaignDispatch(path.join(f.root, 'private/dispatch'));
    assert.deepEqual(journal.entries.map(e => e.status), ['uncertain']);
    await assert.rejects(runPaidCampaign(f.root, f.settings), /already started/);
    assert.equal(await fs.readFile(f.secretRead, 'utf8'), 'read\n');
    const artifact = JSON.parse(await fs.readFile(path.join(f.root, 'run/artifact.json')));
    assert.equal(artifact.completed, false);
  });

test('frozen launcher CLI handles SIGTERM and preserves uncertain paid-path preparation',
  { skip: !integration, timeout: 180000 }, async t => {
    const f = await fixture(t);
    const result = await new Promise((resolve, reject) => {
      const child = spawn(path.join(f.root, 'private/executables/node'),
        ['--import', fileURLToPath(new URL('./test-fixtures/interrupt-provider.mjs', import.meta.url)),
          path.join(f.root, 'runtime/launch-paid.mjs'), f.settings, '--allow-shell'],
        { env: { PATH: '/usr/bin:/bin', HOME: f.directory }, stdio: ['ignore', 'pipe', 'pipe'] });
      const timer = setTimeout(() => child.kill('SIGKILL'), 150000);
      let stdout = '', stderr = '';
      child.stdout.on('data', c => { stdout += c; }); child.stderr.on('data', c => { stderr += c; });
      child.once('error', reject);
      child.once('close', code => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
    });
    assert.equal(result.code, 1);
    assert(!result.stdout.includes('synthetic-paid-launcher') && !result.stderr.includes('synthetic-paid-launcher'));
    const journal = await inspectCampaignDispatch(path.join(f.root, 'private/dispatch'));
    assert.deepEqual(journal.entries.map(e => e.status), ['uncertain']);
    const artifact = JSON.parse(await fs.readFile(path.join(f.root, 'run/artifact.json')));
    assert.equal(artifact.completed, false);
    assert.equal(await fs.readFile(f.secretRead, 'utf8'), 'read\n');
  });

test('SIGKILL of the frozen launcher keeps its fence and never invokes preparation again',
  { skip: !integration, timeout: 180000 }, async t => {
    const f = await fixture(t);
    const loader = new URL('./test-fixtures/interrupt-provider.mjs?signal=SIGKILL', import.meta.url).href;
    const exit = await new Promise((resolve, reject) => {
      const child = spawn(path.join(f.root, 'private/executables/node'), ['--import', loader,
        path.join(f.root, 'runtime/launch-paid.mjs'), f.settings, '--allow-shell'],
      { env: { PATH: '/usr/bin:/bin', HOME: f.directory }, stdio: 'ignore' });
      const timer = setTimeout(() => child.kill('SIGKILL'), 150000);
      child.once('error', reject);
      child.once('close', (code, signal) => { clearTimeout(timer); resolve({ code, signal }); });
    });
    assert.equal(exit.signal, 'SIGKILL');
    assert.deepEqual((await inspectCampaignDispatch(path.join(f.root, 'private/dispatch'))).entries.map(e => e.status), ['uncertain']);
    await assert.rejects(runPaidCampaign(f.root, f.settings), /already started/);
    assert.equal(await fs.readFile(f.secretRead, 'utf8'), 'read\n');
    // The orphaned runner gets a broken IPC connection and retains incomplete
    // evidence. Never start a second owner to manufacture a success receipt.
    let artifact;
    for (let i = 0; i < 100; i++) {
      try { artifact = JSON.parse(await fs.readFile(path.join(f.root, 'run/artifact.json'))); break; } catch { /* wait for owned runner */ }
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert(artifact); assert.equal(artifact.completed, false);
  });
