// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { checkedVector, openEmbeddingProxy } from './embedding-proxy.mjs';

test('supplied vectors use explicit float32 storage and reject invalid values', () => {
  assert.deepEqual(checkedVector([0.1, 1], 2), [Math.fround(0.1), 1]);
  for (const vector of [[0, 0], [1], [NaN, 1], [Infinity, 1], [1e100, 1], [1e-100, 0]]) {
    assert.throws(() => checkedVector(vector, 2), /invalid supplied vector/);
  }
});

test('loopback embedding requests bind model and exact question, consume once and reject fallback', async () => {
  let calls = 0;
  const proxy = await openEmbeddingProxy({ model: 'synthetic-v1', dimensions: 2,
    embed: async (text, purpose) => { assert.equal(text, 'exact question'); assert.equal(purpose, 'query'); calls++; return [1, 0]; } });
  const send = async (body, token = proxy.token) => fetch(proxy.baseUrl + '/embeddings', {
    method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  const body = { model: 'synthetic-v1', input: ['exact question'] };
  const active = { diagnostics: { semantic_status: 'active_space_ok' } };
  try {
    assert.equal((await send(body)).status, 403);
    await proxy.query('exact question', async () => {
      assert.equal((await send(body, 'untrusted')).status, 403);
      assert.equal((await send(body)).status, 200);
      assert.equal((await send(body)).status, 403);
      await assert.rejects(proxy.query('exact question', async () => active), /unavailable/);
      return active;
    });
    assert.equal(calls, 1);
    await assert.rejects(proxy.query('exact question', async () => active), /not verified/);
    await assert.rejects(proxy.query('exact question', async () => {
      assert.equal((await send({ ...body, model: 'changed-model' })).status, 403);
      assert.equal((await send(body)).status, 403);
      return active;
    }), /not verified/);
    await assert.rejects(proxy.query('exact question', async () => {
      assert.equal((await send(body)).status, 200);
      return { diagnostics: { semantic_status: 'active_space_missing' } };
    }), /not verified/);
    assert.equal(calls, 2);
  } finally { await proxy.close(); }
});
