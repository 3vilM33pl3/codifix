// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { openRequestMediator } from '../request-mediator.mjs';
import { runMediatedKnowledge } from './model-answer.mjs';
import { createResponsesPreparation } from '../responses-call.mjs';

const binary = process.env.CODIFIX_R3_BUDGET_BIN, scoreBinary = process.env.CODIFIX_R3_SCORE_BIN;
if (process.env.CODIFIX_R3_REQUIRE_MODEL_ANSWER === '1' && (!binary || !scoreBinary)) throw Error('required mediated answer configuration missing');
const sources = [{ id: 'public/one.md', text: 'Reconcile committed settlement before retry.' },
  { id: 'public/two.md', text: 'UNSELECTED_PRIVATE_SOURCE must not reach this model.' }];
const rubric = { case_id: 'synthetic-question', question: 'What should precede retry?',
  minimum_citations: 1, citations_critical: true,
  criteria: ['factual_correctness', 'required_evidence', 'appropriate_refusal'].map((dimension, i) => ({
    id: `criterion-${i}`, dimension, instruction: 'HIDDEN_RUBRIC_TEXT requires human review.', critical: false })) };
const model = { provider: 'synthetic', resolved_id: 'fake-answer-v1', reasoning: 'none' };
const allowance = { input_tokens: 100, output_tokens: 100, microusd: 10 };

test('prepared Responses knowledge retains private adjudication and exact priced receipts', { skip: !binary || !scoreBinary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-prepared-answer-'));
  const limits = { requests: 5, tokens: 2000, microusd: 1000, task_seconds: 300 };
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'b'.repeat(64), limits, taskLimits: limits });
  const profile = { provider: 'openai', resolved_id: 'synthetic-answer-v1', reasoning: 'none' };
  const inputs = [];
  try {
    const prepareRequest = createResponsesPreparation({ mediator,
      model: { ...profile, embedding_id: 'fake', max_input_tokens: 100, max_output_tokens: 100,
        input_microusd_per_million: 1000000, output_microusd_per_million: 1000000 },
      tokenizer: { id: 'synthetic-counter', sha256: 'b'.repeat(64) }, countInputTokens: () => 100,
      send: async body => {
        const serialized = JSON.stringify(body);
        assert(!serialized.includes('UNSELECTED_PRIVATE_SOURCE') && !serialized.includes('HIDDEN_RUBRIC_TEXT') && !serialized.includes(directory));
        inputs.push(body);
        const source = JSON.parse(body.input[0].content[0].text).context[0];
        const answer = source ? { text: source.text, refused: false, citations: [{ source_id: source.id, quote: source.text }] }
          : { text: 'Insufficient supplied evidence.', refused: true, citations: [] };
        return JSON.stringify({ id: `resp_fake${inputs.length}`, object: 'response', status: 'completed', model: profile.resolved_id,
          error: null, incomplete_details: null,
          output: [{ type: 'message', role: 'assistant', status: 'completed', content: [
            { type: 'output_text', annotations: [], text: JSON.stringify(answer) }] }],
          usage: { input_tokens: 7, output_tokens: 5, total_tokens: 12,
            input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } });
      } });
    for (const condition of ['handover', 'lexical', 'vector', 'full-memory', 'no-memory']) {
      const result = await runMediatedKnowledge({ mediator: { ...mediator, dispatch: request => {
        assert(request.timeoutMs > 119000 && request.timeoutMs <= 120000);
        return mediator.dispatch(request);
      } }, prepareRequest, model: profile, timeoutMs: 120000,
        taskId: condition, requestId: condition, scoreBinary, sources, rubric, question: rubric.question,
        context: condition === 'no-memory' ? [] : [sources[0]], manifestSha256: 'b'.repeat(64),
        work: { case_id: rubric.case_id, condition, repeat: 0 } });
      assert.equal(result.executionMicrousd, 12);
      assert.equal(result.assessment.success, false);
      assert.equal(result.officialEvidence, false);
      if (condition !== 'no-memory') assert(result.assessment.citations_valid && result.assessment.unresolved);
    }
    for (const body of inputs.slice(1, 4)) assert.deepEqual(body, inputs[0]);
    assert.equal((await mediator.audit()).audit.ledger.microusd, 60);
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});

test('all five conditions use one tool-free model contract and private adjudication with exact call accounting', { skip: !binary || !scoreBinary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-model-answer-'));
  const limits = { requests: 6, tokens: 2000, microusd: 100, task_seconds: 20 };
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: '2'.repeat(64), limits, taskLimits: limits });
  const payloads = [];
  const invoke = async payload => {
    payloads.push(structuredClone(payload));
    const raw = JSON.stringify(payload);
    assert(!raw.includes('HIDDEN_RUBRIC_TEXT'));
    assert(!raw.includes('UNSELECTED_PRIVATE_SOURCE'));
    assert(!raw.includes(directory));
    assert(!raw.includes('condition'));
    assert.deepEqual(payload.tools, []);
    const source = payload.input.context[0];
    return { output: source ? { text: source.text, refused: false, citations: [{ source_id: source.id, quote: source.text }] }
      : { text: 'Insufficient supplied evidence.', refused: true, citations: [] },
    usage: { input_tokens: 7, output_tokens: 5, microusd: 3 } };
  };
  const execute = (condition, changes = {}) => runMediatedKnowledge({ mediator, invoke, model, allowance,
    taskId: `task-${condition}`, requestId: `request-${condition}`, scoreBinary, sources, rubric,
    question: rubric.question, context: condition === 'no-memory' ? [] : [sources[0]],
    manifestSha256: '2'.repeat(64), work: { case_id: rubric.case_id, condition, repeat: 0 }, ...changes });
  try {
    for (const condition of ['handover', 'lexical', 'vector', 'full-memory', 'no-memory']) {
      const result = await execute(condition);
      assert.equal(result.providerRequests, 1);
      assert.equal(result.newProviderRequests, 1);
      assert.equal(result.executionMicrousd, 3);
      assert.deepEqual(result.tokenUsage, { input_tokens: 7, output_tokens: 5 });
      assert.equal(result.kind, 'mediated-knowledge-answer');
      assert.equal(result.image, null); // No false claim that an external model ran inside our image.
      assert.equal(result.officialEvidence, false);
      assert.equal(result.assessment.success, false);
      assert.equal(result.evidence.packet.sources.length, 2);
      if (condition !== 'no-memory') assert(result.assessment.citations_valid && result.assessment.unresolved);
    }
    for (const payload of payloads.slice(1, 4)) assert.deepEqual(payload, payloads[0]);
    const repeated = await execute('lexical');
    assert.equal(payloads.length, 5);
    assert.equal(repeated.providerRequests, 1);
    assert.equal(repeated.newProviderRequests, 0);
    assert.equal(repeated.executionMicrousd, 3);
    await assert.rejects(execute('lexical', { context: [{ ...sources[0], text: 'changed' }] }), /context differs/);
    assert.equal(payloads.length, 5);
    const bad = await execute('vector', { requestId: 'bad-citation', invoke: async () => ({
      output: { text: 'Unsupported claim.', refused: false, citations: [{ source_id: sources[0].id, quote: 'Invented evidence.' }] },
      usage: { input_tokens: 7, output_tokens: 5, microusd: 3 },
    }) });
    assert.equal(bad.assessment.citations_valid, false);
    assert.equal(bad.assessment.success, false);
    assert.equal(bad.providerRequests, 1);
    assert.deepEqual((await mediator.status()).ledger, { requests: 6, tokens: 72, microusd: 18 });
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});
