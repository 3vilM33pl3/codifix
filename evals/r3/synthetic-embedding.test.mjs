// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { syntheticVector, syntheticServiceEmbedding } from './synthetic-embedding.mjs';

test('synthetic vectors are stable and exactly representable in the database format', async () => {
  for (const text of ['source one', 'source two', 'unicode: café 🚀']) {
    const vector = syntheticVector(text);
    assert.equal(vector.length, 16);
    assert.deepEqual(vector, vector.map(Math.fround));
    assert.deepEqual(vector, syntheticVector(text));
    const embedding = syntheticServiceEmbedding();
    assert.deepEqual(await embedding.embed(text, 'document'), vector);
    assert.deepEqual(await embedding.embed(text, 'query'), vector);
  }
});

test('document and query purposes use the same fixture without changing chunking', async () => {
  const embedding = syntheticServiceEmbedding();
  assert.deepEqual(await embedding.embed('summary\nnormalized source', 'document'), syntheticVector('summary\nnormalized source'));
  await assert.rejects(embedding.embed('whole source', 'unknown'), /invalid embedding purpose/);
  assert.deepEqual(await embedding.embed('new question', 'query'), syntheticVector('new question'));
});
