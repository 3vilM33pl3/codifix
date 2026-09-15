// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import { prepareEmbeddingsRequest, createEmbeddingsPreparation } from './embeddings-call.mjs';
import { createProviderTransport } from './provider-transport.mjs';
import { openRequestMediator } from './request-mediator.mjs';
import { dispatchModel } from './isolation/model-dispatch.mjs';
import { openEmbeddingProxy } from './isolation/embedding-proxy.mjs';

const model = { provider: 'openai', resolved_id: 'synthetic-embedding-v1', reasoning: 'none', embedding_id: 'synthetic-embedding-v1',
  max_input_tokens: 100, max_output_tokens: 1, input_microusd_per_million: 1000000, output_microusd_per_million: 0 };
const tokenizer = { id: 'synthetic-counter', sha256: 'a'.repeat(64) };
const wire = () => ({ object: 'list', model: model.resolved_id,
  data: [{ object: 'embedding', index: 0, embedding: [0.1, 1] }], usage: { prompt_tokens: 3, total_tokens: 3 } });
const binary = process.env.MEMORY_R3_BUDGET_BIN;
if (process.env.MEMORY_R3_REQUIRE_EMBEDDINGS === '1' && !binary) throw Error('required budget binary missing');

test('embedding codec binds one text, model, dimensions and complete input-only usage', () => {
  const codec = prepareEmbeddingsRequest({ model: model.resolved_id, dimensions: 2, input: 'synthetic text', maxInputTokens: 10 });
  assert.deepEqual(codec.body, { model: model.resolved_id, dimensions: 2, input: ['synthetic text'], encoding_format: 'float' });
  assert.deepEqual(codec.decode(JSON.stringify(wire())), { vector: [Math.fround(0.1), 1], inputTokens: 3 });
  for (const change of [r => { r.model = 'other'; }, r => { r.data.push(r.data[0]); }, r => { r.data[0].index = 1; },
    r => { r.data[0].embedding = [0, 0]; }, r => { r.data[0].embedding = [1e100, 1]; },
    r => { r.usage.total_tokens = 4; }, r => { r.usage.prompt_tokens = r.usage.total_tokens = 11; },
    r => { r.usage.cached_tokens = 1; }, r => { delete r.usage; }]) {
    const receipt = wire(); change(receipt);
    assert.throws(() => codec.decode(JSON.stringify(receipt)), /invalid embedding receipt/);
  }
  assert.throws(() => codec.decode('private provider error'), /invalid embedding receipt/);
  for (const input of ['', ' ', 'x'.repeat(32769), ['text']]) {
    assert.throws(() => prepareEmbeddingsRequest({ model: model.resolved_id, dimensions: 2, input, maxInputTokens: 10 }));
  }
});

test('embedding counters fail before quoting or sending and sanitize asynchronous failures', async () => {
  for (const countInputTokens of [() => { throw Error('PRIVATE'); }, () => Promise.reject(Error('PRIVATE')), () => 0, () => 101]) {
    const prepare = createEmbeddingsPreparation({ model, dimensions: 2, tokenizer, countInputTokens,
      mediator: { price: () => assert.fail('must not quote') }, send: () => assert.fail('must not send') });
    await assert.rejects(prepare({ input: 'text', purpose: 'query' }), e => !e.message.includes('PRIVATE'));
  }
});

test('prepared embedding contracts reject altered payloads and token undercounts', async () => {
  let calls = 0;
  const prepare = createEmbeddingsPreparation({ model, dimensions: 2, tokenizer, countInputTokens: () => 2,
    mediator: { price: async ({ inputTokens, outputTokens }) => ({ input_tokens: inputTokens, output_tokens: outputTokens, microusd: inputTokens }) },
    send: async () => { calls++; return JSON.stringify(wire()); } });
  const prepared = await prepare({ input: 'text', purpose: 'document' });
  const changed = structuredClone(prepared.payload); changed.purpose = 'query';
  await assert.rejects(prepared.invoke(changed, new AbortController().signal), /changed prepared/);
  assert.equal(calls, 0);
  await assert.rejects(prepared.invoke(prepared.payload, new AbortController().signal), /exceeded quote/);
  assert.equal(calls, 1);
});

