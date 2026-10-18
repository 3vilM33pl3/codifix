#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted owner. Normal use executes the hash-locked copy with its pinned Node.
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { verifyPaidRuntime, regular } from './paid-runtime.mjs';
import { checkApprovedCampaign } from './approved-credential.mjs';
import { withApprovedCampaignSession } from './approved-campaign-session.mjs';
import { openCampaignDispatch } from './campaign-dispatch.mjs';

function cli(binary, root, args, timeoutMs = 20000) {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, ['eval', 'campaign', ...args], { cwd: root,
      env: { PATH: '/usr/bin:/bin', HOME: path.join(root, 'private'), CODIFIX_PROFILE: 'prod',
      CODIFIX__SERVICE__BIND_ADDR: '127.0.0.1:9', CODIFIX__SERVICE__API_TOKEN: 'unused-local-campaign-parser', CODIFIX__DATABASE__URL: 'postgresql://unused:unused@127.0.0.1:9/unused' },
      stdio: ['ignore', 'pipe', 'pipe'] });
    const timer = setTimeout(() => child.kill('SIGKILL'), timeoutMs);
    let stdout = '', stderr = '';
    child.stdout.on('data', chunk => { stdout += chunk; if (Buffer.byteLength(stdout) > 16 * 1024 * 1024) child.kill('SIGKILL'); });
    child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-65536); });
    child.once('error', () => { clearTimeout(timer); reject(Error('campaign executable unavailable')); });
    child.once('close', (code, signal) => { clearTimeout(timer); resolve({ code: signal ? 1 : code, stdout, stderr }); });
  });
}

