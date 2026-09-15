// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';

const snapshot = process.env.MEMORY_R3_SNAPSHOT_BIN;
const memory = process.env.MEMORY_R3_CLI_BIN;
const database = process.env.MEMORY_LAYER_TEST_DATABASE_URL;
if (process.env.MEMORY_R3_REQUIRE_SNAPSHOT === '1' && (!snapshot || !memory || !database)) throw Error('required snapshot/service configuration missing');

test('snapshot owner rejects arbitrary operations and unsafe source identifiers before database access', { skip: !snapshot }, () => {
  for (const input of [
    { operation: 'drop', database: 'postgres' },
    { operation: 'prepare', semantic_time: '2026-01-01T00:00:00Z', documents: [
      { project: 'synthetic', source: '/host/secret', text: 'NEVER_ECHO_SYNTHETIC_CONTENT' },
    ] },
    { operation: 'prepare', semantic_time: '2026-01-01T00:00:00Z', documents: [
      { project: 'synthetic', source: '../private/labels.json', text: 'NEVER_ECHO_SYNTHETIC_CONTENT' },
    ] },
  ]) {
    const result = spawnSync(snapshot, [], { env: { PATH: '/usr/bin:/bin', MEMORY_LAYER_TEST_REQUIRE_DB: '1' },
      input: JSON.stringify(input) + '\n', encoding: 'utf8', timeout: 5000 });
    assert.equal(result.status, 1);
    assert.equal(result.stdout, '');
    assert(!result.stderr.includes('NEVER_ECHO_SYNTHETIC_CONTENT'));
    assert(!result.stderr.includes('/host/secret'));
  }
});

