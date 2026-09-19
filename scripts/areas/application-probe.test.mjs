// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { checkResults, probeApplication, loopbackRequest } from './application-probe.mjs';
const ids = Array.from({ length: 4 }, (_, i) => `00000000-0000-4000-8000-00000000000${i + 1}`);
const input = { allowedIds: new Set([ids[0]]), ownId: ids[0], foreignIds: ids.slice(1),
  project: 'synthetic', question: 'PRIVATE_QUESTION', token: 'synthetic-one', foreignToken: 'synthetic-two' };
const answer = () => ({ results: [{ memory_id: ids[0], text: 'PRIVATE_CONTENT' }],
  answer_citations: [{ memory_id: ids[0] }], answer_generation: { method: 'deterministic' } });
const request = async (path, options) => {
  if (options.token !== input.token) return { status: 401, body: {} };
  if (path === '/v1/query') { assert.equal(options.body.answer_mode, 'deterministic');
    assert.equal(options.body.retrieval_mode, 'lexical'); return { status: 200, body: answer() }; }
  if (path === '/v1/capture/task') return { status: 403, body: {} };
  return path.endsWith(ids[0]) ? { status: 200, body: { id: ids[0] } } : { status: 404, body: {} };
};
test('real-path probe returns only sanitized counts', async () => {
  const result = await probeApplication(input, request, async () => answer());
  assert.equal(result.status, 'passed'); assert.equal(result.denied_foreign_areas, 3);
  assert.doesNotMatch(JSON.stringify(result), /PRIVATE_|synthetic-one|00000000/);
});
test('foreign results, citations and provider execution fail closed', () => {
  for (const change of [a => { a.results[0].memory_id = ids[1]; },
    a => { a.answer_citations[0].memory_id = ids[1]; },
    a => { a.answer_generation.method = 'llm'; },
    a => { a.answer_generation.token_usage = { total_tokens: 1 }; },
    a => { a.results = []; }]) {
    const value = answer(); change(value); assert.throws(() => checkResults(value, input.allowedIds), /probe failed/);
  }
});
test('missing auth denial or wrong foreign-record body fails', async () => {
  await assert.rejects(probeApplication(input, async () => ({ status: 200, body: {} }), async () => answer()), /authentication_not_denied/);
  await assert.rejects(probeApplication(input, async (path, options) => path.endsWith(ids[1])
    ? { status: 404, body: { canonical_text: 'PRIVATE_CONTENT' } } : request(path, options), async () => answer()), /foreign_record_visible/);
});
test('HTTP adapter denies remote endpoints, redirects and content-bearing errors', async () => {
  for (const base of ['https://127.0.0.1:4040', 'http://localhost:4040', 'http://example.test:4040', 'http://127.0.0.1:4040/path'])
    assert.throws(() => loopbackRequest(base), /nonlocal_endpoint/);
  const client = loopbackRequest('http://127.0.0.1:4040', async (_url, options) => {
    assert.equal(options.redirect, 'error'); throw Error('PRIVATE_PROVIDER_ERROR');
  });
  await assert.rejects(client('/v1/query', {}), error => error.message === 'Staging application probe failed: bounded_http_request');
});
