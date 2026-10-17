// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { preparedContext, runOfflineKnowledge } from './knowledge.mjs';
import { bindCorpus, bindQuery, syntheticProfile } from '../embedding-evidence.mjs';

const image = process.env.CODIFIX_R3_TEST_IMAGE;
const scoreBinary = process.env.CODIFIX_R3_SCORE_BIN;
const retrieverBinary = process.env.CODIFIX_R3_RETRIEVER_BIN;
if (process.env.CODIFIX_R3_REQUIRE_KNOWLEDGE === '1' && (!image || !scoreBinary || !retrieverBinary)) throw Error('required knowledge bridge configuration missing');
const hash = value => createHash('sha256').update(value).digest('hex');
const sources = [{ id: 'public/incident.md', text: 'The settlement committed before acknowledgement failed.' },
  { id: 'public/irrelevant.md', text: 'OTHER_CONTEXT_ONLY never copy this document when not selected.' }];
const binding = { project: 'r3-alpha', source: sources[0].id, memory_id: 'version-one', canonical_id: 'canonical-one',
  version_no: 1, source_sha256: hash(sources[0].text) };
const hit = { project: 'r3-alpha', memory_id: binding.memory_id, canonical_id: binding.canonical_id, version_no: 1,
  repo_root: '/SECRET_HOST_PATH', snippet: 'UNTRUSTED_EXTRA_TEXT', sources: [{ path: '/NEVER_FORWARD' }] };
const rubric = { case_id: 'synthetic-q1', question: 'What does the settlement evidence show?',
  criteria: ['factual_correctness', 'required_evidence', 'appropriate_refusal'].map((dimension, i) => ({
    id: `criterion-${i}`, dimension, instruction: 'PRIVATE_RUBRIC_ONLY synthetic test criterion.', critical: false })),
  minimum_citations: 1, citations_critical: true };

test('prepared context maps exact source versions and excludes service metadata', () => {
  const input = { condition: 'full-memory', sources, bindings: [binding], project: 'r3-alpha', retrieval: { results: [hit] } };
  assert.deepEqual(preparedContext(input), [sources[0]]);
  assert.deepEqual(preparedContext({ ...input, contextBytes: 1 }), []);
  for (const changed of [
    { bindings: [{ ...binding, source_sha256: '0'.repeat(64) }] },
    { bindings: [binding, binding] }, { bindings: [] },
    { project: 'r3-beta' },
    { retrieval: { results: [{ ...hit, version_no: 2 }] } },
    { retrieval: { results: [{ ...hit, canonical_id: 'different' }] } },
    { retrieval: { results: [{ ...hit, project: 'r3-beta' }] } },
    { retrieval: { results: [hit, hit] } },
  ]) assert.throws(() => preparedContext({ ...input, ...changed }));
  const baseline = { condition: 'lexical', sources,
    retrieval: { results: [{ ...sources[0], citations: [sources[0].id] }] } };
  assert.deepEqual(preparedContext(baseline), [sources[0]]);
  assert.throws(() => preparedContext({ ...baseline, retrieval: { results: [{ ...sources[0], text: 'changed', citations: [sources[0].id] }] } }));
  assert.throws(() => preparedContext({ ...baseline, condition: 'no-memory' }));
  assert.throws(() => preparedContext({ ...baseline, sources: [{ id: '../private', text: 'unsafe' }] }));
});

const agent = ['/usr/local/bin/node', '-e', `
  const fs=require('fs'),assert=require('assert');
  const raw=fs.readFileSync('input.json','utf8'),input=JSON.parse(raw);
  assert.deepEqual(Object.keys(input).sort(),['context','question']);
  assert(!raw.includes('PRIVATE_RUBRIC_ONLY'));
  if(input.context.length < 2) assert(!raw.includes('OTHER_CONTEXT_ONLY'));
  assert(!raw.includes('SECRET_HOST_PATH'));assert(!raw.includes('condition'));
  assert(!fs.existsSync('/var/run/docker.sock'));assert(!fs.existsSync('/private'));
  assert(!Object.keys(process.env).some(k=>/TOKEN|KEY|MEMORY/.test(k)));
  const source=input.context[0];
  fs.writeFileSync('answer.json',JSON.stringify(source
    ? {text:source.text,refused:false,citations:[{source_id:source.id,quote:source.text}]}
    : {text:'Insufficient supplied evidence.',refused:true,citations:[]}));
`];
const options = { image, scoreBinary, sources, context: [sources[0]], question: rubric.question, rubric,
  manifestSha256: '1'.repeat(64), work: { case_id: rubric.case_id, condition: 'lexical', repeat: 0 }, argv: agent };

