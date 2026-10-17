// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { openSyntheticWireProvider } from './synthetic-wire-provider.mjs';
import { syntheticEmbeddingModel, syntheticVector } from './synthetic-embedding.mjs';
import { openRequestMediator } from './request-mediator.mjs';
import { dispatchModel } from './isolation/model-dispatch.mjs';

const binary = process.env.CODIFIX_R3_BUDGET_BIN;
if (process.env.CODIFIX_R3_REQUIRE_MEDIATOR === '1' && !binary) throw Error('required budget binary missing');
const tokenizer = { id: 'synthetic-wire-counter-not-real-tokenizer-v1', asset: { path: 'counter.mjs', sha256: 'a'.repeat(64) } };
const profile = () => ({ smoke_only: true, label_status: 'draft', limits: { microusd: 0 },
  assets: [{ ...tokenizer.asset, kind: 'configuration' }],
  provider_profile: { version: 1, protocol: 'openai-text-v1', responses_tokenizer: tokenizer,
    embedding: { resolved_id: syntheticEmbeddingModel, dimensions: 16, max_input_tokens: 8192,
      input_microusd_per_million: 0, tokenizer }, configuration_evidence: tokenizer.asset },
  provider_budget: { max_calls_per_task: 7 }, preparation_budget: { requests: 10 }, model: {
    provider: 'openai', resolved_id: 'synthetic-mediated-excerpt-noop-v1', embedding_id: syntheticEmbeddingModel,
    reasoning: 'none', max_input_tokens: 100, max_output_tokens: 20,
    input_microusd_per_million: 0, output_microusd_per_million: 0 } });

test('fake wire mode refuses real models, spending or approved/held-out evidence profiles', async () => {
  for (const change of [p => { p.smoke_only = false; }, p => { p.label_status = 'reviewed'; },
    p => { p.limits.microusd = 1; }, p => { p.model.resolved_id = 'real-provider-model'; },
    p => { p.model.embedding_id = 'real-embedding-model'; }, p => { delete p.preparation_budget; },
    p => { p.model.input_microusd_per_million = 1; }]) {
    const p = profile(); change(p);
    await assert.rejects(openSyntheticWireProvider(p), /synthetic zero-spend/);
  }
});

test('one HTTP provider spans preparation, question embeddings and structured answers with audited recovery', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-wire-provider-'));
  const limits = { requests: 10, tokens: 1000, microusd: 0, task_seconds: 30 };
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'a'.repeat(64), limits, taskLimits: limits });
  const manifest = profile(), wire = await openSyntheticWireProvider(manifest);
  try {
    const preparation = wire.bind(mediator, 'preparation');
    assert.deepEqual(await preparation.embed('synthetic source', 'document'), syntheticVector('synthetic source'));
    await assert.rejects(preparation.embed('question', 'query'), /phase mismatch/);
    const task = wire.bind(mediator, 'work-00000');
    await task.embed('question', 'query');
    await assert.rejects(task.embed('source', 'document'), /phase mismatch/);
    const payload = { model: { provider: manifest.model.provider, resolved_id: manifest.model.resolved_id, reasoning: manifest.model.reasoning },
      input: { question: 'question', context: [{ id: 'source', text: 'synthetic source' }] },
      response_schema: 'r3-grounded-answer-v1', tools: [] };
    const ask = () => dispatchModel({ mediator, prepareRequest: task.prepareRequest, taskId: 'work-00000', requestId: 'answer', payload, timeoutMs: 2000 });
    assert.equal((await ask()).output.text, 'synthetic source');
    assert.equal((await ask()).invoked, false);
    assert.equal(wire.calls, 3);
    const audit = (await mediator.audit()).audit;
    assert.deepEqual(audit.ledger, { requests: 3, tokens: 4, microusd: 0 });
    assert.equal(audit.tasks.preparation.requests, 1);
    assert.equal(audit.tasks['work-00000'].requests, 2);
    assert.equal((await mediator.recover('answer')).output.provider.model, manifest.model.resolved_id);
    const cancelled = new AbortController(); cancelled.abort();
    await assert.rejects(task.embed('must not send', 'query', cancelled.signal), /cancelled/);
    assert.equal(wire.calls, 3);
  } finally { await wire.close(); await mediator.close(); await fs.rm(directory, { recursive: true }); }
  assert.throws(() => wire.bind(mediator, 'work-00001'), /identity/);
});
