// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import http from 'node:http';
import { openRequestMediator } from './request-mediator.mjs';
import { createProviderTransport } from './provider-transport.mjs';
import { decodeResponsesReceipt } from './responses-receipt.mjs';
import { providerFailure } from './provider-diagnostics.mjs';

const binary = process.env.MEMORY_R3_BUDGET_BIN;
if (process.env.MEMORY_R3_REQUIRE_MEDIATOR === '1' && !binary) throw Error('required budget binary missing');
const limits = { requests: 4, tokens: 100, microusd: 50, task_seconds: 10 };
const taskLimits = { ...limits, requests: 2 };
const allowance = { input_tokens: 10, output_tokens: 10, microusd: 10 };
const usage = { input_tokens: 3, output_tokens: 2, microusd: 2 };
const payload = { model: 'synthetic-only', purpose: 'answer', input: 'PRIVATE_SYNTHETIC_INPUT' };

test('uncertain calls retain metadata-only diagnostics without releasing reservations', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-diagnostics-'));
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'c'.repeat(64), limits, taskLimits });
  try {
    await assert.rejects(mediator.dispatch({ taskId: 'task', requestId: 'first', payload, allowance, timeoutMs: 500,
      invoke: async () => { throw providerFailure('http', { http_status: 400, api_code: 'invalid_json_schema',
        parameter: 'text.format.schema', message: 'PRIVATE_ERROR' }); } }), /outcome uncertain/);
    const files = (await fs.readdir(directory)).filter(name => name.startsWith('failure-'));
    assert.equal(files.length, 1);
    const bytes = await fs.readFile(path.join(directory, files[0]), 'utf8');
    assert(!bytes.includes('PRIVATE'));
    assert.equal(JSON.parse(bytes).api_code, 'invalid_json_schema');
    assert.equal((await fs.stat(path.join(directory, files[0]))).mode & 0o077, 0);
    assert.deepEqual((await mediator.status()).uncertain_ids, ['first']);
    assert.equal((await mediator.status()).ledger.microusd, 10);
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});

test('task cancellation stops new calls and retains an uncertain reservation for an in-flight call', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-cancelled-mediator-'));
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'c'.repeat(64), limits, taskLimits });
  let release;
  try {
    const request = { taskId: 'task', requestId: 'cancelled', payload, allowance, timeoutMs: 5000 };
    await assert.rejects(mediator.dispatch({ ...request, signal: AbortSignal.abort(),
      invoke: () => assert.fail('cancelled call invoked') }), /before reservation/);
    assert.deepEqual((await mediator.status()).ledger, { requests: 0, tokens: 0, microusd: 0 });
    const controller = new AbortController();
    let entered, providerSignal;
    const started = new Promise(resolve => { entered = resolve; });
    const blocked = new Promise(resolve => { release = resolve; });
    const pending = mediator.dispatch({ ...request, signal: controller.signal, invoke: async (_body, signal) => {
      providerSignal = signal; entered(); await blocked;
      return { output: { late: true }, usage };
    } });
    const rejected = assert.rejects(pending, /outcome uncertain/);
    await started;
    controller.abort(Error('PRIVATE_CANCEL_REASON'));
    await rejected;
    assert(providerSignal.aborted);
    release();
    await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(await mediator.status(), { ledger: { requests: 1, tokens: 20, microusd: 10 }, uncertain_ids: ['cancelled'] });
    await assert.rejects(mediator.dispatch({ ...request, invoke: () => assert.fail('uncertain replay invoked') }), /rejected/);
    const journal = await fs.readFile(path.join(directory, 'budget.jsonl'), 'utf8');
    assert(!journal.includes('PRIVATE_CANCEL_REASON'));
  } finally { release?.(); await mediator.close(); await fs.rm(directory, { recursive: true }); }
});

