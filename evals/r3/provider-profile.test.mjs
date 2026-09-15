// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createProviderBindings, validateProviderProfile } from './provider-profile.mjs';
import { openRequestMediator } from './request-mediator.mjs';
import { dispatchModel } from './isolation/model-dispatch.mjs';

const binary = process.env.MEMORY_R3_BUDGET_BIN;
if (process.env.MEMORY_R3_REQUIRE_MEDIATOR === '1' && !binary) throw Error('required budget binary missing');
function fixture() {
  const responses = { id: 'synthetic-response-counter', asset: { path: 'response-counter.mjs', sha256: 'a'.repeat(64) } };
  const embeddings = { id: 'synthetic-embedding-counter', asset: { path: 'embedding-counter.mjs', sha256: 'b'.repeat(64) } };
  const evidence = { path: 'private/model-review.json', sha256: 'c'.repeat(64) };
  return {
    model: { provider: 'openai', resolved_id: 'synthetic-answer', reasoning: 'none', embedding_id: 'synthetic-embedding',
      max_input_tokens: 100, max_output_tokens: 20, input_microusd_per_million: 2_000_000, output_microusd_per_million: 5_000_000 },
    assets: [responses.asset, embeddings.asset, evidence].map(pin => ({ ...pin, kind: 'configuration' })),
    provider_profile: { version: 1, protocol: 'openai-text-v1', responses_tokenizer: responses,
      embedding: { resolved_id: 'synthetic-embedding', dimensions: 2, max_input_tokens: 100,
        input_microusd_per_million: 11_000_000, tokenizer: embeddings }, configuration_evidence: evidence },
  };
}

test('shared profile refuses mismatched assets, models, numbers and undocumented wire options', () => {
  validateProviderProfile(fixture());
  for (const mutate of [m => { m.provider_profile.embedding.resolved_id = 'other'; },
    m => { m.assets[0].sha256 = 'd'.repeat(64); }, m => { m.assets[2].kind = 'source'; },
    m => { m.provider_profile.embedding.dimensions = 8193; }, m => { m.model.max_input_tokens = Number.MAX_SAFE_INTEGER; },
    m => { m.model.cached_input_microusd_per_million = 3_000_000; },
    m => { m.provider_profile.endpoint = 'https://unapproved.invalid'; },
    m => { m.provider_profile.embedding.tokenizer.id = '../counter'; },
    m => { m.provider_profile.configuration_evidence.path = '/private/model-review.json'; }]) {
    const manifest = fixture(); mutate(manifest);
    assert.throws(() => validateProviderProfile(manifest), /locked provider profile/);
  }
});

test('response and embedding calls retain distinct pinned counters and actual prices', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-provider-profile-'));
  const limits = { requests: 10, tokens: 1000, microusd: 1000, task_seconds: 30 };
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'e'.repeat(64), limits, taskLimits: limits });
  const manifest = fixture();
  const makeCounter = expected => ({ id: expected.id, sha256: expected.asset.sha256, countInputTokens: () => 10 });
  let responseCalls = 0, embeddingCalls = 0;
  const options = { manifest, mediator,
    responsesCounter: makeCounter(manifest.provider_profile.responses_tokenizer),
    embeddingCounter: makeCounter(manifest.provider_profile.embedding.tokenizer),
    sendResponses: async body => {
      responseCalls++; assert.equal(body.model, 'synthetic-answer');
      return JSON.stringify({ id: 'resp_synthetic', object: 'response', status: 'completed', model: body.model,
        error: null, incomplete_details: null, output: [{ type: 'message', role: 'assistant', status: 'completed',
          content: [{ type: 'output_text', annotations: [], text: '{"text":"Insufficient evidence","refused":true,"citations":[]}' }] }],
        usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2,
          input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } });
    },
    sendEmbeddings: async body => {
      embeddingCalls++; assert.equal(body.model, 'synthetic-embedding');
      return JSON.stringify({ object: 'list', model: body.model,
        data: [{ object: 'embedding', index: 0, embedding: [0.5, 1] }], usage: { prompt_tokens: 1, total_tokens: 1 } });
    },
  };
  try {
    assert.throws(() => createProviderBindings({ ...options, taskId: 'work-00000', responsesCounter: options.embeddingCounter }), /counter/);
    const preparation = createProviderBindings({ ...options, taskId: 'preparation' });
    await assert.rejects(preparation.prepareRequest({}), /phase/);
    assert.deepEqual(await preparation.embed('document', 'document'), [0.5, 1]);
    const task = createProviderBindings({ ...options, taskId: 'work-00000' });
    const model = { provider: 'openai', resolved_id: 'synthetic-answer', reasoning: 'none' };
    // Existing bindings are immutable even if a caller mutates its source configuration.
    manifest.provider_profile.embedding.input_microusd_per_million = 0;
    manifest.model.input_microusd_per_million = 0;
    await task.embed('question', 'query');
    const result = await dispatchModel({ mediator, prepareRequest: task.prepareRequest, taskId: 'work-00000', requestId: 'answer',
      payload: { model, input: { question: 'question', context: [] }, response_schema: 'r3-grounded-answer-v1', tools: [] }, timeoutMs: 2000 });
    assert.equal(result.usage.microusd, 7);
    const audit = (await mediator.audit()).audit;
    assert.deepEqual(audit.tasks.preparation, { requests: 1, input_tokens: 1, output_tokens: 0, microusd: 11 });
    assert.deepEqual(audit.tasks['work-00000'], { requests: 2, input_tokens: 2, output_tokens: 1, microusd: 18 });
    assert.equal(responseCalls, 1); assert.equal(embeddingCalls, 2);
    assert.equal(task.embeddingProfile.dimensions, 2);
    assert.equal(task.embeddingProfile.model, 'synthetic-embedding');
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});