test('embedding receipts flow through durable preparation/query budgets and a real query proxy', { skip: !binary }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-embedding-receipts-'));
  const limits = { requests: 5, tokens: 100, microusd: 100, task_seconds: 10 };
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'b'.repeat(64), limits, taskLimits: limits });
  let calls = 0, invalid = false;
  const server = http.createServer(async (req, res) => {
    calls++;
    assert.equal(req.url, '/v1/embeddings');
    assert.equal(req.headers.authorization, 'Bearer synthetic-embedding-test');
    let bytes = ''; for await (const chunk of req) bytes += chunk;
    const request = JSON.parse(bytes);
    assert.equal(request.model, model.resolved_id);
    assert.equal(request.encoding_format, 'float');
    const journal = await fs.readFile(path.join(directory, 'budget.jsonl'), 'utf8');
    assert.equal(journal.trim().split('\n').map(JSON.parse).at(-1).event, 'reserved');
    const receipt = wire(); if (invalid) delete receipt.usage;
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(receipt));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  let proxy;
  try {
    const endpoint = `http://127.0.0.1:${server.address().port}/v1/embeddings`;
    assert.throws(() => createProviderTransport({ endpoint, bearer: 'synthetic-embedding-test' }), /invalid provider/);
    const send = createProviderTransport({ endpoint, bearer: 'synthetic-embedding-test', api: 'embeddings' });
    const prepareRequest = createEmbeddingsPreparation({ mediator, model, dimensions: 2, tokenizer,
      countInputTokens: body => { assert.deepEqual(Object.keys(body).sort(), ['dimensions', 'encoding_format', 'input', 'model']); return 10; }, send });
    const embed = async (taskId, requestId, input, purpose, signal) => (await dispatchModel({ mediator, prepareRequest,
      taskId, requestId, payload: { input, purpose }, timeoutMs: 2000, signal })).output;
    const document = await embed('preparation', 'document-1', 'approved source', 'document');
    assert.deepEqual(await embed('preparation', 'document-1', 'approved source', 'document'), document);
    assert.equal(calls, 1);
    proxy = await openEmbeddingProxy({ model: model.resolved_id, dimensions: 2, embed: () => assert.fail('task must not use preparation') });
    await proxy.query('question', async () => {
      const response = await fetch(proxy.baseUrl + '/embeddings', { method: 'POST',
        headers: { Authorization: `Bearer ${proxy.token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ model: model.resolved_id, input: ['question'] }) });
      assert.equal(response.status, 200);
      assert.deepEqual((await response.json()).data[0].embedding, document);
      return { diagnostics: { semantic_status: 'active_space_ok' } };
    }, (input, purpose, signal) => embed('task-1', 'query-1', input, purpose, signal));
    assert.equal(calls, 2);
    const audit = (await mediator.audit()).audit;
    assert.deepEqual(audit.tasks.preparation, { requests: 1, input_tokens: 3, output_tokens: 0, microusd: 3 });
    assert.deepEqual(audit.tasks['task-1'], audit.tasks.preparation);
    invalid = true;
    await assert.rejects(embed('task-2', 'query-2', 'question', 'query'), /uncertain/);
    await assert.rejects(embed('task-2', 'query-2', 'question', 'query'), /rejected/);
    assert.equal(calls, 3);
    assert.deepEqual((await mediator.status()).uncertain_ids, ['query-2']);
  } finally {
    await proxy?.close(); await mediator.close(); server.closeAllConnections();
    await new Promise(resolve => server.close(resolve)); await fs.rm(directory, { recursive: true });
  }
});
