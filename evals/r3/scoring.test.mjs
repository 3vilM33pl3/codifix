// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const binary = process.env.MEMORY_R3_SCORE_BIN;
if (process.env.MEMORY_R3_REQUIRE_SCORE === '1' && !binary) throw Error('required scoring binary missing');
test('real blinded review utility: pending, human decisions, conflict and tamper rejection', { skip: !binary }, async t => {
  assert(path.isAbsolute(binary));
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-score-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const input = { manifest_sha256: '1'.repeat(64), work: { case_id: 'development-q1', condition: 'full-memory', repeat: 0 },
    rubric: { case_id: 'development-q1', question: 'What does the incident establish?',
      criteria: ['factual_correctness', 'required_evidence', 'appropriate_refusal'].map((dimension, i) => ({
        id: `criterion-${i}`, dimension, instruction: 'Synthetic fixture criterion, not a benchmark label.', critical: false })),
      minimum_citations: 1, citations_critical: true },
    sources: [{ id: 'incident.md', text: 'The settlement committed before acknowledgement failed.' }],
    answer: { text: 'A missing acknowledgement does not establish an uncommitted settlement.', refused: false,
      citations: [{ source_id: 'incident.md', quote: 'settlement committed before acknowledgement failed' }] } };
  const inputPath = path.join(directory, 'input.json');
  const out = path.join(directory, 'out');
  await fs.writeFile(inputPath, JSON.stringify(input));
  function invoke(...args) {
    return spawnSync(binary, args, { encoding: 'utf8', env: { PATH: '/usr/bin:/bin' }, timeout: 5000, maxBuffer: 4 * 1024 * 1024 });
  }
  assert.equal(invoke('prepare', inputPath, out).status, 0);
  const original = await fs.readFile(path.join(out, 'private/evidence.json'), 'utf8');
  assert.equal(invoke('prepare', inputPath, out).status, 1);
  assert.equal(await fs.readFile(path.join(out, 'private/evidence.json'), 'utf8'), original);
  if (process.platform !== 'win32') {
    assert.equal((await fs.stat(out)).mode & 0o777, 0o700);
    assert.equal((await fs.stat(path.join(out, 'private/evidence.json'))).mode & 0o777, 0o600);
  }
  const packet = JSON.parse(await fs.readFile(path.join(out, 'review/packet.json')));
  for (const key of ['work', 'condition', 'repeat', 'model', 'execution_nonce', 'ballots']) assert(!(key in packet));
  const blank = JSON.parse(await fs.readFile(path.join(out, 'review/ballot.json')));
  const ballotsPath = path.join(directory, 'ballots.json');
  async function score(ballots, evidence = JSON.parse(original)) {
    await fs.writeFile(ballotsPath, JSON.stringify(ballots));
    const evidencePath = path.join(directory, 'evidence.json');
    await fs.writeFile(evidencePath, JSON.stringify(evidence));
    return invoke('score', evidencePath, ballotsPath);
  }
  const pending = JSON.parse((await score([blank])).stdout).assessment;
  assert(pending.unresolved && !pending.success);
  const human = structuredClone(blank);
  human.reviewer = 'synthetic-human-test-only';
  for (const judgment of Object.values(human.judgments)) {
    judgment.decision = 'pass'; judgment.reason = 'Fake human decision for executable test only.';
  }
  assert.equal(JSON.parse((await score([human])).stdout).assessment.success, true);
  const diagnosis = { ...human, kind: 'model_diagnosis' };
  assert.equal(JSON.parse((await score([diagnosis])).stdout).assessment.unresolved, true);
  const conflict = structuredClone(human);
  conflict.reviewer = 'synthetic-second-reviewer'; conflict.judgments['criterion-0'].decision = 'fail';
  assert.equal(JSON.parse((await score([human, conflict])).stdout).assessment.unresolved, true);
  const changed = JSON.parse(original);
  changed.packet.answer.text += ' SECRET_UNSUPPORTED_CLAIM';
  const rejected = await score([human], changed);
  assert.equal(rejected.status, 1);
  assert.equal(rejected.stdout, '');
  assert.equal(rejected.stderr.trim(), 'R3 scoring rejected arguments or review evidence');
  assert.equal((await score([human, human])).status, 1);
});
