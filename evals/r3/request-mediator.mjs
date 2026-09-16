// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host per-call boundary backed by mem-eval's durable budget/result
// journal. Do not mount this process, directory or configuration in an agent.
// This does not supply paid-run approval, a tokenizer, prices or network access.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { createHash, randomUUID } from 'node:crypto';
import { providerDiagnostic, providerFailure } from './provider-diagnostics.mjs';

function counters(value, keys) {
  return value && Object.keys(value).sort().join(',') === [...keys].sort().join(',')
    && keys.every(key => Number.isSafeInteger(value[key]) && value[key] >= 0);
}

export async function openRequestMediator(options) {
  const { binary, directory, manifestSha256, limits, taskLimits, resume = false } = structuredClone(options);
  if (!path.isAbsolute(binary) || !path.isAbsolute(directory) || !/^[a-f0-9]{64}$/.test(manifestSha256)
      || ![limits, taskLimits].every(v => counters(v, ['requests', 'tokens', 'microusd', 'task_seconds']))) throw Error('invalid private mediator contract');
  const metadata = await fs.lstat(directory);
  if (!metadata.isDirectory() || metadata.isSymbolicLink() || (metadata.mode & 0o077)) throw Error('private mediator directory required');
  const config = path.join(directory, `session-${randomUUID()}.json`);
  await fs.writeFile(config, JSON.stringify({ version: 1, manifest_sha256: manifestSha256,
    limits, task_limits: taskLimits, resume }), { flag: 'wx', mode: 0o600 });
  const child = spawn(binary, [config, directory], { cwd: directory,
    env: { PATH: '/usr/bin:/bin', HOME: directory }, stdio: ['pipe', 'pipe', 'pipe'] });
  child.stderr.resume();
  let pending, unavailable = false, busy = false, closing = false;
  const fail = () => { unavailable = true; pending?.reject(Error('private budget session unavailable')); pending = undefined; };
  const closed = new Promise(resolve => {
    child.once('error', () => { fail(); resolve(null); });
    child.once('close', code => { fail(); resolve(code); });
  });
  child.stdin.on('error', fail);
  const lines = createInterface({ input: child.stdout });
  lines.on('line', line => {
    try {
      if (!pending || Buffer.byteLength(line) > 16 * 1024 * 1024) throw Error('invalid frame');
      const reply = JSON.parse(line), callback = pending;
      if (reply.version !== 1 || typeof reply.ok !== 'boolean') throw Error('invalid reply');
      pending = undefined;
      if (reply.ok) callback.resolve(reply.result);
      else callback.reject(Error('budget or receipt rejected; inspect private journal'));
    } catch { fail(); }
  });
  const request = async value => {
    if (unavailable || closing || pending) throw Error('private budget session unavailable');
    const bytes = JSON.stringify(value) + '\n';
    if (Buffer.byteLength(bytes) > 2 * 1024 * 1024) throw Error('private budget request too large');
    let timer;
    try {
      return await new Promise((resolve, reject) => {
        pending = { resolve, reject };
        timer = setTimeout(fail, 10000);
        child.stdin.write(bytes);
      });
    } finally { clearTimeout(timer); }
  };
  const close = async () => {
    if (busy) throw Error('wait for active request before closing mediator');
    if (!closing) {
      closing = true;
      if (!child.stdin.destroyed) child.stdin.end('{"operation":"shutdown"}\n');
    }
    const timer = setTimeout(() => child.kill('SIGKILL'), 10000);
    let code;
    try { code = await closed; } finally { clearTimeout(timer); lines.close(); }
    await fs.rm(config);
    if (code !== 0) throw Error('private budget session stopped; retain journal for reconciliation');
  };
  try { await request({ operation: 'status' }); } catch { await close(); throw Error('private budget initialization failed'); }
  return {
    status: () => request({ operation: 'status' }),
    audit: () => request({ operation: 'audit' }),
    // Pure arithmetic, not a reservation or approval. The trusted caller must
    // supply its manifest-locked model; no rate is fetched or inferred here.
    price: ({ model, inputTokens, outputTokens, cachedInputTokens }) => request({ operation: 'price', model,
      input_tokens: inputTokens, output_tokens: outputTokens, ...(cachedInputTokens === undefined ? {} : { cached_input_tokens: cachedInputTokens }) }),
    // Recovery is deliberately separate from dispatch; never infer a free retry
    // from a crash, timeout or missing usage. A receipt must already exist.
    recover: requestId => request({ operation: 'recover', request_id: requestId }),
    close,
    async dispatch({ taskId, requestId, payload, allowance, timeoutMs, invoke, withReceipt = false, signal }) {
      if (busy || typeof invoke !== 'function' || !Number.isInteger(timeoutMs) || timeoutMs < 1
          || timeoutMs > Math.min(600000, taskLimits.task_seconds * 1000)
          || !counters(allowance, ['input_tokens', 'output_tokens', 'microusd'])) throw Error('invalid or busy provider dispatch');
      if (signal !== undefined && !(signal instanceof AbortSignal)) throw Error('invalid provider cancellation signal');
      if (signal?.aborted) throw Error('provider request cancelled before reservation');
      busy = true;
      let timer;
      const abort = new AbortController();
      const cancel = () => abort.abort();
      signal?.addEventListener('abort', cancel, { once: true });
      try {
        // Bind the exact trusted model/purpose/input contract, not an agent's
        // claimed usage. Callers derive allowance from locked tokenizer/prices.
        const bytes = JSON.stringify(payload);
        if (!bytes || Buffer.byteLength(bytes) > 1024 * 1024) throw Error('invalid provider payload');
        const frozenPayload = JSON.parse(bytes);
        const identity = { task_id: taskId, request_id: requestId,
          payload_sha256: createHash('sha256').update(bytes).digest('hex') };
        const decision = await request({ operation: 'begin', ...identity, allowance });
        if (decision.state === 'cached') {
          if (!counters(decision.usage, ['input_tokens', 'output_tokens', 'microusd'])) throw Error('invalid cached usage');
          return withReceipt ? { output: decision.output, usage: decision.usage, invoked: false } : decision.output;
        }
        if (decision.state !== 'reserved') throw Error('invalid reservation');
        let receipt;
        try {
          if (abort.signal.aborted) throw Error('cancelled after reservation');
          receipt = await Promise.race([
            Promise.resolve().then(() => {
              if (abort.signal.aborted) throw Error('cancelled before invocation');
              return invoke(frozenPayload, abort.signal);
            }),
            new Promise((_, reject) => { timer = setTimeout(() => {
              reject(providerFailure('deadline')); abort.abort();
            }, timeoutMs);
            abort.signal.addEventListener('abort', () => reject(providerFailure('cancelled')), { once: true }); }),
          ]);
          if (abort.signal.aborted) throw Error('cancelled provider outcome');
          if (!receipt || !Object.hasOwn(receipt, 'output')
              || !counters(receipt.usage, ['input_tokens', 'output_tokens', 'microusd'])) throw Error('missing provider receipt');
          const result = await request({ operation: 'complete', ...identity, output: receipt.output, usage: receipt.usage });
          if (!counters(result.usage, ['input_tokens', 'output_tokens', 'microusd'])) throw Error('invalid confirmed usage');
          return withReceipt ? { output: result.output, usage: result.usage, invoked: true } : result.output;
        } catch (error) {
          // Diagnostic evidence never settles/refunds an uncertain reservation.
          // Use the trusted payload digest, not an arbitrary request ID as path.
          const failure = await fs.open(path.join(directory, `failure-${identity.payload_sha256}.json`), 'wx', 0o600);
          try {
            await failure.writeFile(JSON.stringify({ version: 1, manifest_sha256: manifestSha256,
              payload_sha256: identity.payload_sha256, ...providerDiagnostic(error) }) + '\n');
            await failure.sync();
          } finally { await failure.close(); }
          throw Error('provider outcome uncertain; retain journal and reconcile before further calls');
        }
      } finally {
        clearTimeout(timer);
        signal?.removeEventListener('abort', cancel);
        abort.abort();
        busy = false;
      }
    },
  };
}
