// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host orchestration only. Never expose this object, its processes or
// temporary configuration directory to an answering/coding container.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { createHash } from 'node:crypto';
import { checkedVector, openEmbeddingProxy } from './embedding-proxy.mjs';

function childProcess(binary, args, options) {
  const child = spawn(binary, args, options);
  const closed = new Promise(resolve => {
    child.once('error', () => resolve({ code: null, signal: 'spawn-error' }));
    child.once('close', (code, signal) => resolve({ code, signal }));
  });
  child.stderr.resume(); // Provider, database and configuration details stay out of errors.
  return { child, closed };
}

async function stopService(process) {
  if (!process) return;
  const { child, closed } = process;
  if (child.exitCode === null && child.signalCode === null) child.kill('SIGINT');
  const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
  try { await closed; } finally { clearTimeout(timer); }
}

async function port() {
  const server = net.createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const value = server.address().port;
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  return value;
}

export async function openPreparedSession(options) {
  const { embedding, ...plainOptions } = options;
  const { snapshotBinary, memoryBinary, databaseUrl, semanticTime, documents } = structuredClone(plainOptions);
  if (!path.isAbsolute(snapshotBinary) || !path.isAbsolute(memoryBinary)
      || typeof databaseUrl !== 'string' || !databaseUrl.startsWith('postgres')
      || !Array.isArray(documents) || documents.length < 1) throw Error('invalid disposable service contract');
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-service-session-'));
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, MEMORY_LAYER_PROFILE: 'prod' };
  const owner = childProcess(snapshotBinary, [], { cwd: directory,
    env: { ...env, MEMORY_LAYER_TEST_DATABASE_URL: databaseUrl, MEMORY_LAYER_TEST_REQUIRE_DB: '1' },
    stdio: ['pipe', 'pipe', 'pipe'] });
  let pending, unavailable = false, closing = false, active = false, proxy;
  const lines = createInterface({ input: owner.child.stdout });
  const fail = () => {
    unavailable = true;
    pending?.reject(Error('disposable snapshot protocol failed'));
    pending = undefined;
  };
  owner.child.stdin.on('error', fail);
  owner.closed.then(fail);
  lines.on('line', line => {
    try {
      if (!pending || Buffer.byteLength(line) > 2 * 1024 * 1024) throw Error('unexpected response');
      const result = JSON.parse(line);
      if (result.version !== 1) throw Error('unexpected protocol');
      const callback = pending;
      pending = undefined;
      callback.resolve(result);
    } catch { fail(); }
  });
  const request = async value => {
    if (unavailable || closing || pending) throw Error('disposable snapshot session unavailable or busy');
    let timer;
    try {
      return await new Promise((resolve, reject) => {
        pending = { resolve, reject };
        timer = setTimeout(fail, 30000);
        owner.child.stdin.write(JSON.stringify(value) + '\n');
      });
    } finally { clearTimeout(timer); }
  };
  const close = async () => {
    if (active) throw Error('release active service before closing snapshot session');
    if (!closing) {
      closing = true;
      if (!owner.child.stdin.destroyed) owner.child.stdin.end(JSON.stringify({ operation: 'shutdown' }) + '\n');
    }
    const timer = setTimeout(() => owner.child.kill('SIGKILL'), 10000);
    let outcome;
    try { outcome = await owner.closed; } finally { clearTimeout(timer); lines.close(); await proxy?.close(); }
    if (outcome.code !== 0) throw Error(`reconcile disposable snapshot session; private staging retained at ${directory}`);
    await fs.rm(directory, { recursive: true });
  };
  let prepared;
  try {
    if (embedding) {
      proxy = await openEmbeddingProxy(embedding);
      const plan = await request({ operation: 'plan_chunks', documents });
      if (plan.chunking !== 'memory-split-search-chunks' || plan.official_evidence !== false
          || plan.documents?.length !== documents.length) throw Error('invalid chunk plan');
      for (let i = 0; i < documents.length; i++) {
        const d = documents[i], p = plan.documents[i];
        if (p.project !== d.project || p.source !== d.source || p.source_sha256 !== createHash('sha256').update(d.text).digest('hex')
            || !Array.isArray(p.chunks) || !p.chunks.length || p.chunks.some(c => typeof c !== 'string' || !c)) throw Error('invalid chunk binding');
        d.vectors = [];
        for (const chunk of p.chunks) d.vectors.push(checkedVector(await embedding.embed(chunk, 'document'), proxy.dimensions));
      }
    }
    prepared = await request({ operation: 'prepare', semantic_time: semanticTime, documents,
      ...(proxy ? { embedding: { model: proxy.model, dimensions: proxy.dimensions, base_url: proxy.baseUrl } } : {}) });
    if (!prepared.prepared || prepared.official_evidence !== false || !/^[a-f0-9]{64}$/.test(prepared.snapshot_sha256)) throw Error('invalid snapshot preparation');
  } catch {
    await close();
    throw Error('disposable snapshot preparation failed');
  }
  return {
    // Supplied vectors prove plumbing only, not provider provenance, preparation
    // cost or full production-ranking parity. Those are separate campaign gates.
    preparation: structuredClone(prepared),
    close,
    async withInstance(project, use, { queryEmbedding } = {}) {
      if (active || closing || unavailable) throw Error('disposable snapshot session unavailable or busy');
      if (typeof use !== 'function' || !prepared.bindings.some(binding => binding.project === project)) throw Error('unknown prepared project');
      if (queryEmbedding !== undefined && (!proxy || typeof queryEmbedding !== 'function')) throw Error('invalid task embedding callback');
      active = true;
      let reader, service, config;
      try {
        reader = await request({ operation: 'fork' });
        if (reader.snapshot_sha256 !== prepared.snapshot_sha256 || !reader.project_tokens?.[project]
            || !/^[a-f0-9-]{36}$/.test(reader.id)) throw Error('invalid disposable clone');
        const bindPort = await port();
        config = path.join(directory, `${reader.id}.toml`);
        await fs.writeFile(config, `[service]\nbind_addr = "127.0.0.1:${bindPort}"\napi_token = "unused-synthetic"\nread_only = true\n` +
          `[database]\nurl = ${JSON.stringify(reader.database_url)}\n[evaluation]\nsemantic_time = ${JSON.stringify(semanticTime)}\n` +
          `[auth]\nmode = "multi_user"\npublic_base_url = "http://127.0.0.1:${bindPort}"\n` +
          `[auth.oidc]\nissuer_url = "https://unused.example.test"\nclient_id = "synthetic-unused"\n` +
          `[offline]\nenabled = false\n[cluster]\nenabled = false\n[mcp]\nenabled = false\n` +
          (proxy ? `[embeddings]\nenabled = true\ncreate_enabled = false\nactive = "r3-supplied"\n` +
            `[[embeddings.backends]]\nname = "r3-supplied"\nprovider = "openai_compatible"\ncreate_enabled = false\n` +
            `model = ${JSON.stringify(proxy.model)}\ndimensions = ${proxy.dimensions}\nbase_url = ${JSON.stringify(proxy.baseUrl)}\napi_key_env = "R3_SUPPLIED_VECTOR_TOKEN"\n` : ''), { flag: 'wx', mode: 0o600 });
        service = childProcess(memoryBinary, ['--config', config, 'service', 'run'], {
          cwd: directory, env: proxy ? { ...env, R3_SUPPLIED_VECTOR_TOKEN: proxy.token } : env, stdio: ['ignore', 'pipe', 'pipe'] });
        service.child.stdout.resume();
        const url = `http://127.0.0.1:${bindPort}`;
        let ready = false;
        for (let attempt = 0; attempt < 100 && service.child.exitCode === null && service.child.signalCode === null; attempt++) {
          try { ready = (await fetch(url + '/healthz', { redirect: 'error', signal: AbortSignal.timeout(500) })).ok; } catch { /* startup */ }
          if (ready) break;
          await delay(100);
        }
        if (!ready) throw Error('disposable service did not start');
        const fingerprint = async () => {
          const current = await request({ operation: 'fingerprint', id: reader.id });
          if (current.snapshot_sha256 !== prepared.snapshot_sha256) throw Error('disposable snapshot changed');
          return current.snapshot_sha256;
        };
        const before = await fingerprint();
        let accepting = true;
        let value, after;
        try {
          value = await use({
            instanceId: reader.id,
            snapshotSha256: before,
            bindings: structuredClone(prepared.bindings.filter(binding => binding.project === project)),
            async query(question, topK = 8) {
              if (!accepting || typeof question !== 'string' || !question.trim() || Buffer.byteLength(question) > 4096
                  || !Number.isInteger(topK) || topK < 1 || topK > 8) throw Error('invalid scoped evaluation query');
              try {
                const execute = async () => {
                const response = await fetch(url + '/v1/query', { method: 'POST', redirect: 'error',
                  headers: { Authorization: `Bearer ${reader.project_tokens[project]}`, 'Content-Type': 'application/json' },
                  body: JSON.stringify({ project, query: question, top_k: topK, answer_mode: 'deterministic' }),
                  signal: AbortSignal.timeout(10000) });
                if (!response.ok) throw Error('query failed');
                let bytes = 0;
                const chunks = [];
                for await (const chunk of response.body) {
                  if ((bytes += chunk.length) > 4 * 1024 * 1024) throw Error('oversized response');
                  chunks.push(chunk);
                }
                return JSON.parse(Buffer.concat(chunks).toString('utf8'));
                };
                return await (proxy ? proxy.query(question, execute, queryEmbedding) : execute());
              } catch { throw Error('scoped disposable query failed'); }
            },
          });
        } finally {
          accepting = false;
          after = await fingerprint();
        }
        return { value, beforeSha256: before, afterSha256: after, instanceId: reader.id };
      } finally {
        try {
          await stopService(service);
          if (reader) await request({ operation: 'release', id: reader.id });
          if (config) await fs.rm(config);
        } finally { active = false; }
      }
    },
  };
}