test('snapshot helper owns sealed resets and a real foreground Memory service', { skip: !snapshot || !memory || !database }, async t => {
  assert(path.isAbsolute(snapshot) && path.isAbsolute(memory));
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-snapshot-'));
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: root, MEMORY_LAYER_PROFILE: 'prod',
    MEMORY_LAYER_TEST_DATABASE_URL: database, MEMORY_LAYER_TEST_REQUIRE_DB: '1' };
  const owner = spawn(snapshot, [], { env, cwd: root, stdio: ['pipe', 'pipe', 'pipe'] });
  let pending;
  let ownerError = '';
  owner.stderr.on('data', bytes => { ownerError = (ownerError + bytes).slice(-2048); });
  const lines = createInterface({ input: owner.stdout });
  lines.on('line', line => { const callback = pending; pending = undefined; callback?.resolve(JSON.parse(line)); });
  owner.on('exit', () => { pending?.reject(Error('snapshot helper exited: ' + ownerError)); pending = undefined; });
  const request = async value => {
    assert(!pending);
    let timer;
    try {
      return await new Promise((resolve, reject) => {
        pending = { resolve, reject };
        timer = setTimeout(() => reject(Error('snapshot request timed out')), 30000);
        owner.stdin.write(JSON.stringify(value) + '\n');
      });
    } finally { clearTimeout(timer); }
  };
  let service;
  const stopService = async () => {
    if (!service || service.exitCode !== null || service.signalCode !== null) return;
    const stopped = once(service, 'exit');
    service.kill('SIGINT');
    const timeout = setTimeout(() => service.kill('SIGKILL'), 5000);
    await stopped;
    clearTimeout(timeout);
  };
  t.after(async () => {
    await stopService();
    if (owner.exitCode === null && owner.signalCode === null) {
      const stopped = once(owner, 'exit');
      owner.stdin.end(JSON.stringify({ operation: 'shutdown' }) + '\n');
      const timeout = setTimeout(() => owner.kill('SIGKILL'), 10000);
      await stopped;
      clearTimeout(timeout);
    }
    lines.close();
    await fs.rm(root, { recursive: true });
  });
  const semanticTime = '2026-01-01T00:00:00Z';
  const prepared = await request({ operation: 'prepare', semantic_time: semanticTime, documents: [
    { project: 'r3-alpha', source: 'public/alpha.md', text: 'Reconcile settlement acknowledgement before retry. ALPHA_ONLY' },
    { project: 'r3-beta', source: 'public/beta.md', text: 'Quarantine conflicting settlement attempts. BETA_ONLY' },
  ] });
  assert(prepared.prepared && !prepared.official_evidence);
  assert.equal(prepared.bindings.length, 2);
  const first = await request({ operation: 'fork' });
  const second = await request({ operation: 'fork' });
  assert.notEqual(first.id, second.id);
  assert.notEqual(first.database_url, second.database_url);
  assert.equal(first.snapshot_sha256, prepared.snapshot_sha256);
  assert.equal(second.snapshot_sha256, prepared.snapshot_sha256);
  const listener = net.createServer();
  listener.listen(0, '127.0.0.1');
  await once(listener, 'listening');
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  const config = path.join(root, 'service.toml');
  const quote = JSON.stringify;
  await fs.writeFile(config, `[service]\nbind_addr = "127.0.0.1:${port}"\napi_token = "unused-synthetic"\nread_only = true\n` +
    `[database]\nurl = ${quote(first.database_url)}\n[evaluation]\nsemantic_time = ${quote(semanticTime)}\n` +
    `[auth]\nmode = "multi_user"\npublic_base_url = "http://127.0.0.1:${port}"\n` +
    `[auth.oidc]\nissuer_url = "https://unused.example.test"\nclient_id = "synthetic-unused"\n` +
    `[offline]\nenabled = false\n[cluster]\nenabled = false\n[mcp]\nenabled = false\n`, { mode: 0o600 });
  // No inherited development config, tokens, plugins or provider credentials.
  service = spawn(memory, ['--config', config, 'service', 'run'], {
    cwd: root, env: { PATH: env.PATH, HOME: root, MEMORY_LAYER_PROFILE: 'prod' }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  service.stdout.resume(); service.stderr.resume();
  const url = `http://127.0.0.1:${port}`;
  let ready = false;
  for (let attempt = 0; attempt < 100 && service.exitCode === null; attempt++) {
    try { ready = (await fetch(url + '/healthz', { signal: AbortSignal.timeout(500) })).ok; } catch { /* startup */ }
    if (ready) break;
    await delay(100);
  }
  assert(ready, 'foreground sealed service must start');
  const headers = { Authorization: `Bearer ${first.project_tokens['r3-alpha']}`, 'Content-Type': 'application/json' };
  const result = await fetch(url + '/v1/query', { method: 'POST', headers,
    body: JSON.stringify({ project: 'r3-alpha', query: 'settlement acknowledgement', top_k: 5, answer_mode: 'deterministic' }),
    signal: AbortSignal.timeout(10000) });
  assert.equal(result.status, 200);
  const answer = await result.json();
  assert.equal(answer.results[0].memory_id, prepared.bindings.find(b => b.project === 'r3-alpha').memory_id);
  assert(!JSON.stringify(answer).includes('BETA_ONLY'));
  const denied = await fetch(url + '/v1/query', { method: 'POST', headers,
    body: JSON.stringify({ project: 'r3-beta', query: 'settlement' }), signal: AbortSignal.timeout(10000) });
  assert.equal(denied.status, 403);
  assert.equal((await request({ operation: 'fingerprint', id: first.id })).snapshot_sha256, prepared.snapshot_sha256);
  assert.equal((await request({ operation: 'fingerprint', id: second.id })).snapshot_sha256, prepared.snapshot_sha256);
  await stopService();
  await request({ operation: 'release', id: first.id });
  const reset = await request({ operation: 'fork' });
  assert.equal(reset.snapshot_sha256, prepared.snapshot_sha256);
  assert.notEqual(reset.database_url, first.database_url);
  await request({ operation: 'release', id: second.id });
  await request({ operation: 'release', id: reset.id });
  const exited = once(owner, 'exit');
  owner.stdin.end(JSON.stringify({ operation: 'shutdown' }) + '\n');
  assert.equal((await exited)[0], 0, 'owned databases and roles must clean up');
});
