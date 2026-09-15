// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import { openRequestMediator } from './request-mediator.mjs';
import { createResponsesPreparation } from './responses-call.mjs';
import { createProviderTransport } from './provider-transport.mjs';
import { dispatchModel } from './isolation/model-dispatch.mjs';
const binary = process.env.MEMORY_R3_BUDGET_BIN;
if (process.env.MEMORY_R3_REQUIRE_MEDIATOR === '1' && !binary) throw Error('required budget binary missing');
const model = { provider: 'openai', resolved_id: 'synthetic-model-v1', reasoning: 'high', embedding_id: 'fake',
  max_input_tokens: 100, max_output_tokens: 20, input_microusd_per_million: 2000000,
  output_microusd_per_million: 4000000, cached_input_microusd_per_million: 500000 };
const payload = { model: { provider: model.provider, resolved_id: model.resolved_id, reasoning: model.reasoning },
  input: { question: 'Synthetic question', context: [] }, response_schema: 'r3-grounded-answer-v1', tools: [] };

for (const scenario of ['success', 'undercount', 'unknown-cache-price']) {
  test(`prepared wire request ${scenario} binds exact bytes and durable billing evidence`, { skip: !binary }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-responses-call-'));
    const limits = { requests: 3, tokens: 300, microusd: 1000, task_seconds: 10 };
    const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'e'.repeat(64), limits, taskLimits: limits });
    let calls = 0, counted;
    const server = http.createServer(async (req, res) => {
      calls++;
      let bytes = ''; for await (const chunk of req) bytes += chunk;
      assert.deepEqual(JSON.parse(bytes), counted);
      assert((await fs.readFile(path.join(directory, 'budget.jsonl'), 'utf8')).includes('"event":"reserved"'));
      const input = scenario === 'undercount' ? 31 : 20;
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ id: 'resp_synthetic', object: 'response', status: 'completed', model: model.resolved_id,
        error: null, incomplete_details: null,
        output: [{ type: 'message', role: 'assistant', status: 'completed', content: [
          { type: 'output_text', annotations: [], text: '{"text":"Insufficient evidence","refused":true,"citations":[]}' }] }],
        usage: { input_tokens: input, output_tokens: 10, total_tokens: input + 10,
          input_tokens_details: { cached_tokens: 5 }, output_tokens_details: { reasoning_tokens: 4 } } }));
    });
    server.listen(0, '127.0.0.1'); await once(server, 'listening');
    try {
      const locked = { ...model };
      if (scenario === 'unknown-cache-price') delete locked.cached_input_microusd_per_million;
      const prepareRequest = createResponsesPreparation({ mediator, model: locked,
        tokenizer: { id: 'synthetic-test-counter-not-real-tokenizer', sha256: 'a'.repeat(64) },
        countInputTokens: body => { counted = body; return 30; },
        send: createProviderTransport({ endpoint: `http://127.0.0.1:${server.address().port}/v1/responses`, bearer: 'synthetic-test' }) });
      const dispatch = requestId => dispatchModel({ mediator, prepareRequest, taskId: 'task', requestId, payload, timeoutMs: 2000 });
      if (scenario === 'success') {
        const result = await dispatch('first');
        assert.equal(result.output.refused, true);
        assert.deepEqual(result.usage, { input_tokens: 20, output_tokens: 10, microusd: 73 });
        assert.equal((await dispatch('first')).invoked, false);
        assert.equal(calls, 1);
        const saved = await mediator.recover('first');
        assert.equal(saved.output.provider.cached_input_tokens, 5);
        assert.equal(saved.output.provider.reasoning_output_tokens, 4);
        assert.equal((await mediator.audit()).audit.ledger.microusd, 73);
      } else {
        await assert.rejects(dispatch('first'), /uncertain/);
        await assert.rejects(dispatch('second'), /rejected/);
        assert.equal(calls, 1);
        assert.deepEqual((await mediator.status()).uncertain_ids, ['first']);
      }
      const invalid = createResponsesPreparation({ mediator, model, tokenizer: { id: 'fake', sha256: 'b'.repeat(64) },
        countInputTokens: () => 101, send: () => assert.fail('no dispatch for excessive input') });
      await assert.rejects(invalid(payload), /excessive input/);
    } finally {
      await mediator.close(); server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
      await fs.rm(directory, { recursive: true });
    }
  });
}

test('preparation cannot replace task identities or consume the entire deadline', async () => {
  const mediator = { dispatch: () => assert.fail('expired preparation must not dispatch') };
  await assert.rejects(dispatchModel({ mediator, payload: {}, timeoutMs: 1,
    prepareRequest: async () => { await new Promise(resolve => setTimeout(resolve, 10));
      return { payload: {}, allowance: {}, invoke() {}, decodeStored() {} }; } }), /expired/);
});
