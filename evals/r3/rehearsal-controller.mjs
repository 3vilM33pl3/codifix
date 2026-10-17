// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs only from its hash-locked copy in a newly generated development pack.
import fs from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { openPreparedSession } from './isolation/service-session.mjs';
import { createRehearsalExecutor } from './isolation/rehearsal-task.mjs';
import { syntheticServiceEmbedding, syntheticVector, syntheticEmbeddingModel } from './synthetic-embedding.mjs';
import { openRequestMediator } from './request-mediator.mjs';
import { openSyntheticWireProvider } from './synthetic-wire-provider.mjs';
import { openProviderCampaignSession } from './provider-campaign-session.mjs';
import { openCampaignDispatch } from './campaign-dispatch.mjs';
import { createWindowsHostJournal } from './windows-host-journal.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'campaign.json');
const manifest = JSON.parse(await fs.readFile(manifestPath));
const configurationBytes = await fs.readFile(path.join(root, 'private/runtime.json'));
const configuration = JSON.parse(configurationBytes);
const configurationSha256 = createHash('sha256').update(configurationBytes).digest('hex');
const native = configuration.execution?.kind === 'windows-native-v1';
if (!manifest.smoke_only || manifest.label_status !== 'draft' || manifest.limits.microusd !== 0
    || process.env.MEMORY_LAYER_TEST_REQUIRE_DB !== '1') throw Error('offline disposable rehearsal only');
if ((process.platform === 'win32' && !native) || (configuration.execution !== undefined && !native)
    || (native && (configuration.syntheticWire || manifest.provider_budget || manifest.preparation_budget
      || configuration.socket !== undefined || configuration.images !== undefined
      || Object.keys(configuration.dispatch ?? {}).sort().join(',') !== 'endpoint,kind'
      || configuration.dispatch.kind !== 'windows-loopback-v1'
      || configuration.dispatch.endpoint !== 'private/dispatch-endpoint.json'))) throw Error('explicit deterministic native controller required');
if (!manifest.assets.some(asset => asset.path === 'private/runtime.json' && asset.sha256 === configurationSha256)) throw Error('controller configuration changed');
const journal = native ? await createWindowsHostJournal(configuration.execution) : undefined;
if (journal) await journal.read(path.join(root, 'private'), 'runtime.json', { sha256: configurationSha256 });
const env = native ? { SystemRoot: process.env.SystemRoot, WINDIR: process.env.SystemRoot,
  PATH: path.dirname(configuration.binaries.node.path), HOME: process.env.HOME, USERPROFILE: process.env.HOME,
  TEMP: process.env.HOME, TMP: process.env.HOME, MEMORY_LAYER_PROFILE: 'prod' }
  : { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: process.env.HOME, MEMORY_LAYER_PROFILE: 'prod' };
