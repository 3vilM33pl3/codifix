// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { openPreparedSession } from './service-session.mjs';
import { createRehearsalExecutor } from './rehearsal-task.mjs';

const snapshotBinary = process.env.MEMORY_R3_SNAPSHOT_BIN;
const memoryBinary = process.env.MEMORY_R3_CLI_BIN;
const databaseUrl = process.env.MEMORY_LAYER_TEST_DATABASE_URL;
const nodeImage = process.env.MEMORY_R3_TEST_IMAGE;
const rustImage = process.env.MEMORY_R3_RUST_IMAGE;
const scoreBinary = process.env.MEMORY_R3_SCORE_BIN;
const retrieverBinary = process.env.MEMORY_R3_RETRIEVER_BIN;
const available = snapshotBinary && memoryBinary && databaseUrl && nodeImage && rustImage && scoreBinary && retrieverBinary;
if (process.env.MEMORY_R3_REQUIRE_REHEARSAL_TASK === '1' && !available) throw Error('required rehearsal task configuration missing');

test('representative tasks execute all knowledge conditions and real hidden Rust grading', { skip: !available, timeout: 180000 }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-rehearsal-task-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const root = path.join(directory, 'pack');
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, MEMORY_LAYER_PROFILE: 'prod' };
  assert.equal(spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', root, '--one-family-per-track'], { env }).status, 0);
  const manifest = JSON.parse(await fs.readFile(path.join(root, 'campaign.json')));
  const planResult = spawnSync(memoryBinary, ['eval', 'campaign', 'plan', '--manifest', path.join(root, 'campaign.json')], { env, encoding: 'utf8' });
  assert.equal(planResult.status, 0, planResult.stderr);
  const plan = JSON.parse(planResult.stdout);
  const documents = [];
  const seen = new Set();
  for (const item of manifest.cases) for (const source of item.sources) {
    const key = `${item.cluster}:${source}`;
    if (seen.has(key)) continue;
    seen.add(key);
    documents.push({ project: `r3-${item.cluster}`, source, text: await fs.readFile(path.join(root, source), 'utf8') });
  }
  const session = await openPreparedSession({ snapshotBinary, memoryBinary, databaseUrl,
    semanticTime: manifest.semantic_time, documents });
  try {
    const configurationSha256 = manifest.assets.find(asset => asset.path === 'configuration.json').sha256;
    const execute = await createRehearsalExecutor({ session, configuration: { root, nodeImage, rustImage, scoreBinary, retrieverBinary,
      manifestSha256: plan.manifest_sha256, driverSha256: configurationSha256, configurationSha256, serviceBuild: 'component-test-not-complete-driver' } });
    for (const condition of ['handover', 'lexical', 'vector', 'full-memory', 'no-memory']) {
      const observation = await execute({ case_id: 'supplier-handover-q1', condition, repeat: 0 });
      assert.equal(observation.work.condition, condition);
      assert.equal(observation.provider_requests, 0);
      assert.equal(observation.execution_microusd, 0);
      assert(observation.unresolved_rubric && !observation.result.success);
      assert.equal(observation.isolation.before_sha256, session.preparation.snapshot_sha256);
      assert.equal(observation.isolation.after_sha256, observation.isolation.before_sha256);
      assert.equal(observation.knowledge_review.packet.sources.length, 5);
      assert(observation.knowledge_review.packet.sources.every(source => source.id.includes('supplier-handover')));
      assert(observation.result.duration_ms > 0);
      if (condition === 'no-memory') assert(observation.knowledge_review.packet.answer.refused);
    }
    const coding = await execute({ case_id: 'durable-importer', condition: 'full-memory', repeat: 0 });
    assert(!coding.result.success, 'no-op agent must fail real hidden acceptance');
    assert.equal(coding.result.sub_results.length, 3);
    assert(coding.result.sub_results.every(step => !step.success));
    assert.equal(coding.knowledge_review, null);
    assert.equal(coding.isolation.after_sha256, session.preparation.snapshot_sha256);
    const source = manifest.cases[0].sources[0];
    await fs.appendFile(path.join(root, source), 'tampered');
    await assert.rejects(execute({ case_id: 'supplier-handover-q1', condition: 'handover', repeat: 1 }), /changed rehearsal input/);
  } finally { await session.close(); }
});