test('price bridge uses checked Rust arithmetic without reserving a call or inventing cache rates', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-price-bridge-'));
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'c'.repeat(64), limits, taskLimits });
  const model = { provider: 'fake', resolved_id: 'fake-v1', reasoning: 'none', embedding_id: 'fake-vectors',
    max_input_tokens: 100, max_output_tokens: 100, input_microusd_per_million: 2000000, output_microusd_per_million: 4000000 };
  try {
    assert.deepEqual(await mediator.price({ model, inputTokens: 20, outputTokens: 10 }),
      { input_tokens: 20, output_tokens: 10, microusd: 80 });
    await assert.rejects(mediator.price({ model, inputTokens: 20, outputTokens: 10, cachedInputTokens: 5 }), /rejected/);
    model.cached_input_microusd_per_million = 500000;
    assert.equal((await mediator.price({ model, inputTokens: 20, outputTokens: 10, cachedInputTokens: 5 })).microusd, 73);
    assert.deepEqual((await mediator.status()).ledger, { requests: 0, tokens: 0, microusd: 0 });
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});

test('loopback HTTP receipts are reserved before transmission, cached and rejected when usage disappears', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-mediated-http-'));
  let calls = 0;
  const server = http.createServer(async (_req, res) => {
    calls++;
    const journal = await fs.readFile(path.join(directory, 'budget.jsonl'), 'utf8');
    assert.equal(journal.split('\n').filter(line => line.includes('"event":"reserved"')).length, calls);
    const response = { id: 'resp_synthetic', object: 'response', status: 'completed', model: 'synthetic-only',
      error: null, incomplete_details: null,
      output: [{ type: 'message', role: 'assistant', status: 'completed', content: [
        { type: 'output_text', text: '{"kind":"finish"}', annotations: [] }] }],
      usage: { input_tokens: 3, output_tokens: 2, total_tokens: 5,
        input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } };
    if (calls > 1) delete response.usage;
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(response));
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  let mediator;
  try {
    const send = createProviderTransport({ endpoint: `http://127.0.0.1:${server.address().port}/v1/responses`, bearer: 'synthetic-test' });
    mediator = await openRequestMediator({ binary, directory, manifestSha256: 'f'.repeat(64), limits, taskLimits });
    const invoke = async (body, signal) => {
      const receipt = decodeResponsesReceipt(await send(body, signal), { model: 'synthetic-only', maxInputTokens: 10, maxOutputTokens: 10 });
      // Explicit synthetic cost: this is not real provider price reconciliation.
      return { output: receipt.output, usage: { ...receipt.tokenUsage, microusd: 0 } };
    };
    const dispatch = requestId => mediator.dispatch({ taskId: 'task', requestId, payload, allowance, timeoutMs: 2000, invoke });
    assert.deepEqual(await dispatch('first'), { kind: 'finish' });
    assert.deepEqual(await dispatch('first'), { kind: 'finish' });
    assert.equal(calls, 1);
    await assert.rejects(dispatch('second'), /uncertain/);
    await assert.rejects(dispatch('third'), /rejected/);
    assert.equal(calls, 2);
    assert.deepEqual((await mediator.status()).uncertain_ids, ['second']);
    await assert.rejects(mediator.audit(), /rejected/);
  } finally {
    await mediator?.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await fs.rm(directory, { recursive: true });
  }
});

test('killing the actual budget owner after reservation does not allow a replayed call', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-mediator-kill-'));
  const configPath = path.join(directory, 'synthetic-config.json');
  await fs.writeFile(configPath, JSON.stringify({ version: 1, manifest_sha256: 'd'.repeat(64), limits, task_limits: taskLimits, resume: false }), { mode: 0o600 });
  const child = spawn(binary, [configPath, directory], { env: { PATH: '/usr/bin:/bin', HOME: directory }, stdio: ['pipe', 'pipe', 'ignore'] });
  let mediator;
  try {
    const reply = once(child.stdout, 'data');
    child.stdin.write(JSON.stringify({ operation: 'begin', task_id: 'task', request_id: 'first', payload_sha256: 'e'.repeat(64), allowance }) + '\n');
    assert.equal(JSON.parse((await reply)[0]).result.state, 'reserved');
    const stopped = once(child, 'exit');
    child.kill('SIGKILL');
    await stopped;
    mediator = await openRequestMediator({ binary, directory, manifestSha256: 'd'.repeat(64), limits, taskLimits, resume: true });
    assert.deepEqual((await mediator.status()).uncertain_ids, ['first']);
    await assert.rejects(mediator.dispatch({ taskId: 'task', requestId: 'new', payload, allowance, timeoutMs: 1000,
      invoke: () => assert.fail('must not invoke after ambiguous crash') }), /rejected/);
  } finally {
    if (child.exitCode === null && child.signalCode === null) { const stopped = once(child, 'exit'); child.kill('SIGKILL'); await stopped; }
    await mediator?.close();
    await fs.rm(directory, { recursive: true });
  }
});

