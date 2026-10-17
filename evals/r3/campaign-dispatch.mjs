// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host request/reply boundary. The Rust campaign runner remains the
// budget/evidence authority. Never mount this socket or journal in an agent.
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { authenticateWindowsDispatchRequest, signWindowsDispatchResponse, validateWindowsDispatchEndpoint } from './windows-dispatch-auth.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const key = work => JSON.stringify([work.case_id, work.condition, work.repeat]);
const validWork = work => work && Object.keys(work).sort().join(',') === 'case_id,condition,repeat'
  && typeof work.case_id === 'string' && work.case_id.length > 0 && work.case_id.length <= 256
  && typeof work.condition === 'string' && work.condition.length > 0 && work.condition.length <= 64
  && Number.isSafeInteger(work.repeat) && work.repeat >= 0;
const maximum = 16 * 1024 * 1024;
async function syncDirectory(directory) {
  const handle = await fs.open(directory, 'r');
  try { await handle.sync(); } finally { await handle.close(); }
}
async function exclusiveJson(directory, name, value) {
  const bytes = JSON.stringify(value);
  if (Buffer.byteLength(bytes) > maximum) throw Error('oversized dispatch evidence');
  const file = await fs.open(path.join(directory, name), 'wx', 0o600);
  try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
  await syncDirectory(directory);
}

export async function openCampaignDispatch(options) {
  const { prepare, execute, verify, onProgress, native, ...plain } = options;
  const { root, directory, socket, manifestSha256, work, offline, preparation } = structuredClone(plain);
  if (![root, directory, ...(native ? [] : [socket])].every(p => typeof p === 'string' && path.isAbsolute(p))
      || !/^[a-f0-9]{64}$/.test(manifestSha256) || !Array.isArray(work) || !work.length || work.length > 100000
      || !work.every(validWork)
      || new Set(work.map(key)).size !== work.length || typeof offline !== 'boolean'
      || typeof preparation !== 'boolean' || typeof execute !== 'function' || typeof verify !== 'function'
      || (preparation && typeof prepare !== 'function') || (onProgress !== undefined && typeof onProgress !== 'function')) {
    throw Error('invalid campaign dispatch contract');
  }
  if (native && (!offline || preparation || socket !== undefined
      || !/^[a-f0-9]{64}$/.test(native.configurationSha256)
      || native.endpointPath !== path.join(root, 'private/dispatch-endpoint.json')
      || !['create','write','read'].every(name => typeof native.journal?.[name] === 'function'))) {
    throw Error('explicit zero-spend native dispatch contract required');
  }
  // An exclusive directory is also the cross-process fence. Never remove it on
  // close. A crashed owner is not evidence that its provider call was free.
  if (native) await native.journal.create(directory);
  else { await fs.mkdir(directory, { mode: 0o700 }); await syncDirectory(path.dirname(directory)); }
  const store = (name, value) => native ? native.journal.write(directory, name, value) : exclusiveJson(directory, name, value);
  await store('identity.json', { version: 1, manifest_sha256: manifestSha256,
    offline, work_items: work.length, preparation });
  let cursor = 0, prepared = !preparation, active, stopping = false, failed = false;
  const receipts = new Map(), lifetime = new AbortController();
  let endpoint;
  const server = http.createServer(async (request, response) => {
    let transportProof;
    response.on('error', () => {});
    try {
      if (stopping || request.method !== 'POST' || request.url !== '/dispatch') throw Error('unavailable');
      const chunks = [];
      let length = 0;
      for await (const chunk of request) {
        if ((length += chunk.length) > 32768) throw Error('oversized');
        chunks.push(chunk);
      }
      if (stopping) throw Error('stopping');
      const bytes = Buffer.concat(chunks);
      if (native) transportProof = authenticateWindowsDispatchRequest(endpoint, request.headers, bytes);
      const dispatch = JSON.parse(bytes);
      if (dispatch.version !== 1 || dispatch.offline !== offline
          || (offline && dispatch.maximum_microusd !== 0)
          || !Number.isSafeInteger(dispatch.maximum_microusd) || dispatch.maximum_microusd < 0
          || dispatch.manifest !== path.join(root, 'campaign.json')
          || dispatch.manifest_sha256 !== manifestSha256) throw Error('invalid dispatch');
      const preparing = dispatch.phase === 'preparation';
      if ((!preparing && dispatch.phase !== undefined) || (!preparing && !validWork(dispatch.work))) throw Error('invalid phase');
      const identity = preparing ? 'preparation' : key(dispatch.work), digest = hash(bytes);
      let receipt = receipts.get(identity);
      const replaying = !!receipt;
      if (receipt && receipt.digest !== digest) throw Error('changed retry');
      if (!receipt) {
        if (failed || active || (preparing ? prepared || !preparation : !prepared || cursor >= work.length || key(work[cursor]) !== identity)) {
          throw Error('out of order dispatch');
        }
        const id = preparing ? 'preparation' : `work-${String(cursor).padStart(5, '0')}`;
        if (dispatch.response !== path.join(root, 'run', id, preparing ? 'receipt.json' : 'observation.json')) throw Error('wrong response location');
        const task = (async () => {
          try {
            await verify();
            if (stopping) throw Error('stopping');
            await store(`${id}.attempt.json`, { version: 1, request_sha256: digest, id });
            if (stopping) throw Error('stopping');
            const result = preparing ? await prepare(dispatch.provider_budget, lifetime.signal)
              : await execute(dispatch.work, dispatch.provider_budget, lifetime.signal);
            await verify();
            if (stopping) throw Error('stopped before receipt');
            // Persist before reply: a broken socket cannot cause re-execution.
            await store(`${id}.receipt.json`, { version: 1, request_sha256: digest, id, result });
            if (preparing) prepared = true;
            else { cursor++; onProgress?.(cursor, work.length); }
            return result;
          } catch {
            failed = true; lifetime.abort();
            throw Error('campaign dispatch stopped; retain private evidence');
          }
        })();
        active = task;
        receipt = { digest, task };
        receipts.set(identity, receipt);
        task.then(() => { active = undefined; }, () => { active = undefined; });
      }
      const result = await receipt.task;
      if (failed) throw Error('dispatch owner retired');
      // Redelivery is still subject to current approval/input checks. A failed
      // check retires this owner, even if the asset is subsequently put back.
      try { if (replaying) await verify(); }
      catch { failed = true; lifetime.abort(); throw Error('changed dispatch authority'); }
      if (stopping) throw Error('stopped before delivery');
      const output = Buffer.from(JSON.stringify(result));
      response.writeHead(200, { 'Content-Type': 'application/json',
        ...(native ? signWindowsDispatchResponse(endpoint, transportProof, 200, output) : {}) });
      response.end(output);
    } catch {
      const output = Buffer.from('R3 dispatch refused or uncertain; no automatic repeat.');
      response.writeHead(409, { 'Content-Type': 'text/plain',
        ...(native && transportProof ? signWindowsDispatchResponse(endpoint, transportProof, 409, output) : {}) });
      response.end(output);
    }
  });
  server.requestTimeout = 30000;
  server.headersTimeout = 10000;
  try {
    await new Promise((resolve, reject) => { server.once('error', reject);
      server.listen(native ? { host: '127.0.0.1', port: 0, exclusive: true } : socket, resolve); });
    if (native) {
      endpoint = validateWindowsDispatchEndpoint({ format: 'memory-r3-windows-loopback-v1', host: '127.0.0.1',
        port: server.address().port, session_id: randomUUID(), key: randomBytes(32).toString('hex'),
        manifest_sha256: manifestSha256, configuration_sha256: native.configurationSha256 },
      { manifestSha256, configurationSha256: native.configurationSha256 });
      await native.journal.write(path.dirname(native.endpointPath), path.basename(native.endpointPath), endpoint);
    } else await fs.chmod(socket, 0o600);
  } catch (error) {
    server.closeAllConnections();
    if (server.listening) await new Promise(resolve => server.close(resolve));
    throw error;
  }
  let closing;
  return {
    get completed() { return cursor; },
    get signal() { return lifetime.signal; },
    close() {
      if (!closing) closing = (async () => {
        stopping = true; lifetime.abort();
        server.closeAllConnections();
        const closed = new Promise(resolve => server.close(resolve));
        try { await active; } catch { /* Uncertain evidence is retained. */ }
        await closed;
      })();
      return closing;
    },
  };
}