test('knowledge contract rejects changed context before starting any container', async () => {
  const fake = { ...options, image: `sha256:${'a'.repeat(64)}`, scoreBinary: '/not-executed', argv: ['/not-executed'] };
  for (const changed of [
    { context: [{ ...sources[0], text: 'invented' }] }, { context: [sources[0], sources[0]] },
    { work: { ...fake.work, condition: 'no-memory' } }, { question: 'different question' },
  ]) await assert.rejects(runOfflineKnowledge({ ...fake, ...changed }));
});

test('actual retriever, isolated answer and Rust blinded adjudicator share source bytes', { skip: !image || !scoreBinary || !retrieverBinary, timeout: 120000 }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-knowledge-retriever-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const config = path.join(directory, 'config.json');
  // Both documents are prepared for every informed condition. Selection is
  // actual BM25 or fake offline cosine; no expected answer is passed to either.
  for (const mode of ['handover', 'lexical', 'vector']) {
    const limit = mode === 'handover' ? 2 : 1;
    const profile = syntheticProfile(2, 'unit-vectors-r1');
    await fs.writeFile(config, JSON.stringify({ project: 'r3-alpha', condition: 'external-retriever', mode,
      documents: sources, embeddings: [[1, 0], [0, 1]], embedding_corpus: bindCorpus(profile, sources, [[1, 0], [0, 1]]),
      top_k: limit, bytes: 32768 }));
    const request = { schema_version: 1, project: 'r3-alpha', condition: 'external-retriever', query: 'settlement',
      limit, item_id: rubric.case_id, context: { fixture_path: '', hidden_facts: [] }, query_embedding: [1, 0],
      embedding_query: bindQuery(profile, 'settlement', [1, 0]) };
    // execFile has no input option; use the existing CLI's JSON stdin contract.
    const retrieval = await new Promise((resolve, reject) => {
      const child = spawn(retrieverBinary, [config], { env: { PATH: '/usr/bin:/bin' }, stdio: ['pipe', 'pipe', 'pipe'] });
      let stdout = '';
      const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
      child.stdout.on('data', bytes => { stdout += bytes; if (stdout.length > 1048576) child.kill('SIGKILL'); });
      child.stderr.resume();
      child.on('error', reject);
      child.on('close', code => { clearTimeout(timer); code === 0 ? resolve(JSON.parse(stdout)) : reject(Error('retriever failed')); });
      child.stdin.end(JSON.stringify(request));
    });
    const context = preparedContext({ condition: mode, sources, retrieval });
    const result = await runOfflineKnowledge({ ...options, context, work: { ...options.work, condition: mode } });
    assert.equal(result.answer.text, sources[0].text);
    assert(result.assessment.citations_valid && result.assessment.unresolved && !result.assessment.success);
    assert.equal(result.evidence.ballots.length, 0);
    assert.equal(result.providerRequests, 0);
    assert(!result.officialEvidence);
    // The reviewer, unlike the answering process, retains the complete sources.
    assert.equal(result.evidence.packet.sources.length, 2);
  }
});

test('no-memory, invalid citations and malformed answer outputs cannot pass', { skip: !image || !scoreBinary, timeout: 120000 }, async () => {
  const empty = await runOfflineKnowledge({ ...options, context: [], work: { ...options.work, condition: 'no-memory' } });
  assert(empty.answer.refused && !empty.assessment.success);
  const wrong = await runOfflineKnowledge({ ...options, argv: ['/usr/local/bin/node', '-e',
    `require('fs').writeFileSync('answer.json',JSON.stringify({text:'Unsupported answer',refused:false,citations:[{source_id:'public/incident.md',quote:'NOT_IN_THE_SOURCE'}]}))`] });
  assert(!wrong.assessment.citations_valid && wrong.assessment.critical_failure && !wrong.assessment.success);
  for (const content of ['not JSON', '{"text":"missing fields"}']) {
    await assert.rejects(runOfflineKnowledge({ ...options, argv: ['/usr/local/bin/node', '-e',
      `require('fs').writeFileSync('answer.json',${JSON.stringify(content)})`] }), /invalid structured offline answer/);
  }
});
