// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';

const binary = process.env.CODIFIX_R3_CLI_BIN;
const scorer = process.env.CODIFIX_R3_SCORE_BIN;
if (process.env.CODIFIX_R3_REQUIRE_REVIEW === '1' && (!binary || !scorer)) throw Error('required review binaries missing');
test('campaign CLI exports blinded packets and applies synthetic ballots without rerunning execution', { skip: !binary || !scorer }, async t => {
  assert(path.isAbsolute(binary) && path.isAbsolute(scorer));
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-review-'));
  t.after(() => fs.rm(root, { recursive: true }));
  const pack = path.join(root, 'pack');
  const env = { PATH: process.env.PATH, HOME: root, CODIFIX_PROFILE: 'prod',
      CODIFIX__SERVICE__BIND_ADDR: '127.0.0.1:9', CODIFIX__SERVICE__API_TOKEN: 'unused-local-campaign-parser', CODIFIX__DATABASE__URL: 'postgresql://unused:unused@127.0.0.1:9/unused' };
  const invoke = (program, args) => spawnSync(program, args, { env, encoding: 'utf8', timeout: 60000, maxBuffer: 8 * 1024 * 1024 });
  const ok = result => { assert.equal(result.status, 0, result.stderr); return result; };
  const write = (file, value) => fs.writeFile(file, JSON.stringify(value));
  const read = async file => JSON.parse(await fs.readFile(file, 'utf8'));
  const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
  ok(invoke(process.execPath, ['evals/r3/build-review-pack.mjs', pack]));
  const manifestPath = path.join(pack, 'campaign.json');
  const manifest = await read(manifestPath);
  const selected = manifest.cases.find(c => c.track === 'knowledge' && c.split === 'development');
  const source = { id: selected.sources[0], text: await fs.readFile(path.join(pack, selected.sources[0]), 'utf8') };
  const rubric = { case_id: selected.id, question: 'Synthetic workflow check, not a benchmark label.',
    criteria: ['factual_correctness', 'required_evidence', 'appropriate_refusal'].map((dimension, i) => ({
      id: `criterion-${i}`, dimension, instruction: 'Synthetic executable review test only.', critical: false })),
    minimum_citations: 1, citations_critical: true };
  const rubricBytes = JSON.stringify(rubric) + '\n';
  await fs.writeFile(path.join(pack, 'private/review-rubric.json'), rubricBytes);
  manifest.assets.push({ path: 'private/review-rubric.json', sha256: hash(rubricBytes), kind: 'labels' });
  await write(manifestPath, manifest);
  const cli = args => invoke(binary, ['eval', 'campaign', ...args, '--manifest', manifestPath]);
  const plan = JSON.parse(ok(cli(['plan', '--split', 'development'])).stdout);
  const work = plan.work.find(w => w.case_id === selected.id);
  const answer = { text: 'Synthetic answer for testing review transport.', refused: false,
    citations: [{ source_id: source.id, quote: 'Fictional operational handover.' }] };
  const preparation = path.join(root, 'preparation.json');
  await write(preparation, { manifest_sha256: plan.manifest_sha256, work, rubric, sources: [source], answer });
  const scored = path.join(root, 'scored');
  ok(invoke(scorer, ['prepare', preparation, scored]));
  const evidence = await read(path.join(scored, 'private/evidence.json'));
  const configuration = manifest.assets.find(a => a.path === 'configuration.json').sha256;
  const observations = plan.work.map(w => ({ work: w, model: manifest.model,
    result: { item_id: w.case_id, eval_type: 'synthetic-review-test', condition: 'full-memory', metadata: {},
      success: false, skipped: false, scores: {}, duration_ms: 1,
      token_usage: { input_tokens: 0, output_tokens: 0, total_tokens: 0 }, answer: null, notes: [], sub_results: [] },
    execution_microusd: 0, provider_requests: 0, uncertain_charge: false, critical_failure: false, unresolved_rubric: false,
    isolation: { before_sha256: hash('snapshot'), after_sha256: hash('snapshot'), snapshot_sha256: hash('snapshot'),
      configuration_sha256: configuration, driver_sha256: configuration, service_build: 'synthetic-test-only',
      semantic_time: manifest.semantic_time, sealed: true, restricted_network: true, hidden_tests_inaccessible: true } }));
  const observation = observations.find(o => JSON.stringify(o.work) === JSON.stringify(work));
  observation.knowledge_review = evidence;
  observation.unresolved_rubric = true;
  observation.result.answer = answer.text;
  const artifact = { manifest_sha256: plan.manifest_sha256, split: 'development', offline: true, completed: true,
    preparation_microusd: 0, approvals: [], observations };
  const artifactPath = path.join(root, 'artifact.json');
  await write(artifactPath, artifact);
  const original = await fs.readFile(artifactPath, 'utf8');
  const exported = path.join(root, 'exported');
  ok(cli(['review', '--artifact', artifactPath, '--out', exported]));
  const names = await fs.readdir(path.join(exported, 'packets'));
  assert.equal(names.length, 1);
  const packet = await read(path.join(exported, 'packets', names[0]));
  for (const key of ['work', 'condition', 'repeat', 'model', 'execution_nonce', 'ballots']) assert(!(key in packet));
  if (process.platform !== 'win32') {
    assert.equal((await fs.stat(exported)).mode & 0o777, 0o700);
    assert.equal((await fs.stat(path.join(exported, 'packets', names[0]))).mode & 0o777, 0o600);
  }
  const ballot = await read(path.join(exported, 'ballots', names[0]));
  ballot.reviewer = 'synthetic-test-human-not-a-real-review';
  for (const judgment of Object.values(ballot.judgments)) {
    judgment.decision = 'pass'; judgment.reason = 'Synthetic CLI verification, not actual adjudication.';
  }
  const batch = await read(path.join(exported, 'batch-template.json'));
  batch.updates.push({ ballot, replaces_sha256: null });
  const batchPath = path.join(root, 'batch.json');
  await write(batchPath, batch);
  const applied = path.join(root, 'applied');
  ok(cli(['review', '--artifact', artifactPath, '--ballots', batchPath, '--out', applied]));
  const report = await read(path.join(applied, 'report.json'));
  assert(report.harness_valid && !report.official_evidence);
  assert.equal(await fs.readFile(artifactPath, 'utf8'), original);
  const updated = await read(path.join(applied, 'artifact.json'));
  const judged = updated.observations.find(o => JSON.stringify(o.work) === JSON.stringify(work));
  assert(judged.result.success && !judged.unresolved_rubric);
  assert.equal(judged.result.duration_ms, observation.result.duration_ms);
  assert.equal(judged.execution_microusd, 0);
  assert.equal(cli(['review', '--artifact', artifactPath, '--out', exported]).status, 1);
  batch.artifact_sha256 = '0'.repeat(64);
  await write(batchPath, batch);
  const rejected = path.join(root, 'rejected');
  assert.equal(cli(['review', '--artifact', artifactPath, '--ballots', batchPath, '--out', rejected]).status, 1);
  await assert.rejects(fs.stat(rejected), { code: 'ENOENT' });
  observations[0].result.token_usage = null;
  await write(artifactPath, artifact);
  assert.equal(cli(['review', '--artifact', artifactPath, '--out', rejected]).status, 1);
  await assert.rejects(fs.stat(rejected), { code: 'ENOENT' });
});
