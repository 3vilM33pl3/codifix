// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { openPreparedSession } from './service-session.mjs';
import { preparedContext, runOfflineKnowledge } from './knowledge.mjs';

const snapshotBinary = process.env.MEMORY_R3_SNAPSHOT_BIN;
const memoryBinary = process.env.MEMORY_R3_CLI_BIN;
const databaseUrl = process.env.MEMORY_LAYER_TEST_DATABASE_URL;
const image = process.env.MEMORY_R3_TEST_IMAGE;
const scoreBinary = process.env.MEMORY_R3_SCORE_BIN;
const available = snapshotBinary && memoryBinary && databaseUrl && image && scoreBinary;
if (process.env.MEMORY_R3_REQUIRE_SERVICE_SESSION === '1' && !available) throw Error('required service-session configuration missing');

test('prepared service session resets each answer case and retains no exposed credentials', { skip: !available, timeout: 120000 }, async () => {
  const documents = [{ project: 'r3-alpha', source: 'public/incident.md', text: 'Reconcile settlement acknowledgement before retry.' },
    { project: 'r3-beta', source: 'public/beta.md', text: 'BETA_ONLY never included for alpha.' }];
  const session = await openPreparedSession({ snapshotBinary, memoryBinary, databaseUrl,
    semanticTime: '2026-01-01T00:00:00Z', documents });
  let savedQuery, firstInput;
  try {
    assert.equal(session.preparation.official_evidence, false);
    const ids = [];
    for (let repeat = 0; repeat < 2; repeat++) {
      const result = await session.withInstance('r3-alpha', async instance => {
        assert(!JSON.stringify(instance).includes('database_url'));
        assert(!JSON.stringify(instance).includes('project_tokens'));
        savedQuery = instance.query;
        await assert.rejects(session.withInstance('r3-alpha', async () => {}), /busy/);
        await assert.rejects(session.close(), /active/);
        const retrieval = await instance.query('settlement acknowledgement');
        assert(!JSON.stringify(retrieval).includes('BETA_ONLY'));
        const sources = [{ id: documents[0].source, text: documents[0].text }];
        const context = preparedContext({ condition: 'full-memory', project: 'r3-alpha', sources,
          retrieval, bindings: instance.bindings });
        const question = 'What should happen before retry?';
        const answer = await runOfflineKnowledge({ image, scoreBinary, sources, context, question,
          manifestSha256: '1'.repeat(64), work: { case_id: 'reset-q1', condition: 'full-memory', repeat },
          rubric: { case_id: 'reset-q1', question, minimum_citations: 1, citations_critical: true,
            criteria: ['factual_correctness', 'required_evidence', 'appropriate_refusal'].map((dimension, i) => ({
              id: `criterion-${i}`, dimension, instruction: 'Synthetic pending reviewer criterion.', critical: false })) },
          argv: ['/usr/local/bin/node', '-e', `
            const fs=require('fs'),assert=require('assert');
            assert(!fs.existsSync('answer.json'));
            const input=JSON.parse(fs.readFileSync('input.json'));
            assert(!JSON.stringify(input).includes('BETA_ONLY'));
            const s=input.context[0];
            fs.writeFileSync('answer.json',JSON.stringify({text:s.text,refused:false,citations:[{source_id:s.id,quote:s.text}]}));
          `] });
        assert(answer.assessment.citations_valid && answer.assessment.unresolved);
        return answer.publicInputSha256;
      });
      ids.push(result.instanceId);
      assert.equal(result.beforeSha256, session.preparation.snapshot_sha256);
      assert.equal(result.afterSha256, result.beforeSha256);
      if (repeat) assert.equal(result.value, firstInput);
      else firstInput = result.value;
      await assert.rejects(savedQuery('settlement'), /invalid scoped/);
    }
    assert.notEqual(ids[0], ids[1]);
    await assert.rejects(session.withInstance('unknown-project', async () => {}), /unknown prepared project/);
    await assert.rejects(session.withInstance('r3-alpha', async () => { throw Error('synthetic interrupted task'); }), /synthetic interrupted task/);
    const recovered = await session.withInstance('r3-alpha', async instance => (await instance.query('settlement')).results.length);
    assert(recovered.value > 0);
    assert.equal(recovered.beforeSha256, recovered.afterSha256);
  } finally { await session.close(); }
  await assert.rejects(session.withInstance('r3-alpha', async () => {}), /unavailable/);
});
