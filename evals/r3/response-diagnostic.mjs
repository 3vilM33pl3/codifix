#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// One prepared Responses request, not a campaign, scorer or command executor.
// Reuses the existing approval, fixed-destination credential and budget owners.
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyPaidRuntime, regular, digest } from './paid-runtime.mjs';
import { checkApprovedCampaign } from './approved-credential.mjs';
import { withApprovedProvider } from './approved-provider.mjs';
import { openRequestMediator } from './request-mediator.mjs';

const configPath = 'private/response-diagnostic.json';
const payloadPath = 'private/diagnostic-payload.json';
const runtimePath = 'runtime/response-diagnostic.mjs';
const limits = { requests: 1, tokens: 416384, microusd: 373728, task_seconds: 120 };
const exact = (a, b) => a && Object.keys(a).sort().join(',') === Object.keys(b).sort().join(',')
  && Object.keys(b).every(k => a[k] === b[k]);

export async function runResponseDiagnostic(root, settingsPath, signal = new AbortController().signal) {
  if (!(signal instanceof AbortSignal) || signal.aborted) throw Error('diagnostic cancelled');
  if (!path.isAbsolute(settingsPath) || (await fs.lstat(settingsPath)).mode & 0o077) throw Error('private settings required');
  const settings = JSON.parse(await regular(settingsPath, 16384));
  if (Object.keys(settings).sort().join(',') !== 'approvals,secret,version' || settings.version !== 1) throw Error('invalid diagnostic settings');
  const { runtime, manifest } = await verifyPaidRuntime(root);
  const read = async relative => {
    const pin = manifest.assets.find(a => a.path === relative && a.kind === 'configuration');
    const file = path.join(root, relative);
    if (!pin || await fs.realpath(file) !== file) throw Error('missing diagnostic pin');
    const bytes = await regular(file);
    if (digest(bytes) !== pin.sha256) throw Error('changed diagnostic asset');
    return bytes;
  };
  if (!(await read(runtimePath)).equals(await regular(new URL(import.meta.url)))) throw Error('use pinned diagnostic runtime');
  const contract = JSON.parse(await read(configPath)), payload = JSON.parse(await read(payloadPath));
  if (Object.keys(contract).sort().join(',') !== 'kind,limits,payload_sha256,source_manifest_sha256,version'
      || contract.version !== 1 || contract.kind !== 'single-response-diagnostic'
      || !exact(contract.limits, limits) || !/^[a-f0-9]{64}$/.test(contract.source_manifest_sha256)
      || !/^[a-f0-9]{64}$/.test(contract.payload_sha256)
      || !exact(manifest.limits, { requests: 2, tokens: 416385, microusd: 373728, task_seconds: 120 })
      || !exact(manifest.preparation_budget, { requests: 1, tokens: 1, microusd: 0, task_seconds: 120 })
      || manifest.provider_budget?.max_calls_per_task !== 1 || manifest.model.max_output_tokens !== 16384) {
    throw Error('invalid one-request diagnostic contract');
  }
  // The legacy campaign approval schema requires an allocated preparation slot.
  // It is unfunded and never executed. The actual diagnostic journal has ONE
  // request, and the ordinary launcher refuses diagnostic-marked manifests.
  const directory = path.join(root, 'private/diagnostic-attempt');
  try { await fs.lstat(directory); throw Error('diagnostic already started; retain evidence'); }
  catch (e) { if (e.code !== 'ENOENT') throw e; }
  const binaries = Object.fromEntries(Object.entries(runtime.binaries).map(([n,p]) => [n,path.join(root,p.path)]));
  // The contract pins the Rust manifest identity through the real approval gate.
  const { spawnSync } = await import('node:child_process');
  const plan = spawnSync(binaries.memory, ['eval','campaign','plan','--manifest',path.join(root,'campaign.json')], {
    env: { PATH: '/usr/bin:/bin', HOME: path.join(root,'private'), MEMORY_LAYER_PROFILE: 'prod' },
    encoding: 'utf8', timeout: 20000, maxBuffer: 1048576,
  });
  if (plan.status !== 0) throw Error('diagnostic approval manifest invalid');
  const identity = JSON.parse(plan.stdout).manifest_sha256;
  const credential = { manifest: path.join(root,'campaign.json'), manifestSha256: identity,
    memoryBinary: binaries.memory, baoBinary: binaries.bao, split: 'development',
    approvals: settings.approvals, secret: settings.secret };
  await checkApprovedCampaign(credential);
  // Exclusive mkdir fences competing launches and every interruption before any
  // key read or request. It is never removed or resumed, even on an early error.
  await fs.mkdir(directory, { mode: 0o700 });
  let mediator, accepted = false;
  try {
    mediator = await openRequestMediator({ binary: binaries.budget, directory,
      manifestSha256: identity, limits, taskLimits: limits });
    await withApprovedProvider(credential, async provider => {
      const prepared = await provider.bind(mediator, 'work-00000').prepareRequest(payload);
      if (digest(JSON.stringify(prepared.payload)) !== contract.payload_sha256
          || !exact(prepared.allowance, { input_tokens: 400000, output_tokens: 16384, microusd: 373728 })) {
        throw Error('diagnostic request differs from approved reconstruction');
      }
      // Exactly one dispatch. Returned actions/answers are never executed/scored.
      await mediator.dispatch({ taskId: 'work-00000', requestId: 'diagnostic-one',
        payload: prepared.payload, allowance: prepared.allowance, timeoutMs: 120000,
        invoke: async (received, current) => {
          const receipt = await prepared.invoke(received, current);
          return { usage: receipt.usage, output: { version: 1,
            diagnostic: 'accepted-structured-output', provider: receipt.output.provider } };
        }, signal });
      accepted = true;
    }, signal);
    const audit = (await mediator.audit()).audit;
    const result = { version: 1, kind: 'single-response-diagnostic', manifest_sha256: identity,
      accepted, benchmark_observations: 0, official_evidence: false, usage: audit.ledger };
    await fs.writeFile(path.join(directory,'diagnostic.json'), JSON.stringify(result)+'\n', { flag: 'wx', mode: 0o600 });
    return result;
  } finally { await mediator?.close(); }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const abort = new AbortController(), interrupt = () => abort.abort();
  process.once('SIGINT', interrupt); process.once('SIGTERM', interrupt);
  try {
    if (process.argv.length !== 4 || process.argv[3] !== '--allow-one-request') throw Error('explicit diagnostic consent required');
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
    console.log(JSON.stringify(await runResponseDiagnostic(root, path.resolve(process.argv[2]), abort.signal)));
  } catch {
    console.error('Diagnostic refused or interrupted. Retain private journal and structural diagnostics; no automatic retry.');
    process.exitCode = 1;
  } finally { process.removeListener('SIGINT',interrupt); process.removeListener('SIGTERM',interrupt); }
}