// Read-only reconciliation. This never repairs files, clears a fence, resumes a
// provider or returns answer/request payloads. Partial/corrupt writes fail closed.
export async function inspectCampaignDispatch(directory, { journal } = {}) {
  const stat = await fs.lstat(directory);
  if (!stat.isDirectory() || stat.isSymbolicLink() || (!journal && (stat.mode & 0o077))) throw Error('private dispatch directory required');
  const read = async name => {
    if (journal) return journal.read(directory, name);
    const file = path.join(directory, name), stat = await fs.lstat(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > maximum) throw Error('invalid dispatch evidence');
    return JSON.parse(await fs.readFile(file));
  };
  const identity = await read('identity.json');
  if (identity.version !== 1 || !/^[a-f0-9]{64}$/.test(identity.manifest_sha256)
      || !Number.isSafeInteger(identity.work_items) || identity.work_items < 1 || identity.work_items > 100000
      || typeof identity.preparation !== 'boolean' || typeof identity.offline !== 'boolean') throw Error('invalid dispatch identity');
  const names = await fs.readdir(directory), entries = [];
  const ids = [...(identity.preparation ? ['preparation'] : []),
    ...Array.from({ length: identity.work_items }, (_, i) => `work-${String(i).padStart(5, '0')}`)];
  const allowed = new Set(['identity.json', ...ids.flatMap(id => [`${id}.attempt.json`, `${id}.receipt.json`])]);
  if (names.some(name => !allowed.has(name))) throw Error('unknown dispatch evidence');
  let ended = false;
  for (const id of ids) {
    if (!names.includes(`${id}.attempt.json`)) {
      if (names.includes(`${id}.receipt.json`)) throw Error('receipt without attempt');
      ended = true; continue;
    }
    if (ended) throw Error('non-contiguous dispatch history');
    const attempt = await read(`${id}.attempt.json`);
    if (attempt.version !== 1 || attempt.id !== id || !/^[a-f0-9]{64}$/.test(attempt.request_sha256)) throw Error('invalid dispatch attempt');
    let status = 'uncertain';
    if (names.includes(`${id}.receipt.json`)) {
      const receipt = await read(`${id}.receipt.json`);
      if (receipt.version !== 1 || receipt.id !== id || receipt.request_sha256 !== attempt.request_sha256
          || !Object.hasOwn(receipt, 'result')) throw Error('invalid dispatch receipt');
      status = 'completed';
    } else ended = true;
    entries.push({ id, status, request_sha256: attempt.request_sha256 });
  }
  return { version: 1, manifest_sha256: identity.manifest_sha256, offline: identity.offline,
    work_items: identity.work_items, preparation: identity.preparation, entries, execution_enabled: false };
}
