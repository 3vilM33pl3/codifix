// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { decodeResponsesReceipt } from './responses-receipt.mjs';

const contract = { model: 'synthetic-model-v1', maxInputTokens: 100, maxOutputTokens: 50 };
const fixture = () => ({
  id: 'resp_synthetic', object: 'response', status: 'completed', model: contract.model,
  error: null, incomplete_details: null,
  output: [
    { type: 'reasoning', summary: [{ text: 'private reasoning must not escape' }] },
    { type: 'message', role: 'assistant', status: 'completed', content: [
      { type: 'output_text', text: '{"kind":"finish"}', annotations: [] },
    ] },
  ],
  usage: { input_tokens: 20, output_tokens: 10, total_tokens: 30,
    input_tokens_details: { cached_tokens: 5 }, output_tokens_details: { reasoning_tokens: 7 } },
});
const decode = value => decodeResponsesReceipt(JSON.stringify(value), contract);

test('preserves usage once, cache detail and exact model without returning reasoning', () => {
  assert.deepEqual(decode(fixture()), {
    responseId: 'resp_synthetic', model: contract.model, output: { kind: 'finish' },
    tokenUsage: { input_tokens: 20, output_tokens: 10 }, cachedInputTokens: 5, reasoningOutputTokens: 7,
  });
});

test('unknown, inconsistent, excessive or novel usage fails closed', () => {
  for (const change of [
    r => { delete r.usage; }, r => { r.usage.total_tokens++; },
    r => { r.usage.input_tokens = 101; r.usage.total_tokens = 111; },
    r => { r.usage.output_tokens = 51; r.usage.total_tokens = 71; },
    r => { r.usage.input_tokens_details.cached_tokens = 21; },
    r => { r.usage.output_tokens_details.reasoning_tokens = 11; },
    r => { delete r.usage.input_tokens_details; },
    r => { r.usage.input_tokens_details.audio_tokens = 1; },
    r => { r.usage.tool_calls = 1; }, r => { r.usage.input_tokens = 0.5; },
  ]) { const r = fixture(); change(r); assert.throws(() => decode(r), /uncertain attempt/); }
});

test('truncated, switched-model, tool, refusal and malformed responses never become answers', () => {
  for (const change of [
    r => { r.status = 'incomplete'; }, r => { r.model = 'another-model'; },
    r => { r.error = { message: 'SENSITIVE PROVIDER ERROR' }; },
    r => { r.output.push({ type: 'function_call', arguments: 'SECRET' }); },
    r => { r.output[1].content[0] = { type: 'refusal', refusal: 'SECRET' }; },
    r => { r.output[1].content[0].text = 'SECRET non JSON'; },
    r => { r.output[1].content[0].text = '[]'; },
    r => { r.output[1].content[0].annotations.push({ type: 'url_citation' }); },
    r => { r.output.push(r.output[1]); }, r => { r.output = [r.output[0]]; },
    r => { r.output[1].status = 'in_progress'; },
  ]) {
    const r = fixture(); change(r);
    assert.throws(() => decode(r), error => !/SECRET|SENSITIVE/.test(error.message) && /uncertain attempt/.test(error.message));
  }
  assert.throws(() => decodeResponsesReceipt('x'.repeat(1024 * 1024 + 1), contract));
  assert.throws(() => decodeResponsesReceipt('{}', { ...contract, apiKey: 'must not be accepted' }));
});
