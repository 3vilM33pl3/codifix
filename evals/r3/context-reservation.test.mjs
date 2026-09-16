// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createContextReservations } from './context-reservation.mjs';
import { createProviderBindings, validateProviderProfile, taskTokenLimit } from './provider-profile.mjs';
import { openRequestMediator } from './request-mediator.mjs';
import { dispatchModel } from './isolation/model-dispatch.mjs';

const binary = process.env.MEMORY_R3_BUDGET_BIN;
if (process.env.MEMORY_R3_REQUIRE_MEDIATOR === '1' && !binary) throw Error('required budget binary missing');
const source = await fs.readFile(new URL('./context-reservation.mjs', import.meta.url));
function fixture() {
  const pin = { path: 'context-reservation.mjs', sha256: createHash('sha256').update(source).digest('hex') };
  return {
    model: { provider: 'openai', resolved_id: 'gpt-5.4-mini-2026-03-17', reasoning: 'medium', embedding_id: 'text-embedding-3-small',
      max_input_tokens: 400000, max_output_tokens: 4096, input_microusd_per_million: 750000,
      output_microusd_per_million: 4500000, cached_input_microusd_per_million: 75000 },
    provider_budget: { max_calls_per_task: 3 }, preparation_budget: { requests: 10, tokens: 81920, microusd: 1640, task_seconds: 60 },
    assets: [{ ...pin, kind: 'configuration' }],
    provider_profile: { version: 2, protocol: 'openai-text-v1', input_accounting: 'full_context_reservation',
      responses_tokenizer: { id: 'full-context-responses-v1', asset: pin },
      embedding: { resolved_id: 'text-embedding-3-small', dimensions: 1536, max_input_tokens: 8192,
        input_microusd_per_million: 20000, tokenizer: { id: 'full-context-embeddings-v1', asset: pin } }, configuration_evidence: pin },
  };
}

test('full-context reservations cannot become an exact counter, cheaper model or partial window', () => {
  const f = fixture();
  validateProviderProfile(f);
  assert.equal(taskTokenLimit(f), 1212288);
  for (const mutate of [m => { m.provider_profile.version = 1; }, m => { delete m.provider_profile.input_accounting; },
    m => { m.model.resolved_id = 'gpt-5.4-mini'; }, m => { m.model.max_input_tokens = 32768; },
    m => { m.model.reasoning = 'max'; }, m => { m.model.input_microusd_per_million = 0; },
    m => { m.provider_profile.embedding.dimensions = 2; }, m => { m.provider_profile.embedding.max_input_tokens = 8191; },
    m => { delete m.provider_budget; }, m => { m.provider_profile.responses_tokenizer.id = 'fake-exact-tokenizer'; }]) {
    const invalid = structuredClone(f); mutate(invalid);
    assert.throws(() => validateProviderProfile(invalid));
  }
  const counters = createContextReservations(f.model, f.provider_profile);
  const body = { model: f.model.resolved_id, max_output_tokens: 4096, truncation: 'disabled', tools: [], input: 'Unicode: café 🌻' };
  assert.equal(counters.responsesCounter.countInputTokens(body), 400000);
  assert.throws(() => counters.responsesCounter.countInputTokens({ ...body, input: 'x'.repeat(131072) }));
  assert.throws(() => counters.responsesCounter.countInputTokens({ ...body, tools: [{}] }));
  const embedding = { model: f.model.embedding_id, dimensions: 1536, input: ['source'] };
  assert.equal(counters.embeddingCounter.countInputTokens(embedding), 8192);
  assert.throws(() => counters.embeddingCounter.countInputTokens({ ...embedding, input: ['x'.repeat(8193)] }));
  assert.throws(() => createProviderBindings({ manifest: f, responsesCounter: counters.responsesCounter }), /cannot override/);
});

for (const mode of ['settle', 'budget-denied', 'uncertain']) {
  test(`full-context ${mode} uses real durable reservations but fake provider results`, { skip: !binary }, async t => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-context-reservation-'));
    const f = fixture(), limits = { requests: 3, tokens: taskTokenLimit(f), microusd: mode === 'budget-denied' ? 318431 : 1000000, task_seconds: 30 };
    const mediator = await openRequestMediator({ binary, directory: root, manifestSha256: 'a'.repeat(64), limits, taskLimits: limits });
    t.after(async () => { await mediator.close(); await fs.rm(root, { recursive: true }); });
    let calls = 0;
    const binding = createProviderBindings({ manifest: f, mediator, taskId: 'work-00000', sendEmbeddings: () => assert.fail('not used'),
      sendResponses: async body => {
        calls++;
        const journal = await fs.readFile(path.join(root, 'budget.jsonl'), 'utf8');
        assert(journal.includes('"input_tokens":400000'));
        assert(journal.includes('"microusd":318432'));
        if (mode === 'uncertain') throw Error('synthetic lost response');
        return JSON.stringify({ id: 'resp_synthetic', object: 'response', status: 'completed', model: body.model,
          error: null, incomplete_details: null,
          output: [{ type: 'message', role: 'assistant', status: 'completed', content: [{ type: 'output_text', annotations: [],
            text: '{"text":"Insufficient evidence","refused":true,"citations":[]}' }] }],
          usage: { input_tokens: 100, output_tokens: 12, total_tokens: 112,
            input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } });
      } });
    const request = { model: { provider: 'openai', resolved_id: f.model.resolved_id, reasoning: f.model.reasoning },
      input: { question: 'Synthetic question', context: [] }, response_schema: 'r3-grounded-answer-v1', tools: [] };
    const dispatch = requestId => dispatchModel({ mediator, prepareRequest: binding.prepareRequest, taskId: 'work-00000', requestId,
      payload: request, timeoutMs: 1000 });
    const prepared = await binding.prepareRequest(request);
    assert.equal(prepared.payload.input_accounting, 'full_context_reservation');
    if (mode === 'settle') {
      assert.equal((await dispatch('first')).usage.microusd, 129);
      assert.equal((await dispatch('first')).invoked, false);
      assert.equal((await dispatch('second')).usage.microusd, 129);
      assert.equal(calls, 2);
      assert.deepEqual((await mediator.audit()).audit.ledger, { requests: 2, tokens: 224, microusd: 258 });
    } else {
      await assert.rejects(dispatch('first'));
      await assert.rejects(dispatch('second'));
      assert.equal(calls, mode === 'uncertain' ? 1 : 0);
      if (mode === 'uncertain') assert.deepEqual((await mediator.status()).uncertain_ids, ['first']);
    }
  });
}
