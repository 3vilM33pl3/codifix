// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';

const names = ['MEMORY_LAYER_TEST_DATABASE_URL', 'MEMORY_R3_SNAPSHOT_BIN', 'MEMORY_R3_CLI_BIN',
  'MEMORY_R3_TEST_IMAGE', 'MEMORY_R3_RUST_IMAGE', 'MEMORY_R3_SCORE_BIN', 'MEMORY_R3_RETRIEVER_BIN'];
const wire = process.env.MEMORY_R3_WIRE === '1';
const mediated = process.env.MEMORY_R3_MEDIATED === '1' || wire;
if (mediated) names.push('MEMORY_R3_BUDGET_BIN');
const available = names.every(name => process.env[name]);
if (process.env.MEMORY_R3_REQUIRE_REHEARSAL === '1' && !available) throw Error('required integrated rehearsal configuration missing');

test('rehearsal bootstrap requires explicit disposable configuration before creating output', async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-rehearsal-reject-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const out = path.join(directory, 'pack');
  const result = spawnSync(process.execPath, ['evals/r3/rehearse.mjs', out], { env: { PATH: '/usr/bin:/bin' }, encoding: 'utf8' });
  assert.equal(result.status, 1);
  await assert.rejects(fs.stat(out));
});

test(`existing campaign CLI completes a real 75-dispatch ${wire ? 'HTTP-mediated' : mediated ? 'mediated' : 'isolated'} rehearsal with pending human review`, { skip: !available, timeout: mediated ? 900000 : 600000 }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-rehearsal-cli-'));
  let verified = false;
  t.after(async () => {
    if (verified && process.env.MEMORY_R3_KEEP_REHEARSAL !== '1') await fs.rm(directory, { recursive: true });
    else console.error(`${verified ? 'Verified' : 'Failed'} rehearsal diagnostics retained at ${directory}`);
  });
  const root = path.join(directory, 'pack');
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, MEMORY_LAYER_TEST_REQUIRE_DB: '1' };
  for (const name of names) env[name] = process.env[name];
  const originalBins = path.join(directory, 'build-outputs');
  await fs.mkdir(originalBins, { mode: 0o700 });
  for (const name of names.filter(name => name.endsWith('_BIN'))) {
    const copy = path.join(originalBins, name);
    await fs.copyFile(env[name], copy);
    await fs.chmod(copy, 0o700);
    env[name] = copy;
  }
  let replacedBuildOutput = false, replacement;
  const run = await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ['evals/r3/rehearse.mjs', root, '--one-family-per-track', ...(wire ? ['--wire'] : mediated ? ['--mediated'] : [])], { env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    const timer = setTimeout(() => child.kill('SIGINT'), mediated ? 850000 : 550000);
    child.stdout.on('data', chunk => { stdout += chunk; if (stdout.length > 1048576) child.kill('SIGINT'); });
    child.stderr.on('data', chunk => {
      stderr += chunk;
      if (stderr.length > 1048576) child.kill('SIGINT');
      if (!replacedBuildOutput && stderr.includes('Offline rehearsal completed 1/75 tasks.')) {
        replacedBuildOutput = true;
        // Replace only this test's copy of an upstream build artifact. Neither
        // the checkout's binaries nor the frozen runtime copy are modified.
        replacement = fs.writeFile(env.MEMORY_R3_SCORE_BIN, 'Deliberately replaced test-owned build output.');
        replacement.catch(() => {});
      }
    });
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('close', code => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
  });
  await replacement;
  assert(replacedBuildOutput, 'build-output mutation must actually be exercised');
  assert.equal(run.code, 0, run.stderr);
  const summary = JSON.parse(run.stdout.trim().split('\n').at(-1));
  assert(summary.execution_complete && !summary.official_evidence && !summary.quality_passed);
  assert.equal(summary.task_executions, 75);
  assert.equal(summary.knowledge_reviews_pending, 60);
  const artifact = JSON.parse(await fs.readFile(summary.artifact));
  assert(artifact.completed && artifact.offline);
  assert.equal(artifact.observations.length, 75);
  const { inspectCampaignDispatch } = await import('./campaign-dispatch.mjs');
  const dispatch = await inspectCampaignDispatch(path.join(root, 'private/dispatch'));
  assert.equal(dispatch.manifest_sha256, artifact.manifest_sha256);
  assert.equal(dispatch.entries.length, mediated ? 76 : 75);
  assert(dispatch.entries.every(entry => entry.status === 'completed'));
  assert.equal(dispatch.execution_enabled, false);
  assert.equal(new Set(artifact.observations.map(row => JSON.stringify(row.work))).size, 75);
  assert(artifact.observations.every(row => row.execution_microusd === 0));
  if (mediated) {
    if (wire) {
      assert.equal(artifact.manifest_sha256, JSON.parse(await fs.readFile(path.join(root, 'run/plan.json'))).manifest_sha256);
      const results = path.join(root, 'run/preparation/provider/results');
      const saved = JSON.parse(await fs.readFile(path.join(results, (await fs.readdir(results))[0])));
      assert(saved.output.provider, 'wire mode must retain decoded provider usage');
      assert.deepEqual(saved.output.vector.length, 16);
    }
    assert(artifact.preparation_receipt);
    assert(artifact.preparation_receipt.audit.ledger.requests >= 12); // Six source documents, both preparation paths.
    assert.equal(artifact.preparation_receipt.audit.ledger.tokens, artifact.preparation_receipt.audit.ledger.requests);
    assert.equal(artifact.preparation_receipt.snapshot_sha256, artifact.observations[0].isolation.snapshot_sha256);
    assert.equal(artifact.observations.reduce((sum,row) => sum + row.provider_requests,0),180);
    assert(artifact.observations.every(row => row.provider_audit.calls.length === row.provider_requests
      && row.provider_audit.ledger.tokens === row.result.token_usage.total_tokens));
    for (const row of artifact.observations) {
      const embeddings = row.provider_audit.calls.filter(call => call.request_id.startsWith('query-embedding-'));
      assert.equal(embeddings.length, ['vector', 'full-memory'].includes(row.work.condition) ? 1 : 0);
      if (row.result.sub_results?.length) {
        assert.equal(row.result.token_usage.total_tokens,
          row.result.sub_results.reduce((sum, step) => sum + step.token_usage.total_tokens, 0) + embeddings.length);
      }
    }
  } else assert(artifact.observations.every(row => row.provider_requests === 0));
  assert(artifact.observations.every(row => row.isolation.before_sha256 === row.isolation.after_sha256));
  assert.equal(new Set(artifact.observations.map(row => row.isolation.snapshot_sha256)).size, 1);
  const coding = artifact.observations.filter(row => row.work.case_id === 'durable-importer');
  assert.equal(coding.length, 15);
  assert(coding.every(row => !row.result.success && row.result.sub_results.length === 3));
  const knowledge = artifact.observations.filter(row => row.knowledge_review);
  assert.equal(knowledge.length, 60);
  assert(knowledge.every(row => row.unresolved_rubric && row.knowledge_review.ballots.length === 0));
  assert.equal((await fs.readdir(path.join(summary.review, 'packets'))).length, 60);
  const report = JSON.parse(await fs.readFile(path.join(root, 'run/report.json')));
  assert(!report.harness_valid && !report.official_evidence && !report.quality_passed, 'pending judgments are not silently approved');
  assert(report.execution_valid);
  assert.equal(report.pending_knowledge_reviews, 60);
  assert(!JSON.stringify(artifact).includes(env.MEMORY_LAYER_TEST_DATABASE_URL));
  const runtime = JSON.parse(await fs.readFile(path.join(root, 'private/runtime.json')));
  for (const binary of Object.values(runtime.binaries)) {
    assert(binary.path.startsWith(path.join(root, 'private/executables') + '/'));
    assert.equal((await fs.stat(binary.path)).mode & 0o777, 0o500);
  }
  await assert.rejects(fs.stat(runtime.socket), 'owned IPC socket is removed after controller cleanup');
  verified = true;
});