test('actual per-call bridge reserves durably, aggregates sequence steps, reuses outputs and survives restart', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-mediator-'));
  const config = { binary, directory, manifestSha256: 'a'.repeat(64), limits, taskLimits };
  let mediator = await openRequestMediator(config), calls = 0;
  const invoke = async () => {
    const journal = await fs.readFile(path.join(directory, 'budget.jsonl'), 'utf8');
    assert(journal.includes('reserved'));
    assert(!journal.includes(payload.input));
    calls++;
    return { output: { text: 'PRIVATE_SYNTHETIC_OUTPUT' }, usage };
  };
  const dispatch = (requestId, taskId = 'sequence-one', override = {}) => mediator.dispatch({
    taskId, requestId, payload, allowance, timeoutMs: 1000, invoke, ...override });
  try {
    await dispatch('step-one-call-one');
    await dispatch('step-two-call-one');
    await assert.rejects(dispatch('step-three-call-one'), /budget or receipt rejected/);
    assert.equal(calls, 2);
    assert.deepEqual(await dispatch('step-one-call-one'), { text: 'PRIVATE_SYNTHETIC_OUTPUT' });
    await assert.rejects(dispatch('step-one-call-one', 'sequence-one', { payload: { ...payload, model: 'changed' } }), /rejected/);
    assert.equal(calls, 2);
    assert.deepEqual((await mediator.status()).ledger, { requests: 2, tokens: 10, microusd: 4 });
    const audit = await mediator.audit();
    assert.match(audit.sha256, /^[a-f0-9]{64}$/);
    assert.equal(audit.audit.calls.length, 2);
    assert.deepEqual(audit.audit.tasks['sequence-one'], { requests: 2, input_tokens: 6, output_tokens: 4, microusd: 4 });
    assert(!JSON.stringify(audit).includes('PRIVATE_SYNTHETIC'));
    await mediator.close();
    mediator = await openRequestMediator({ ...config, resume: true });
    await dispatch('step-two-call-one');
    assert.equal(calls, 2);
    assert.deepEqual(await mediator.audit(), audit);
    await dispatch('other-one', 'sequence-two');
    await dispatch('other-two', 'sequence-two');
    await assert.rejects(dispatch('another', 'sequence-three'), /rejected/);
    assert.equal(calls, 4);
    const journal = await fs.readFile(path.join(directory, 'budget.jsonl'), 'utf8');
    assert(!journal.includes('PRIVATE_SYNTHETIC'));
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});

for (const failure of ['throw', 'missing-usage', 'over-usage', 'timeout']) {
  test(`per-call ${failure} retains allowance and blocks new calls across restart`, { skip: !binary }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-mediator-fault-'));
    const config = { binary, directory, manifestSha256: 'b'.repeat(64), limits, taskLimits };
    let mediator = await openRequestMediator(config), calls = 0;
    const invoke = async (_payload, signal) => {
      calls++;
      if (failure === 'throw') throw Error('PRIVATE_PROVIDER_BODY');
      if (failure === 'timeout') return new Promise((_, reject) => signal.addEventListener('abort', () => reject(Error('aborted')), { once: true }));
      return { output: 'test', ...(failure === 'over-usage' ? { usage: { ...usage, input_tokens: 11 } } : {}) };
    };
    const dispatch = id => mediator.dispatch({ taskId: 'task', requestId: id, payload, allowance, timeoutMs: 50, invoke });
    try {
      await assert.rejects(dispatch('first'), error => /uncertain/.test(error.message) && !error.message.includes('PRIVATE'));
      assert.deepEqual(await mediator.status(), { ledger: { requests: 1, tokens: 20, microusd: 10 }, uncertain_ids: ['first'] });
      await assert.rejects(mediator.audit(), /rejected/);
      await assert.rejects(dispatch('second'), /rejected/);
      await mediator.close();
      mediator = await openRequestMediator({ ...config, resume: true });
      await assert.rejects(dispatch('first'), /rejected/);
      await assert.rejects(dispatch('new'), /rejected/);
      assert.equal(calls, 1);
    } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
  });
}
