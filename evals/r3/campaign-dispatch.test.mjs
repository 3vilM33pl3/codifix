// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { openCampaignDispatch, inspectCampaignDispatch } from './campaign-dispatch.mjs';

const work = [0, 1].map(repeat => ({ case_id: 'case', condition: 'handover', repeat }));
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
async function fixture(t, overrides = {}) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-dispatch-'));
  t.after(() => fs.rm(root, { recursive: true }));
  const options = { root, directory: path.join(root, 'dispatch'), socket: path.join(root, 'dispatch.sock'),
    manifestSha256: 'a'.repeat(64), work, offline: true, preparation: true,
    verify: async () => {}, prepare: async () => ({ prepared: true }), execute: async () => ({ answer: 'private' }), ...overrides };
  const request = (item = 'preparation') => ({ version: 1, ...(item === 'preparation' ? { phase: 'preparation' } : { work: work[item] }),
    offline: true, maximum_microusd: 0, manifest: path.join(root, 'campaign.json'), manifest_sha256: options.manifestSha256,
    response: path.join(root, 'run', item === 'preparation' ? item : `work-0000${item}`, item === 'preparation' ? 'receipt.json' : 'observation.json'),
    provider_budget: { test: 'synthetic-only' } });
  return { options, request, root };
}
function post(socket, body) {
  return new Promise((resolve, reject) => {
    const bytes = JSON.stringify(body);
    const request = http.request({ socketPath: socket, path: '/dispatch', method: 'POST', agent: false,
      headers: { 'Content-Length': Buffer.byteLength(bytes) }, timeout: 4000 }, response => {
      let text = '';
      response.on('data', data => { text += data; });
      response.once('end', () => resolve({ status: response.statusCode, text }));
      response.once('error', reject);
    });
    request.once('timeout', () => request.destroy(Error('test deadline')));
    request.once('error', reject); request.end(bytes);
  });
}
async function open(t, options) {
  const owner = await openCampaignDispatch(options);
  t.after(() => owner.close());
  return owner;
}

test('ordered duplicate delivery reuses durable receipts and inspection omits payloads', async t => {
  let preparations = 0, executions = 0;
  const f = await fixture(t, { prepare: async () => { preparations++; return { prepared: true }; },
    execute: async () => { executions++; return { answer: 'private answer' }; } });
  const owner = await open(t, f.options), send = item => post(f.options.socket, f.request(item));
  assert.equal((await send(0)).status, 409);
  assert.equal((await send()).status, 200);
  assert.equal((await send()).status, 200);
  assert.equal((await send(1)).status, 409);
  const first = await send(0);
  assert.equal(first.status, 200);
  assert.deepEqual(await send(0), first);
  assert.equal((await post(f.options.socket, { ...f.request(0), maximum_microusd: 1 })).status, 409);
  assert.equal((await send(1)).status, 200);
  assert.equal((await post(f.options.socket, { ...f.request(1), work: {} })).status, 409);
  assert.equal((await post(f.options.socket, { ...f.request(1), work: { ...work[1], repeat: 2 } })).status, 409);
  assert.equal(owner.completed, 2);
  assert.equal(preparations, 1); assert.equal(executions, 2);
  await owner.close();
  const audit = await inspectCampaignDispatch(f.options.directory);
  assert.equal(audit.execution_enabled, false);
  const inspected = spawnSync(process.execPath, ['evals/r3/inspect-dispatch.mjs', f.options.directory], { encoding: 'utf8' });
  assert.equal(inspected.status, 0);
  assert.deepEqual(JSON.parse(inspected.stdout), audit);
  assert.deepEqual(audit.entries.map(e => e.status), ['completed', 'completed', 'completed']);
  assert(!JSON.stringify(audit).includes('private answer'));
  for (const name of await fs.readdir(f.options.directory)) {
    assert.equal((await fs.stat(path.join(f.options.directory, name))).mode & 0o077, 0);
  }
  await assert.rejects(openCampaignDispatch(f.options), { code: 'EEXIST' });
});

test('concurrent exact duplicates share work; altered retry and competing work are refused', async t => {
  const entered = deferred(), finish = deferred(); let executions = 0;
  const f = await fixture(t, { preparation: false, execute: async () => {
    executions++; entered.resolve(); await finish.promise; return { answer: 'once' };
  } });
  await open(t, f.options);
  const first = post(f.options.socket, f.request(0));
  await entered.promise;
  const duplicate = post(f.options.socket, f.request(0));
  assert.equal((await post(f.options.socket, f.request(1))).status, 409);
  assert.equal((await post(f.options.socket, { ...f.request(0), provider_budget: {} })).status, 409);
  finish.resolve();
  assert.deepEqual(await first, await duplicate);
  assert.equal(executions, 1);
});

test('lost response is safe to redeliver and failed verification stops the next dispatch', async t => {
  const entered = deferred(), finish = deferred(); let calls = 0, changed = false;
  const f = await fixture(t, { preparation: false, verify: async () => { if (changed) throw Error('private content'); },
    execute: async () => { calls++; entered.resolve(); await finish.promise; return { answer: 'retained' }; } });
  await open(t, f.options);
  const bytes = JSON.stringify(f.request(0));
  const connection = http.request({ socketPath: f.options.socket, path: '/dispatch', method: 'POST',
    headers: { 'Content-Length': Buffer.byteLength(bytes) }, agent: false });
  connection.on('error', () => {}); connection.end(bytes);
  await entered.promise; connection.destroy(); finish.resolve();
  assert.equal((await post(f.options.socket, f.request(0))).status, 200);
  changed = true;
  const denied = await post(f.options.socket, f.request(1));
  assert.equal(denied.status, 409); assert(!denied.text.includes('private content'));
  assert.equal(calls, 1);
  assert.equal((await inspectCampaignDispatch(f.options.directory)).entries.length, 1);
  changed = false;
  assert.equal((await post(f.options.socket, f.request(0))).status, 409, 'restoring assets cannot revive a retired owner');
});

