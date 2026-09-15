// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs only from its hash-locked copy in a newly generated development pack.
import fs from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { openPreparedSession } from './isolation/service-session.mjs';
import { createRehearsalExecutor } from './isolation/rehearsal-task.mjs';
import { syntheticServiceEmbedding } from './synthetic-embedding.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'campaign.json');
const manifest = JSON.parse(await fs.readFile(manifestPath));
const configuration = JSON.parse(await fs.readFile(path.join(root, 'private/runtime.json')));
if (!manifest.smoke_only || manifest.label_status !== 'draft' || manifest.limits.microusd !== 0
    || process.env.MEMORY_LAYER_TEST_REQUIRE_DB !== '1') throw Error('offline disposable rehearsal only');
const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: process.env.HOME, MEMORY_LAYER_PROFILE: 'prod' };
let activeCli, stopping = false;
const interrupted = () => { stopping = true; activeCli?.kill('SIGINT'); };
process.once('SIGINT', interrupted);
process.once('SIGTERM', interrupted);
const cli = args => new Promise((resolve, reject) => {
  if (stopping) { reject(Error('rehearsal interrupted')); return; }
  const child = spawn(configuration.binaries.memory.path, ['eval', 'campaign', ...args], { cwd: root, env,
    stdio: ['ignore', 'pipe', 'pipe'] });
  activeCli = child;
  let stdout = '', stderr = '';
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-65536); });
  child.stdout.on('data', chunk => {
    stdout += chunk;
    if (Buffer.byteLength(stdout) > 16 * 1024 * 1024) child.kill('SIGKILL');
  });
  child.once('error', () => { activeCli = undefined; reject(Error('campaign CLI unavailable')); });
  child.once('close', (code, signal) => { activeCli = undefined; resolve({ code: signal ? 1 : code, stdout, stderr }); });
});
const verifyBinaries = async () => {
  for (const binary of Object.values(configuration.binaries)) {
    const digest = createHash('sha256');
    for await (const chunk of createReadStream(binary.path)) digest.update(chunk);
    if (digest.digest('hex') !== binary.sha256) throw Error('rehearsal executable changed');
  }
};
const key = work => JSON.stringify([work.case_id, work.condition, work.repeat]);
const out = path.join(root, 'run');
let session, server, activeTask;
try {
  await verifyBinaries();
  const planned = await cli(['plan', '--manifest', manifestPath, '--split', 'development']);
  if (planned.code !== 0) throw Error('campaign inputs invalid');
  const plan = JSON.parse(planned.stdout);
  const documents = [], seen = new Set();
  for (const item of manifest.cases) {
    if (item.split !== 'development') throw Error('this rehearsal does not consume held-out data');
    for (const source of item.sources) {
      const identity = `${item.cluster}:${source}`;
      if (seen.has(identity)) continue;
      seen.add(identity);
      documents.push({ project: `r3-${item.cluster}`, source, text: await fs.readFile(path.join(root, source), 'utf8') });
    }
  }
  session = await openPreparedSession({ snapshotBinary: configuration.binaries.snapshot.path,
    memoryBinary: configuration.binaries.memory.path, databaseUrl: process.env.MEMORY_LAYER_TEST_DATABASE_URL,
    semanticTime: manifest.semantic_time, documents, embedding: syntheticServiceEmbedding() });
  const driver = path.join(root, 'runtime/offline-driver.mjs');
  const execute = await createRehearsalExecutor({ session, configuration: { root,
    manifestSha256: plan.manifest_sha256, nodeImage: configuration.images.node, rustImage: configuration.images.rust,
    scoreBinary: configuration.binaries.score.path, retrieverBinary: configuration.binaries.retriever.path,
    budgetBinary: configuration.binaries.budget?.path,
    driverSha256: manifest.assets.find(asset => asset.path === 'runtime/offline-driver.mjs').sha256,
    configurationSha256: manifest.assets.find(asset => asset.path === 'private/runtime.json').sha256,
    serviceBuild: `memory-sha256:${configuration.binaries.memory.sha256}` } });
  let cursor = 0;
  const receipts = new Map();
  server = http.createServer(async (request, response) => {
    response.on('error', () => {});
    try {
      if (stopping || request.method !== 'POST' || request.url !== '/dispatch') throw Error('unavailable');
      const chunks = [];
      let length = 0;
      for await (const chunk of request) {
        if ((length += chunk.length) > 32768) throw Error('oversized');
        chunks.push(chunk);
      }
      const bytes = Buffer.concat(chunks);
      const dispatch = JSON.parse(bytes);
      if (dispatch.version !== 1 || dispatch.offline !== true || dispatch.maximum_microusd !== 0
          || dispatch.manifest !== manifestPath || dispatch.manifest_sha256 !== plan.manifest_sha256) throw Error('invalid dispatch');
      const identity = key(dispatch.work);
      const digest = createHash('sha256').update(bytes).digest('hex');
      let receipt = receipts.get(identity);
      if (receipt && receipt.digest !== digest) throw Error('changed retry');
      if (!receipt) {
        if (activeTask || !plan.work[cursor] || key(plan.work[cursor]) !== identity) throw Error('out of order dispatch');
        const task = (async () => {
          await verifyBinaries();
          const observation = await execute(dispatch.work,dispatch.provider_budget);
          await verifyBinaries();
          cursor++;
          console.error(`Offline rehearsal completed ${cursor}/${plan.work.length} tasks.`);
          return observation;
        })();
        activeTask = task;
        // Retain failed/uncertain attempts too; replay never starts another task.
        receipt = { digest, task };
        receipts.set(identity, receipt);
        task.then(() => { activeTask = undefined; }, () => { activeTask = undefined; });
      }
      const observation = await receipt.task;
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify(observation));
    } catch {
      response.writeHead(409, { 'Content-Type': 'text/plain' });
      response.end('Offline rehearsal dispatch refused or failed.');
    }
  });
  server.requestTimeout = 600000;
  server.headersTimeout = 10000;
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(configuration.socket, resolve); });
  await fs.chmod(configuration.socket, 0o600);
  const run = await cli(['run', '--manifest', manifestPath, '--split', 'development', '--driver', driver, '--out', out,
    '--offline', '--allow-shell']);
  // Private bounded diagnostics from the host CLI, never an operational listing
  // or agent input. This lets an operator distinguish schema failures from
  // genuine pending review without rerunning an uncertain dispatch.
  await fs.writeFile(path.join(root, 'private/run-diagnostic.txt'), run.stderr, { flag: 'wx', mode: 0o600 });
  const artifact = JSON.parse(await fs.readFile(path.join(out, 'artifact.json')));
  if (!artifact.completed || artifact.observations.length !== plan.work.length || cursor !== plan.work.length) throw Error('rehearsal incomplete');
  // A complete execution normally exits nonzero because semantic judgments are
  // genuinely pending. The existing review validator distinguishes that from
  // missing/invalid usage, input bindings or isolation evidence.
  const review = await cli(['review', '--manifest', manifestPath, '--artifact', path.join(out, 'artifact.json'), '--out', path.join(out, 'review')]);
  if (review.code !== 0) throw Error('rehearsal evidence failed review validation');
  console.log(JSON.stringify({ execution_complete: true, task_executions: cursor,
    campaign_exit_code: run.code, knowledge_reviews_pending: artifact.observations.filter(row => row.unresolved_rubric).length,
    official_evidence: false, quality_passed: false, artifact: path.join(out, 'artifact.json'), review: path.join(out, 'review'),
    limitations: ['Synthetic excerpt/no-op agents and fake vectors.', 'Execution timings separate setup/grading but do not measure real model latency.',
      'Development-only fixtures; labels, paid evaluation and complete R3 evidence remain pending.'] }));
} catch {
  console.error('Offline R3 rehearsal incomplete or invalid. Retain its private artifacts; do not automatically repeat uncertain work.');
  process.exitCode = 1;
} finally {
  stopping = true;
  activeCli?.kill('SIGINT');
  if (activeTask) { try { await activeTask; } catch { /* attempt remains failed */ } }
  if (server?.listening) await new Promise(resolve => server.close(resolve));
  if (session) {
    try { await session.close(); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
  }
}
