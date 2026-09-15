// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { openRequestMediator } from './request-mediator.mjs';

const binary = process.env.MEMORY_R3_BUDGET_BIN;
if (process.env.MEMORY_R3_REQUIRE_MEDIATOR === '1' && !binary) throw Error('required budget binary missing');
const limits = { requests: 4, tokens: 100, microusd: 50, task_seconds: 10 };
const taskLimits = { ...limits, requests: 2 };
const allowance = { input_tokens: 10, output_tokens: 10, microusd: 10 };
const usage = { input_tokens: 3, output_tokens: 2, microusd: 2 };
const payload = { model: 'synthetic-only', purpose: 'answer', input: 'PRIVATE_SYNTHETIC_INPUT' };

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