test('inspection rejects partial, mismatched and symlink receipts without exposing unknown metadata', async t => {
  const f = await fixture(t, { preparation: false });
  const owner = await open(t, f.options);
  assert.equal((await post(f.options.socket, f.request(0))).status, 200);
  await owner.close();
  const identityFile = path.join(f.options.directory, 'identity.json');
  const identity = JSON.parse(await fs.readFile(identityFile));
  await fs.writeFile(identityFile, JSON.stringify({ ...identity, unwanted_payload: 'private question' }));
  assert(!JSON.stringify(await inspectCampaignDispatch(f.options.directory)).includes('private question'));
  const receiptFile = path.join(f.options.directory, 'work-00000.receipt.json');
  const receipt = JSON.parse(await fs.readFile(receiptFile));
  await fs.writeFile(receiptFile, '{');
  await assert.rejects(inspectCampaignDispatch(f.options.directory));
  await fs.writeFile(receiptFile, JSON.stringify({ ...receipt, request_sha256: 'b'.repeat(64) }));
  await assert.rejects(inspectCampaignDispatch(f.options.directory), /invalid dispatch receipt/);
  await fs.unlink(receiptFile);
  await fs.symlink(identityFile, receiptFile);
  await assert.rejects(inspectCampaignDispatch(f.options.directory), /invalid dispatch evidence/);
});

test('cancellation retires execution, awaits work and keeps uncertain attempt metadata', async t => {
  const entered = deferred(); let cancelled = false;
  const f = await fixture(t, { preparation: false, execute: async (_work, _contract, signal) => {
    entered.resolve(); await once(signal, 'abort'); cancelled = true; throw Error('private provider failure');
  } });
  const owner = await open(t, f.options);
  const pending = post(f.options.socket, f.request(0)).catch(() => undefined);
  await entered.promise; await owner.close(); await pending;
  assert(cancelled); assert(owner.signal.aborted);
  assert.equal((await inspectCampaignDispatch(f.options.directory)).entries[0].status, 'uncertain');
  const inspected = spawnSync(process.execPath, ['evals/r3/inspect-dispatch.mjs', f.options.directory], { encoding: 'utf8' });
  assert.equal(inspected.status, 2);
  assert(!inspected.stdout.includes('private provider failure'));
  await assert.rejects(openCampaignDispatch(f.options), { code: 'EEXIST' });
});

test('receipt publication failure never blesses a completed external effect or reruns it', async t => {
  let calls = 0;
  const f = await fixture(t, { preparation: false });
  f.options.execute = async () => {
    calls++;
    await fs.mkdir(path.join(f.options.directory, 'work-00000.receipt.json'));
    return { external_effect: true };
  };
  await open(t, f.options);
  assert.equal((await post(f.options.socket, f.request(0))).status, 409);
  assert.equal((await post(f.options.socket, f.request(0))).status, 409);
  assert.equal((await post(f.options.socket, f.request(1))).status, 409);
  assert.equal(calls, 1);
  await assert.rejects(inspectCampaignDispatch(f.options.directory), /invalid dispatch evidence/);
});

for (const point of ['before-claim', 'during-effect', 'after-receipt']) {
  test(`SIGKILL ${point} preserves a cross-process fence without replay`, { timeout: 15000 }, async t => {
    const f = await fixture(t, { preparation: false });
    const source = `import { openCampaignDispatch } from ${JSON.stringify(new URL('./campaign-dispatch.mjs', import.meta.url).href)};
      const o = JSON.parse(process.argv[1]);
      await openCampaignDispatch({ ...o, verify: async () => {}, execute: async () => {
        process.stdout.write('effect\\n');
        ${point === 'during-effect' ? 'await new Promise(() => {});' : ''}
        return { private_answer: true };
      } }); process.stdout.write('ready\\n');`;
    const child = spawn(process.execPath, ['--input-type=module', '-e', source, JSON.stringify(f.options)], { stdio: ['ignore', 'pipe', 'pipe'] });
    const closed = once(child, 'close');
    t.after(async () => { if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL'); await closed; });
    child.stderr.resume();
    const ready = deferred(), effect = deferred(); let stdout = '';
    child.stdout.on('data', data => { stdout += data; if (stdout.includes('ready\n')) ready.resolve(); if (stdout.includes('effect\n')) effect.resolve(); });
    await ready.promise;
    // A second process cannot take ownership even before the first claim.
    await assert.rejects(openCampaignDispatch(f.options), { code: 'EEXIST' });
    let request;
    if (point !== 'before-claim') {
      request = post(f.options.socket, f.request(0)).catch(() => undefined);
      await effect.promise;
      if (point === 'after-receipt') assert.equal((await request).status, 200);
    }
    child.kill('SIGKILL'); await closed; await request;
    const audit = await inspectCampaignDispatch(f.options.directory);
    assert.deepEqual(audit.entries.map(e => e.status), point === 'before-claim' ? [] : [point === 'during-effect' ? 'uncertain' : 'completed']);
    await assert.rejects(openCampaignDispatch(f.options), { code: 'EEXIST' });
  });
}