export async function runPaidCampaign(root, settingsPath, signal = new AbortController().signal) {
  if (!(signal instanceof AbortSignal) || signal.aborted) throw Error('campaign cancelled');
  if (typeof settingsPath !== 'string' || !path.isAbsolute(settingsPath)
      || (await fs.lstat(settingsPath)).mode & 0o077) throw Error('private explicit settings required');
  const settings = JSON.parse(await regular(settingsPath, 16384));
  if (Object.keys(settings).sort().join(',') !== 'approvals,database_url,disposable_database,secret,split,version'
      || settings.version !== 1 || settings.disposable_database !== true
      || !['development', 'held-out'].includes(settings.split)) throw Error('invalid launcher settings');
  const db = new URL(settings.database_url);
  if (!['postgres:', 'postgresql:'].includes(db.protocol) || !['127.0.0.1', 'localhost'].includes(db.hostname)
      || !db.port || !/^\/(memory_test|memory_r3_[a-z0-9_-]+)$/.test(db.pathname) || db.search || db.hash) throw Error('explicit local disposable evaluation database required');
  const { runtime, manifest } = await verifyPaidRuntime(root);
  if (manifest.assets.some(a => a.path === 'private/response-diagnostic.json')) throw Error('diagnostic pack cannot run a campaign');
  if (path.join(root, 'private/dispatch.sock').length > 100) throw Error('use a shorter private pack path');
  // Existing evidence, even an empty abandoned owner directory, fences launch
  // before approval/key access. Never delete it or restart a charged campaign.
  for (const relative of ['run', 'private/dispatch', 'private/dispatch.sock']) {
    try { await fs.lstat(path.join(root, relative)); }
    catch (error) { if (error.code === 'ENOENT') continue; throw error; }
    throw Error('campaign already started; inspect retained evidence');
  }
  const binaries = Object.fromEntries(Object.entries(runtime.binaries).map(([name, pin]) => [name, path.join(root, pin.path)]));
  const manifestPath = path.join(root, 'campaign.json'), approvalArgs = settings.approvals?.flatMap(file => ['--approval', file]);
  const planned = await cli(binaries.memory, root, ['plan', '--manifest', manifestPath, '--split', settings.split]);
  if (planned.code !== 0) throw Error('campaign planning failed');
  const plan = JSON.parse(planned.stdout);
  const credential = { memoryBinary: binaries.memory, baoBinary: binaries.bao, manifest: manifestPath,
    manifestSha256: plan.manifest_sha256, approvals: settings.approvals, split: settings.split, secret: settings.secret };
  await checkApprovedCampaign(credential);
  if (signal.aborted) throw Error('campaign cancelled');
  let active, dispatcher, closed = false;
  const interrupt = () => { closed = true; void dispatcher?.close().catch(() => {}); };
  signal.addEventListener('abort', interrupt, { once: true });
  try {
    dispatcher = await openCampaignDispatch({ root, directory: path.join(root, 'private/dispatch'),
      socket: path.join(root, 'private/dispatch.sock'), manifestSha256: plan.manifest_sha256,
      work: plan.work, offline: false, preparation: true,
      verify: async () => { await verifyPaidRuntime(root); await checkApprovedCampaign(credential); },
      prepare: contract => active.prepare(contract), execute: (work, contract) => active.execute(work, contract) });
    if (closed || signal.aborted) throw Error('campaign cancelled');
    const pin = relative => manifest.assets.find(a => a.path === relative)?.sha256;
    await withApprovedCampaignSession({ credential, session: { offline: false, split: settings.split.replace('-', '_'),
      databaseUrl: settings.database_url, snapshotBinary: binaries.snapshot, memoryBinary: binaries.memory,
      configuration: { root, manifestSha256: plan.manifest_sha256, budgetBinary: binaries.budget,
        scoreBinary: binaries.score, retrieverBinary: binaries.retriever, nodeImage: runtime.images.node, rustImage: runtime.images.rust,
        driverSha256: pin('runtime/approved-driver.mjs'), configurationSha256: pin('private/paid-runtime.json'),
        serviceBuild: `memory-sha256:${runtime.binaries.memory.sha256}` } } }, async session => {
      active = session;
      try {
        const timeout = Math.min(2147483647, (manifest.limits.task_seconds * plan.work.length + manifest.preparation_budget.task_seconds + 60) * 1000);
        const run = await cli(binaries.memory, root, ['run', '--manifest', manifestPath, '--split', settings.split,
          '--driver', path.join(root, 'runtime/approved-driver.mjs'), '--out', path.join(root, 'run'), '--allow-shell',
          '--approved-runtime', path.join(root, 'private/paid-runtime.json'), ...approvalArgs], timeout);
        await fs.writeFile(path.join(root, 'private/run-diagnostic.txt'), run.stderr, { flag: 'wx', mode: 0o600 });
      } finally {
        // Abort/drain the host dispatch before releasing its database session.
        // A killed driver does not imply that its provider never accepted work.
        await dispatcher.close();
      }
    }, dispatcher.signal);
    const artifactPath = path.join(root, 'run/artifact.json');
    const artifact = JSON.parse(await regular(artifactPath, 256 * 1024 * 1024));
    if (closed || !artifact.completed || artifact.offline || artifact.observations.length !== plan.work.length
        || dispatcher.completed !== plan.work.length) throw Error('campaign incomplete; retain journals');
    const review = await cli(binaries.memory, root, ['review', '--manifest', manifestPath, '--artifact', artifactPath,
      '--out', path.join(root, 'run/review')]);
    if (review.code !== 0) throw Error('campaign execution evidence invalid');
    const report = JSON.parse(await regular(path.join(root, 'run/report.json'), 16 * 1024 * 1024));
    return { execution_complete: true, task_executions: dispatcher.completed, artifact: artifactPath,
      execution_valid: report.execution_valid, harness_valid: report.harness_valid,
      official_evidence: report.official_evidence, quality_passed: report.quality_passed,
      knowledge_reviews_pending: report.pending_knowledge_reviews };
  } finally {
    signal.removeEventListener('abort', interrupt);
    await dispatcher?.close();
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const abort = new AbortController(), interrupt = () => abort.abort();
  process.once('SIGINT', interrupt); process.once('SIGTERM', interrupt);
  try {
    if (process.argv.length !== 4 || process.argv[3] !== '--allow-shell') throw Error('explicit settings and execution consent required');
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
    const summary = await runPaidCampaign(root, path.resolve(process.argv[2]), abort.signal);
    console.log(JSON.stringify(summary));
    if (!summary.harness_valid || (summary.official_evidence && !summary.quality_passed)) process.exitCode = 2;
  } catch {
    console.error('Approved campaign refused or interrupted. Retain private evidence and reconcile uncertain charges; no automatic retry.');
    process.exitCode = 1;
  } finally { process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', interrupt); }
}
