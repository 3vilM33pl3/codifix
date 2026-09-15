// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { openPreparedSession } from './service-session.mjs';
import { openRequestMediator } from '../request-mediator.mjs';
import { preparedContext, runOfflineKnowledge } from './knowledge.mjs';

const snapshotBinary = process.env.MEMORY_R3_SNAPSHOT_BIN;
const memoryBinary = process.env.MEMORY_R3_CLI_BIN;
const databaseUrl = process.env.MEMORY_LAYER_TEST_DATABASE_URL;
const image = process.env.MEMORY_R3_TEST_IMAGE;
const scoreBinary = process.env.MEMORY_R3_SCORE_BIN;
const budgetBinary = process.env.MEMORY_R3_BUDGET_BIN;
const available = snapshotBinary && memoryBinary && databaseUrl && image && scoreBinary;
if (process.env.MEMORY_R3_REQUIRE_SERVICE_SESSION === '1' && !available) throw Error('required service-session configuration missing');
if (process.env.MEMORY_R3_REQUIRE_BUDGETED_SERVICE === '1' && (!available || !budgetBinary)) throw Error('required budgeted service configuration missing');

test('service embedding callbacks reserve each preparation/query call and stop before exceeding a task cap', { skip: !available || !budgetBinary, timeout: 120000 }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-budgeted-service-'));
  const limits = { requests: 5, tokens: 100, microusd: 50, task_seconds: 30 };
  const mediator = await openRequestMediator({ binary: budgetBinary, directory,
    manifestSha256: 'c'.repeat(64), limits, taskLimits: { ...limits, requests: 2 } });
  let session, calls = 0, requests = 0;
  const embedFor = taskId => (input, purpose, signal) => mediator.dispatch({
    signal,
    taskId, requestId: `embedding-${requests++}`,
    payload: { model: 'synthetic-v1', dimensions: 2, purpose, input },
    allowance: { input_tokens: 10, output_tokens: 0, microusd: 5 }, timeoutMs: 1000,
    invoke: async () => {
      const journal = await fs.readFile(path.join(directory, 'budget.jsonl'), 'utf8');
      assert.equal(journal.trim().split('\n').map(JSON.parse).at(-1).event, 'reserved');
      calls++;
      return { output: [1, 0], usage: { input_tokens: 3, output_tokens: 0, microusd: 1 } };
    },
  });
  try {
    session = await openPreparedSession({ snapshotBinary, memoryBinary, databaseUrl,
      semanticTime: '2026-01-01T00:00:00Z',
      documents: [{ project: 'r3-alpha', source: 'public/source.md', text: 'Reconcile acknowledgement before retry.' }],
      embedding: { model: 'synthetic-v1', dimensions: 2, embed: (input, purpose) => {
        assert.equal(purpose, 'document'); // A query may not spend preparation allowance.
        return embedFor('preparation')(input, purpose);
      } } });
    const queryEmbedding = embedFor('answer-sequence');
    for (let repeat = 0; repeat < 2; repeat++) await session.withInstance('r3-alpha', async instance => instance.query('acknowledgement'), { queryEmbedding });
    await assert.rejects(session.withInstance('r3-alpha', async instance => instance.query('acknowledgement'), { queryEmbedding }), /scoped disposable query failed/);
    assert.equal(calls, 3); // One preparation plus two queries; no fourth invocation.
    assert.deepEqual(await mediator.status(), { ledger: { requests: 3, tokens: 9, microusd: 3 }, uncertain_ids: [] });
    await session.withInstance('r3-alpha', async instance => instance.query('acknowledgement'), { queryEmbedding: embedFor('next-task') });
    assert.equal(calls, 4);
    assert.deepEqual((await mediator.status()).ledger, { requests: 4, tokens: 12, microusd: 4 });
  } finally {
    await session?.close();
    await mediator.close();
    await fs.rm(directory, { recursive: true });
  }
});

test('supplied production chunks reach configured semantic search without writeback or silent fallback', { skip: !available, timeout: 120000 }, async () => {
  const calls = [];
  let failQuery = false;
  const documents = [{ project: 'r3-alpha', source: 'public/incident.md', text: 'Reconcile settlement acknowledgement before retry.' },
    { project: 'r3-beta', source: 'public/private.md', text: 'BETA_ONLY must not enter alpha answers.' }];
  const session = await openPreparedSession({ snapshotBinary, memoryBinary, databaseUrl,
    semanticTime: '2026-01-01T00:00:00Z', documents,
    embedding: { model: 'synthetic-v1', dimensions: 3, embed: async (text, purpose) => {
      calls.push({ text, purpose });
      if (purpose === 'query' && failQuery) throw Error('synthetic secret provider body must not escape');
      return text.includes('BETA_ONLY') ? [0, 1, 0] : [1, 0, 0];
    } } });
  try {
    assert.equal(session.preparation.official_evidence, false);
    assert.equal(session.preparation.preparation_microusd, null);
    assert.equal(calls.length, 2);
    assert(calls[0].text.includes(documents[0].source));
    assert(calls[0].text.includes(documents[0].text));
    for (let repeat = 0; repeat < 2; repeat++) {
      const result = await session.withInstance('r3-alpha', async instance => {
        const answer = await instance.query('settlement acknowledgement');
        assert.equal(answer.diagnostics.semantic_status, 'active_space_ok');
        assert(answer.diagnostics.semantic_candidates > 0);
        assert(!JSON.stringify(answer).includes('BETA_ONLY'));
        return answer.results.length;
      });
      assert(result.value > 0);
      assert.equal(result.beforeSha256, result.afterSha256);
    }
    assert.deepEqual(calls.slice(2), Array(2).fill({ text: 'settlement acknowledgement', purpose: 'query' }));
    failQuery = true;
    await assert.rejects(session.withInstance('r3-alpha', async instance => instance.query('settlement acknowledgement')),
      error => error.message === 'scoped disposable query failed');
    failQuery = false;
    await session.withInstance('r3-alpha', async instance => instance.query('settlement acknowledgement'));
  } finally { await session.close(); }
  assert(!documents.some(d => Object.hasOwn(d, 'vectors')));
});

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
