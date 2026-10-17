// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host orchestration only. Never expose this object, its processes or
// temporary configuration directory to an answering/coding container.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { createHash, randomUUID } from 'node:crypto';
import { checkedVector, openEmbeddingProxy } from './embedding-proxy.mjs';
import { validateExecutionInput } from './execution.mjs';

const exec = promisify(execFile);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const invokeNativeHelper = (binary, args, env) => exec(binary, args,
  { env, timeout: 10000, maxBuffer: 1048576, windowsHide: true, shell: false });

function childProcess(spawnProcess, binary, args, options) {
  const child = spawnProcess(binary, args, { ...options, shell: false, windowsHide: true });
  const closed = new Promise(resolve => {
    child.once('error', () => resolve({ code: null, signal: 'spawn-error' }));
    child.once('close', (code, signal) => resolve({ code, signal }));
  });
  child.stderr.resume(); // Provider, database and configuration details stay out of errors.
  return { child, closed };
}

async function stopService(process, native) {
  if (!process) return;
  const { child, closed } = process;
  // Native roots inherit the independently verified outer kill-on-close Job.
  // Node's Windows SIGINT has no graceful console meaning; terminate only the
  // exact owned root. Outer native cancellation also reaps its descendants.
  if (child.exitCode === null && child.signalCode === null) child.kill(native ? 'SIGKILL' : 'SIGINT');
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

export function createPreparedSessionOpener({ fileSystem = fs, spawnProcess = spawn, invokeHelper = invokeNativeHelper,
  platform = process.platform, requestId = randomUUID, reservePort = port, fetchResponse = fetch, wait = delay,
  systemRoot = process.env.SystemRoot ?? 'C:\\Windows', callerPid = process.pid } = {}) {
  if ([spawnProcess, invokeHelper, requestId, reservePort, fetchResponse, wait].some(value => typeof value !== 'function')) throw Error('invalid trusted service-session adapter');
  return options => openSession(options, { fileSystem, spawnProcess, invokeHelper, platform, requestId,
    reservePort, fetchResponse, wait, systemRoot, callerPid });
}
export const openPreparedSession = createPreparedSessionOpener();

async function nativeDirectory(execution, binaries, deps) {
  const { fileSystem, invokeHelper, platform, requestId, systemRoot, callerPid } = deps;
  validateExecutionInput({ execution, argv: [execution.profile?.node?.path], files: {}, exportPaths: [] });
  if (platform !== 'win32') throw Error('native prepared service requires Windows; no Linux fallback');
  if (typeof systemRoot !== 'string' || !/^[A-Za-z]:\\Windows$/i.test(systemRoot)
      || !Number.isSafeInteger(callerPid) || callerPid < 1) throw Error('invalid native service host contract');
  const profile = execution.profile;
  const inventory = profile.toolchains.flatMap(tree => tree.files);
  const pins = [profile.helper];
  for (const binary of binaries) {
    validateExecutionInput({ execution, argv: [binary], files: {}, exportPaths: [] });
    const pin = inventory.find(file => file.path.toLowerCase() === binary.toLowerCase());
    if (!pin) throw Error('native service executable absent from reviewed profile');
    pins.push(pin);
  }
  for (const pin of pins) {
    const stat = await fileSystem.stat(pin.path);
    if (!stat.isFile() || stat.size < 1 || stat.size > 512 * 1024 * 1024
        || hash(await fileSystem.readFile(pin.path)) !== pin.sha256) throw Error('native service helper/executable bytes changed');
  }
  const helperEnv = { SystemRoot: systemRoot, WINDIR: systemRoot, PATH: '' };
  let job;
  try { job = JSON.parse((await invokeHelper(profile.helper.path, ['--verify-outer-job', String(callerPid)], helperEnv)).stdout); }
  catch { throw Error('native prepared service requires a verified outer Job; no payload written'); }
  if (!job || Object.keys(job).sort().join(',') !== 'breakaway_allowed,caller_pid,format,helper_pid,kill_on_close'
      || job.format !== 'memory-windows-trusted-outer-job-v1' || job.caller_pid !== callerPid
      || !Number.isSafeInteger(job.helper_pid) || job.helper_pid < 1 || job.helper_pid === callerPid
      || job.kill_on_close !== true || job.breakaway_allowed !== false) throw Error('invalid native outer Job receipt; no payload written');
  const id = requestId();
  if (typeof id !== 'string' || id === '00000000-0000-0000-0000-000000000000'
      || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id)) throw Error('invalid native service directory identity');
  const directory = path.win32.join(profile.io_root, id);
  let prepared;
  try { prepared = JSON.parse((await invokeHelper(profile.helper.path, ['--prepare-io', profile.io_root, '--request-id', id], helperEnv)).stdout); }
  catch { throw Error('native service private I/O preparation failed; no payload written'); }
  if (!prepared || prepared.format !== 'memory-windows-evaluation-io-v1' || prepared.request_id !== id
      || typeof prepared.directory !== 'string' || path.win32.normalize(prepared.directory).toLowerCase() !== directory.toLowerCase()
      || ![prepared.root_identity, prepared.directory_identity].every(value => Array.isArray(value) && value.length === 2
        && value.every(part => typeof part === 'string' && /^[0-9a-f]+$/.test(part)))) throw Error('native service private I/O receipt mismatch; no payload written');
  return directory;
}

