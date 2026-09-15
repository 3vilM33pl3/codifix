// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const binary = process.env.MEMORY_R3_RETRIEVER_BIN;
if (process.env.MEMORY_R3_REQUIRE_RETRIEVER === '1' && !binary) throw Error('required retriever binary missing');
test('real external retriever protocol: baselines, scoped context and sanitized rejection', { skip: !binary }, async t => {
  assert(path.isAbsolute(binary));
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-retriever-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const config = { project: 'synthetic', condition: 'external-retriever', mode: 'lexical',
    documents: [{ id: 'runbook', text: 'retry after reconciliation' }, { id: 'incident', text: 'gateway maintenance' }],
    top_k: 8, bytes: 1024, embeddings: [[1, 0], [0, 1]] };
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
  const vector = await invoke({ ...config, mode: 'vector' }, { ...request, query_embedding: [0, 1] });
  assert.equal(JSON.parse(vector.stdout).results[0].id, 'incident');
  for (const [cfg, req] of [
    [config, { ...request, project: 'another-project' }],
    [config, { ...request, context: { ...request.context, hidden_facts: ['SECRET_LABEL'] } }],
    [{ ...config, mode: 'vector' }, request],
    [{ ...config, mode: 'handover', bytes: 1 }, request],
    [config, { ...request, query: 'sensitive'.repeat(150000) }],
  ]) {
    const rejected = await invoke(cfg, req);
    assert.equal(rejected.status, 1);
    assert.equal(rejected.stdout, '');
    assert.equal(rejected.stderr.trim(), 'R3 retriever rejected request or configuration');
  }
});
