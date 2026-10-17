// SPDX-License-Identifier: AGPL-3.0-or-later
// Real loopback protocol, portable fake journal. Not native Windows ACL evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import { openCampaignDispatch, inspectCampaignDispatch } from './campaign-dispatch.mjs';
import { requestWindowsDispatch } from './windows-dispatch-client.mjs';

async function fixture(use, execute) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'native-dispatch-'));
  await fs.mkdir(path.join(root, 'private'), { mode: 0o700 });
  const journal = { create: directory => fs.mkdir(directory, { mode: 0o700 }),
    write: (directory, name, value) => fs.writeFile(path.join(directory, name), JSON.stringify(value), { flag: 'wx', mode: 0o600 }),
    read: async (directory, name) => JSON.parse(await fs.readFile(path.join(directory, name))) };
  const directory = path.join(root, 'private/dispatch'), endpointPath = path.join(root, 'private/dispatch-endpoint.json');
  const manifestSha256 = 'a'.repeat(64), configurationSha256 = 'b'.repeat(64);
  const work = { case_id: 'synthetic', condition: 'full_memory', repeat: 0 };
  let calls = 0;
  const dispatcher = await openCampaignDispatch({ root, directory, manifestSha256, work: [work], offline: true, preparation: false,
    native: { journal, endpointPath, configurationSha256 }, verify: async () => {}, execute: async (...args) => {
      await fs.stat(path.join(directory, 'work-00000.attempt.json')); calls++;
      return execute ? execute(...args) : { version: 1, synthetic: 'private answer' };
    } });
  const endpoint = JSON.parse(await fs.readFile(endpointPath));
  const input = Buffer.from(JSON.stringify({ version: 1, offline: true, maximum_microusd: 0, work,
    manifest: path.join(root, 'campaign.json'), manifest_sha256: manifestSha256,
    response: path.join(root, 'run/work-00000/observation.json') }));
  try { await use({ dispatcher, root, directory, endpoint, input, journal,
    expected: { manifestSha256, configurationSha256 }, calls: () => calls }); }
  finally { await dispatcher.close(); await fs.rm(root, { recursive: true, force: true }); }
}
test('mutually authenticated loopback dispatch persists once and replays with fresh transport nonces', async () => fixture(async f => {
  const request = () => requestWindowsDispatch({ ...f, timeoutMs: 2000 });
  assert.equal(JSON.parse(await request()).synthetic, 'private answer');
  assert.equal(JSON.parse(await request()).synthetic, 'private answer');
  assert.equal(f.calls(), 1);
  const audit = await inspectCampaignDispatch(f.directory, { journal: f.journal });
  assert.equal(audit.entries[0].status, 'completed');
  assert(!JSON.stringify(audit).includes('private answer'));
  await assert.rejects(requestWindowsDispatch({ ...f, input: Buffer.from(f.input.toString().replace('synthetic', 'changed')), timeoutMs: 2000 }));
  assert.equal(f.calls(), 1);
}));
test('a real replacement listener on the stale port cannot read the key or forge an accepted response', async () => fixture(async f => {
  await f.dispatcher.close();
  let captured;
  const replacement = http.createServer(async (request, response) => {
    const chunks = []; for await (const bytes of request) chunks.push(bytes);
    captured = { headers: request.headers, input: Buffer.concat(chunks).toString() };
    response.writeHead(200, { 'Content-Type': 'application/json' }); response.end('{"forged":true}');
  });
  replacement.listen(f.endpoint.port, '127.0.0.1'); await once(replacement, 'listening');
  try {
    await assert.rejects(requestWindowsDispatch({ ...f, timeoutMs: 2000 }), /failed or uncertain/);
    assert(captured); assert(!JSON.stringify(captured).includes(f.endpoint.key));
    assert.equal(f.calls(), 0);
  } finally { await new Promise(resolve => replacement.close(resolve)); }
}));
test('unauthenticated callers cannot execute and shutdown preserves uncertain work without replay', async () => {
  let started, release;
  const active = new Promise(resolve => { started = resolve; });
  const pending = new Promise(resolve => { release = resolve; });
  await fixture(async f => {
    const status = await new Promise((resolve, reject) => {
      const request = http.request({ host: '127.0.0.1', port: f.endpoint.port, path: '/dispatch', method: 'POST' }, response => {
        response.resume(); response.on('end', () => resolve(response.statusCode));
      }); request.on('error', reject); request.end(f.input);
    });
    assert.equal(status, 409); assert.equal(f.calls(), 0);
    const request = requestWindowsDispatch({ ...f, timeoutMs: 2000 });
    const rejected = assert.rejects(request, /failed or uncertain/);
    await active; const closed = f.dispatcher.close(); release({ version: 1 });
    await closed; await rejected;
    const audit = await inspectCampaignDispatch(f.directory, { journal: f.journal });
    assert.equal(audit.entries[0].status, 'uncertain'); assert.equal(f.calls(), 1);
  }, async () => { started(); return pending; });
});
