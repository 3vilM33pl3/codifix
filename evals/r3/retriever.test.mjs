// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { bindCorpus, bindQuery, syntheticProfile, vectorDigest } from './embedding-evidence.mjs';

const binary = process.env.CODIFIX_R3_RETRIEVER_BIN;
if (process.env.CODIFIX_R3_REQUIRE_RETRIEVER === '1' && !binary) throw Error('required retriever binary missing');
test('decimal float32 vectors preserve exact bindings across the JavaScript/Rust boundary', { skip: !binary }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-decimal-vectors-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  // Synthetic, non-dyadic decimal encodings; constant 0.5 vectors missed this
  // boundary. No provider output or benchmark labels are needed to reproduce it.
  const vector = Array.from({ length: 1536 }, (_, i) => Math.fround(((i * 7919) % 65521 - 32760) / 1000003));
  const profile = { ...syntheticProfile(vector.length, 'decimal-fixture-r1'), normalization: 'float32-rounding-v1' };
  const documents = [{ id: 'runbook', text: 'synthetic source' }];
  const file = path.join(directory, 'configuration.json');
  await fs.writeFile(file, JSON.stringify({ project: 'synthetic', condition: 'external-retriever', mode: 'vector',
    documents, embeddings: [vector], embedding_corpus: bindCorpus(profile, documents, [vector]), top_k: 8, bytes: 1024 }));
  const request = { schema_version: 1, project: 'synthetic', condition: 'external-retriever', query: 'synthetic question',
    limit: 8, item_id: 'q1', context: { fixture_path: '', hidden_facts: [] }, query_embedding: vector,
    embedding_query: bindQuery(profile, 'synthetic question', vector) };
  const invoke = input => spawnSync(binary, [file], { input: JSON.stringify(input), encoding: 'utf8',
    env: { PATH: '/usr/bin:/bin' }, timeout: 5000 });
  const result = invoke(request);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout).results.map(hit => hit.id), ['runbook']);
  const changed = structuredClone(request);
  changed.query_embedding[0] = Math.fround(changed.query_embedding[0] + 0.000001);
  assert.equal(invoke(changed).status, 1, 'do not weaken digest checks to accept changed vectors');
});
test('embedding bindings are strict, bounded and do not mutate caller profiles', () => {
  const profile = syntheticProfile(2, 'unit-vectors-r1');
  const docs = [{ id: 'a', text: 'source' }];
  const corpus = bindCorpus(profile, docs, [[1, 0]]);
  profile.model = 'changed';
  assert.equal(corpus.profile.model, 'unit-vectors-r1');
  for (const vector of [[], [0, 0], [NaN, 1], [Infinity, 1], new Array(8193).fill(1)]) assert.throws(() => vectorDigest(vector));
  assert.throws(() => bindCorpus(profile, [docs[0], docs[0]], [[1, 0], [1, 0]]));
  assert.throws(() => bindQuery({ ...profile, unknown: true }, 'q', [1, 0]));
  assert.throws(() => bindQuery(profile, 'q', [1]));
});
test('real external retriever protocol: baselines, scoped context and sanitized rejection', { skip: !binary }, async t => {
  assert(path.isAbsolute(binary));
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-retriever-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const config = { project: 'synthetic', condition: 'external-retriever', mode: 'lexical',
    documents: [{ id: 'runbook', text: 'retry after reconciliation' }, { id: 'incident', text: 'gateway maintenance' }],
    top_k: 8, bytes: 1024, embeddings: [[1, 0], [0, 1]] };
  const profile = syntheticProfile(2, 'unit-vectors-r1');
  config.embedding_corpus = bindCorpus(profile, config.documents, config.embeddings);
  const request = { schema_version: 1, project: 'synthetic', condition: 'external-retriever',
    query: 'retry', limit: 8, item_id: 'q1', context: { fixture_path: '/private/not-readable', hidden_facts: [] } };
  async function invoke(configuration, input) {
    const file = path.join(directory, 'configuration.json');
    await fs.writeFile(file, JSON.stringify(configuration));
    return spawnSync(binary, [file], { input: JSON.stringify(input), encoding: 'utf8',
      env: { PATH: '/usr/bin:/bin' }, timeout: 5000, maxBuffer: 2 * 1024 * 1024 });
  }
  const lexical = await invoke(config, request);
  assert.equal(lexical.status, 0);
  assert.deepEqual(JSON.parse(lexical.stdout).results.map(h => h.id), ['runbook']);
  assert.deepEqual(JSON.parse(lexical.stdout).results[0].citations, ['runbook']);
  const handover = await invoke({ ...config, mode: 'handover' }, request);
  assert.equal(JSON.parse(handover.stdout).results.length, 2);
  const vectorRequest = { ...request, query_embedding: [0, 1], embedding_query: bindQuery(profile, request.query, [0, 1]) };
  const vector = await invoke({ ...config, mode: 'vector' }, vectorRequest);
  assert.equal(vector.status, 0, vector.stderr);
  assert.equal(JSON.parse(vector.stdout).results[0].id, 'incident');
  for (const [cfg, req] of [
    [config, { ...request, project: 'another-project' }],
    [config, { ...request, context: { ...request.context, hidden_facts: ['SECRET_LABEL'] } }],
    [{ ...config, mode: 'vector' }, request],
    [{ ...config, mode: 'vector', embedding_corpus: undefined }, vectorRequest],
    [{ ...config, mode: 'vector' }, { ...vectorRequest, embedding_query: undefined }],
    [{ ...config, mode: 'vector' }, { ...vectorRequest, query: 'changed question' }],
    [{ ...config, mode: 'vector' }, { ...vectorRequest, query_embedding: [1, 0] }],
    [{ ...config, mode: 'vector' }, { ...vectorRequest, embedding_query: bindQuery({ ...profile, model: 'unit-vectors-r2' }, request.query, [0, 1]) }],
    [{ ...config, mode: 'vector', documents: [{ ...config.documents[0], text: 'changed source' }, config.documents[1]] }, vectorRequest],
    [{ ...config, mode: 'vector', embeddings: [...config.embeddings].reverse() }, vectorRequest],
    [{ ...config, mode: 'handover', bytes: 1 }, request],
    [config, { ...request, query: 'sensitive'.repeat(150000) }],
  ]) {
    const rejected = await invoke(cfg, req);
    assert.equal(rejected.status, 1);
    assert.equal(rejected.stdout, '');
    assert.equal(rejected.stderr.trim(), 'R3 retriever rejected request or configuration');
  }
});