let activeCli, stopping = false, dispatcher;
const interrupted = () => { stopping = true; activeCli?.kill(native ? 'SIGKILL' : 'SIGINT'); void dispatcher?.close(); };
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
  if (createHash('sha256').update(await fs.readFile(path.join(root, 'private/runtime.json'))).digest('hex') !== configurationSha256) throw Error('controller configuration changed');
  for (const binary of Object.values(configuration.binaries)) {
    const digest = createHash('sha256');
    for await (const chunk of createReadStream(binary.path)) digest.update(chunk);
    if (digest.digest('hex') !== binary.sha256) throw Error('rehearsal executable changed');
  }
};
const out = path.join(root, 'run');
let session, wireProvider, providerSession;
try {
  await verifyBinaries();
  if (configuration.syntheticWire) wireProvider = await openSyntheticWireProvider(manifest);
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
  let execute;
  const prepare = async contract => {
    if (wireProvider) {
      providerSession = await openProviderCampaignSession({ provider: wireProvider, offline: true, split: 'development',
        snapshotBinary: configuration.binaries.snapshot.path, memoryBinary: configuration.binaries.memory.path,
        databaseUrl: process.env.MEMORY_LAYER_TEST_DATABASE_URL,
        configuration: { root, manifestSha256: plan.manifest_sha256, nodeImage: configuration.images.node, rustImage: configuration.images.rust,
          scoreBinary: configuration.binaries.score.path, retrieverBinary: configuration.binaries.retriever.path,
          budgetBinary: configuration.binaries.budget.path,
          driverSha256: manifest.assets.find(asset => asset.path === 'runtime/offline-driver.mjs').sha256,
          configurationSha256: manifest.assets.find(asset => asset.path === 'private/runtime.json').sha256,
          serviceBuild: `memory-sha256:${configuration.binaries.memory.sha256}` } });
      const receipt = await providerSession.prepare(contract);
      execute = (work, contract) => providerSession.execute(work, contract);
      return receipt;
    }
    let mediator, call = 0;
    const baselineEmbeddings = new Map();
    try {
      if (manifest.preparation_budget) {
        if (!contract || contract.directory !== path.join(out, 'preparation/provider')
            || contract.task_id !== 'preparation' || contract.manifest_sha256 !== plan.manifest_sha256
            || !contract.limits || Object.keys(contract.limits).length !== 4
            || ['requests', 'tokens', 'microusd', 'task_seconds'].some(key => contract.limits[key] !== manifest.preparation_budget[key])
            || JSON.stringify(contract.task_limits) !== JSON.stringify(contract.limits)) throw Error('invalid preparation contract');
        mediator = await openRequestMediator({ binary: configuration.binaries.budget.path, directory: contract.directory,
          manifestSha256: plan.manifest_sha256, limits: contract.limits, taskLimits: contract.task_limits });
      }
      const embedding = syntheticServiceEmbedding();
      if (wireProvider) embedding.embed = wireProvider.bind(mediator, 'preparation').embed;
      else if (mediator) embedding.embed = (input, purpose) => mediator.dispatch({ taskId: 'preparation', requestId: `document-${call++}`,
        payload: { model: syntheticEmbeddingModel, purpose, input }, allowance: { input_tokens: 64, output_tokens: 0, microusd: 0 },
        timeoutMs: 10000, invoke: async payload => ({ output: syntheticVector(payload.input), usage: { input_tokens: 1, output_tokens: 0, microusd: 0 } }) });
      session = await openPreparedSession({ snapshotBinary: configuration.binaries.snapshot.path,
        memoryBinary: configuration.binaries.memory.path, databaseUrl: process.env.MEMORY_LAYER_TEST_DATABASE_URL,
        semanticTime: manifest.semantic_time, documents, embedding, ...(native ? { execution: configuration.execution } : {}) });
      if (mediator) for (const document of documents) {
        baselineEmbeddings.set(`${document.project}:${document.source}`, await embedding.embed(document.text, 'document'));
      }
      execute = await createRehearsalExecutor({ session, wireProvider, baselineEmbeddings: mediator ? baselineEmbeddings : undefined, configuration: { root,
        manifestSha256: plan.manifest_sha256, retrieverBinary: configuration.binaries.retriever.path,
        ...(native ? { execution: configuration.execution, nativeScorer: configuration.nativeScorer, compiler: configuration.compiler }
          : { nodeImage: configuration.images.node, rustImage: configuration.images.rust,
            scoreBinary: configuration.binaries.score.path, budgetBinary: configuration.binaries.budget?.path }),
        driverSha256: manifest.assets.find(asset => asset.path === 'runtime/offline-driver.mjs').sha256,
        configurationSha256: manifest.assets.find(asset => asset.path === 'private/runtime.json').sha256,
        serviceBuild: `memory-sha256:${configuration.binaries.memory.sha256}` } });
      return mediator ? { version: 1, snapshot_sha256: session.preparation.snapshot_sha256, audit: (await mediator.audit()).audit } : undefined;
    } finally { await mediator?.close(); }
  };
  if (!manifest.preparation_budget) await prepare();
  const driver = path.join(root, 'runtime/offline-driver.mjs');
  dispatcher = await openCampaignDispatch({ root, directory: path.join(root, 'private/dispatch'),
    ...(native ? { native: { journal, configurationSha256, endpointPath: path.join(root, configuration.dispatch.endpoint) } }
      : { socket: configuration.socket }),
    manifestSha256: plan.manifest_sha256, work: plan.work, offline: true, preparation: !!manifest.preparation_budget,
    verify: verifyBinaries, prepare, execute: (work, contract) => execute(work, contract),
    onProgress: (cursor, total) => console.error(`Offline rehearsal completed ${cursor}/${total} tasks.`) });
  if (stopping) throw Error('rehearsal interrupted');
  const run = await cli(['run', '--manifest', manifestPath, '--split', 'development', '--driver', driver, '--out', out,
    '--offline', '--allow-shell']);
  // Private bounded diagnostics from the host CLI, never an operational listing
  // or agent input. This lets an operator distinguish schema failures from
  // genuine pending review without rerunning an uncertain dispatch.
  if (journal) await journal.write(path.join(root, 'private'), 'run-diagnostic.json', { stderr: run.stderr });
  else await fs.writeFile(path.join(root, 'private/run-diagnostic.txt'), run.stderr, { flag: 'wx', mode: 0o600 });
  const artifact = JSON.parse(await fs.readFile(path.join(out, 'artifact.json')));
  if (!artifact.completed || artifact.observations.length !== plan.work.length || dispatcher.completed !== plan.work.length) throw Error('rehearsal incomplete');
  if (wireProvider && wireProvider.calls !== artifact.preparation_receipt.audit.ledger.requests
      + artifact.observations.reduce((sum, row) => sum + row.provider_requests, 0)) throw Error('wire call accounting mismatch');
  // A complete execution normally exits nonzero because semantic judgments are
  // genuinely pending. The existing review validator distinguishes that from
  // missing/invalid usage, input bindings or isolation evidence.
  const review = await cli(['review', '--manifest', manifestPath, '--artifact', path.join(out, 'artifact.json'), '--out', path.join(out, 'review')]);
  if (review.code !== 0) throw Error('rehearsal evidence failed review validation');
  console.log(JSON.stringify({ execution_complete: true, task_executions: dispatcher.completed,
    campaign_exit_code: run.code, knowledge_reviews_pending: artifact.observations.filter(row => row.unresolved_rubric).length,
    official_evidence: false, quality_passed: false, artifact: path.join(out, 'artifact.json'), review: path.join(out, 'review'),
    limitations: ['Synthetic excerpt/no-op agents and fake vectors.', 'Execution timings separate setup/grading but do not measure real model latency.',
      'Development-only fixtures; labels, paid evaluation and complete R3 evidence remain pending.'] }));
} catch {
  console.error('Offline R3 rehearsal incomplete or invalid. Retain its private artifacts; do not automatically repeat uncertain work.');
  process.exitCode = 1;
} finally {
  stopping = true;
  activeCli?.kill(native ? 'SIGKILL' : 'SIGINT');
  await wireProvider?.close();
  await dispatcher?.close();
  if (session) {
    try { await session.close(); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
  }
  if (providerSession) {
    try { await providerSession.close(); }
    catch { console.error('Provider campaign cleanup failed; retain private staging.'); process.exitCode = 1; }
  }
}