async function openSession(options, deps) {
  const { fileSystem, spawnProcess, platform, reservePort, fetchResponse, wait, systemRoot } = deps;
  const { embedding, ...plainOptions } = options;
  const { snapshotBinary, memoryBinary, databaseUrl, semanticTime, documents, execution } = structuredClone(plainOptions);
  const native = execution?.kind === 'windows-native-v1';
  if (execution !== undefined && !native) throw Error('unknown prepared service backend; no Linux fallback');
  if (platform === 'win32' && !native) throw Error('explicit native prepared service profile required');
  const paths = native ? path.win32 : path;
  if (!paths.isAbsolute(snapshotBinary) || !paths.isAbsolute(memoryBinary)
      || typeof databaseUrl !== 'string' || !databaseUrl.startsWith('postgres')
      || !Array.isArray(documents) || documents.length < 1) throw Error('invalid disposable service contract');
  const directory = native ? await nativeDirectory(execution, [snapshotBinary, memoryBinary], deps)
    : await fileSystem.mkdtemp(path.join(os.tmpdir(), 'memory-r3-service-session-'));
  const env = native ? { SystemRoot: systemRoot, WINDIR: systemRoot, PATH: '', HOME: directory,
    USERPROFILE: directory, TEMP: directory, TMP: directory, CODIFIX_PROFILE: 'prod' }
    : { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, CODIFIX_PROFILE: 'prod' };
  const owner = childProcess(spawnProcess, snapshotBinary, [], { cwd: directory,
    env: { ...env, CODIFIX_TEST_DATABASE_URL: databaseUrl, CODIFIX_TEST_REQUIRE_DB: '1' },
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
    await fileSystem.rm(directory, { recursive: true });
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
        const bindPort = await reservePort();
        config = paths.join(directory, `${reader.id}.toml`);
        await fileSystem.writeFile(config, `[service]\nbind_addr = "127.0.0.1:${bindPort}"\napi_token = "unused-synthetic"\nread_only = true\n` +
          `[database]\nurl = ${JSON.stringify(reader.database_url)}\n[evaluation]\nsemantic_time = ${JSON.stringify(semanticTime)}\n` +
          `[auth]\nmode = "multi_user"\npublic_base_url = "http://127.0.0.1:${bindPort}"\n` +
          `[auth.oidc]\nissuer_url = "https://unused.example.test"\nclient_id = "synthetic-unused"\n` +
          `[offline]\nenabled = false\n[cluster]\nenabled = false\n[mcp]\nenabled = false\n` +
          (proxy ? `[embeddings]\nenabled = true\ncreate_enabled = false\nactive = "r3-supplied"\n` +
            `[[embeddings.backends]]\nname = "r3-supplied"\nprovider = "openai_compatible"\ncreate_enabled = false\n` +
            `model = ${JSON.stringify(proxy.model)}\ndimensions = ${proxy.dimensions}\nbase_url = ${JSON.stringify(proxy.baseUrl)}\napi_key_env = "R3_SUPPLIED_VECTOR_TOKEN"\n` : ''), { flag: 'wx', ...(native ? {} : { mode: 0o600 }) });
        service = childProcess(spawnProcess, memoryBinary, ['--config', config, 'service', 'run'], {
          cwd: directory, env: proxy ? { ...env, R3_SUPPLIED_VECTOR_TOKEN: proxy.token } : env, stdio: ['ignore', 'pipe', 'pipe'] });
        service.child.stdout.resume();
        const url = `http://127.0.0.1:${bindPort}`;
        let ready = false;
        for (let attempt = 0; attempt < 100 && service.child.exitCode === null && service.child.signalCode === null; attempt++) {
          try { ready = (await fetchResponse(url + '/healthz', { redirect: 'error', signal: AbortSignal.timeout(500) })).ok; } catch { /* startup */ }
          if (ready) break;
          await wait(100);
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
                const response = await fetchResponse(url + '/v1/query', { method: 'POST', redirect: 'error',
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
          await stopService(service, native);
          if (reader) await request({ operation: 'release', id: reader.id });
          if (config) await fileSystem.rm(config);
        } finally { active = false; }
      }
    },
  };
}
