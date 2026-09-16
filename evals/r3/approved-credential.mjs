// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-owner primitive, not a paid launcher. Check the Rust approval gate
// before reading one explicitly selected OpenBao KV version. Never log secrets,
// consult provider environment variables, write a key file or invoke a model.
import { execFile } from 'node:child_process';
import path from 'node:path';

const absolute = value => typeof value === 'string' && path.isAbsolute(value) && !value.includes('\0');
const segment = value => typeof value === 'string' && /^[a-zA-Z0-9_][a-zA-Z0-9_-]{0,127}$/.test(value);

function capture(binary, args, env, maxBuffer) {
  return new Promise((resolve, reject) => {
    execFile(binary, args, { env, timeout: 20000, killSignal: 'SIGKILL', maxBuffer, encoding: 'utf8' },
      (error, stdout) => {
        // execFile errors contain stdout/stderr. Never propagate them or their cause.
        if (error) reject(Error('evaluation credential preflight failed; inspect approvals or secret-store access locally'));
        else resolve(stdout);
      });
  });
}

export async function withApprovedEvaluationKey(options, use) {
  const { memoryBinary, baoBinary, manifest, approvals, split, manifestSha256, secret } = structuredClone(options);
  if (![memoryBinary, baoBinary, manifest].every(absolute)
      || !Array.isArray(approvals) || approvals.length < 2 || approvals.length > 8 || !approvals.every(absolute)
      || !['development', 'held-out'].includes(split) || !/^[a-f0-9]{64}$/.test(manifestSha256)
      || typeof use !== 'function' || !secret || Object.keys(secret).sort().join(',') !== 'address,field,mount,path,version'
      || !segment(secret.mount) || !segment(secret.field) || typeof secret.path !== 'string'
      || secret.path.length > 512 || !secret.path.split('/').every(segment)
      || !Number.isSafeInteger(secret.version) || secret.version < 1) throw Error('invalid evaluation credential contract');
  let address;
  try { address = new URL(secret.address); } catch { throw Error('invalid secret-store address'); }
  if (address.protocol !== 'https:' || address.username || address.password || address.search || address.hash
      || address.pathname !== '/' || !address.hostname) throw Error('invalid secret-store address');
  // Only the existing user's home is preserved for the Bao token helper. Do not
  // inherit provider credentials, proxy overrides, alternate endpoints or TLS
  // bypass settings. The explicit Bao binary uses the operating-system CA store.
  const env = { PATH: '/usr/bin:/bin', ...(process.env.HOME ? { HOME: process.env.HOME } : {}) };
  const check = async () => {
    const output = await capture(memoryBinary, ['eval', 'campaign', 'check-approvals', '--manifest', manifest,
      '--split', split, ...approvals.flatMap(file => ['--approval', file])], env, 16384);
    let report;
    try { report = JSON.parse(output); } catch { throw Error('invalid approval preflight response'); }
    if (report.version !== 1 || report.manifest_sha256 !== manifestSha256
        || report.split !== (split === 'held-out' ? 'held_out' : split)
        || report.approvals_valid !== true || report.paid_execution_enabled !== false
        || !Number.isSafeInteger(report.work_items) || report.work_items < 1
        || !report.limits || !['requests', 'tokens', 'microusd', 'task_seconds']
          .every(key => Number.isSafeInteger(report.limits[key]) && report.limits[key] > 0)) {
      throw Error('approval preflight identity mismatch');
    }
    return report;
  };
  const before = await check();
  let key = await capture(baoBinary, ['kv', 'get', `-address=${address.origin}`, '-tls-skip-verify=false',
    `-mount=${secret.mount}`, `-version=${secret.version}`, `-field=${secret.field}`, secret.path],
  { ...env, BAO_SKIP_VERIFY: 'false', VAULT_SKIP_VERIFY: 'false', BAO_DISABLE_REDIRECTS: 'true' }, 4096);
  try {
    key = key.replace(/\r?\n$/, '');
    if (!/^[A-Za-z0-9._-]{1,512}$/.test(key)) throw Error('invalid evaluation credential value');
    const after = await check();
    if (JSON.stringify(before) !== JSON.stringify(after)) throw Error('changed evaluation approval preflight');
    // The callback belongs only to the trusted host owner, never to an agent or
    // driver loaded from benchmark material. Its return value is not exposed.
    try { await use(key); } catch { throw Error('evaluation credential consumer failed; no automatic retry'); }
    return { manifest_sha256: manifestSha256, credential_used: true };
  } finally {
    // Drop this reference; JavaScript cannot promise zeroization of string copies.
    key = undefined;
  }
}
