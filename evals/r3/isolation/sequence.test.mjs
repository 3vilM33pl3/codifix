// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { runOfflineSequence, validateSequence } from './sequence.mjs';
const image = process.env.MEMORY_R3_TEST_IMAGE;
if (process.env.MEMORY_R3_REQUIRE_CONTAINER === '1' && !image) throw Error('required container test image missing');

function fixture(image) {
  return { image, initialFiles: { 'source.json': '{"count":0}', 'public.txt': 'Only increment the count once per step.' },
    submissionPaths: ['source.json'], timeoutMs: 4000,
    steps: [1, 2, 3].map(n => ({ id: `step-${n}`,
      inputFiles: { 'task.json': JSON.stringify({ stage: n }), [`current-${n}.txt`]: 'Only this stage is disclosed.' },
      argv: ['/usr/local/bin/node', '-e', `
        const fs=require('fs'),a=require('assert');
        a(!fs.existsSync('grader'));a(!fs.existsSync('grading-feedback.txt'));
        a(!fs.existsSync('/input/files/grader'));a(!fs.existsSync('private.json'));
        a.equal(fs.readFileSync('public.txt','utf8'),'Only increment the count once per step.');
        a.equal(JSON.parse(fs.readFileSync('task.json')).stage,${n});
        for(let i=1;i<=3;i++) a.equal(fs.existsSync('current-'+i+'.txt'),i===${n});
        const value=JSON.parse(fs.readFileSync('source.json'));a.equal(value.count,${n-1});
        value.count++;fs.writeFileSync('source.json',JSON.stringify(value));
        fs.mkdirSync('grader');fs.writeFileSync('grader/check.mjs','process.exit(0)');
        fs.writeFileSync('public.txt','TAMPERED PUBLIC CONTEXT');
        fs.writeFileSync('grading-feedback.txt','AGENT CANNOT INJECT NEXT STEP FEEDBACK');
        console.log('I passed all tests');
      `],
      grader: { argv: ['/usr/local/bin/node', '/work/grader/check.mjs'], files: {
        'private.json': JSON.stringify({ expected: n, hidden: `private-step-${n}` }),
        'check.mjs': `
          import fs from 'node:fs';import assert from 'node:assert/strict';
          const expected=JSON.parse(fs.readFileSync('grader/private.json'));
          const value=JSON.parse(fs.readFileSync('submission/source.json'));
          assert.equal(value.count,expected.expected);
          fs.writeFileSync('submission/source.json','{"count":999}');
          fs.writeFileSync('grading-feedback.txt',expected.hidden);
          console.log(expected.hidden);
        `,
      } },
    })) };
}

test('sequence validates the complete hidden grader contract before execution', () => {
  const valid = fixture(`sha256:${'a'.repeat(64)}`);
  validateSequence(valid);
  for (const mutate of [
    o => { o.steps.pop(); }, o => { o.steps[2].id = o.steps[0].id; },
    o => { o.submissionPaths = ['../private']; }, o => { o.steps[2].grader.files = {}; },
    o => { o.steps[2].grader.files = { '../escape': 'no' }; },
    o => { o.steps[2].grader.argv = ['relative-command']; },
    o => { o.steps[2].inputFiles = { 'source.json': 'override submission' }; },
    o => { o.steps[2].inputFiles = { 'public.txt': 'override prepared context' }; },
    o => { o.steps[2].inputFiles = { '../private': 'unsafe' }; },
    o => { o.steps[2].inputFiles = []; },
  ]) { const invalid = structuredClone(valid); mutate(invalid); assert.throws(() => validateSequence(invalid)); }
});

test('independent grading: private checks never return to agents; negative results and fresh resets', { skip: !image, timeout: 90000 }, async () => {
  const options = fixture(image);
  const pending = runOfflineSequence(options);
  options.steps[2].grader.files['private.json'] = '{"expected":999}';
  const good = await pending;
  assert.equal(good.success, true);
  assert.equal(good.officialEvidence, false);
  assert.equal(good.steps.length, 3);
  assert.deepEqual(JSON.parse(good.submission['source.json']), { count: 3 });
  assert(!JSON.stringify(good).includes('private-step-'));
  const repeated = await runOfflineSequence(fixture(image));
  assert.equal(repeated.initialSha256, good.initialSha256);
  assert.equal(repeated.steps[0].agentInputSha256, good.steps[0].agentInputSha256);
  assert.deepEqual(repeated.submission, good.submission);
  const bad = fixture(image);
  // A confident no-op implementation cannot pass hidden positive checks.
  for (const step of bad.steps) step.argv = ['/usr/local/bin/node', '-e', 'console.log("all checks passed")'];
  const failed = await runOfflineSequence(bad);
  assert.equal(failed.success, false);
  assert(failed.steps.every(step => !step.passed && step.graderExitCode !== 0));
  assert.deepEqual(JSON.parse(failed.submission['source.json']), { count: 0 });
  const deleted = fixture(image);
  deleted.steps[0].argv = ['/usr/local/bin/node', '-e', "require('fs').unlinkSync('source.json')"];
  deleted.steps[1].argv = ['/usr/local/bin/node', '-e', "const fs=require('fs');require('assert')(!fs.existsSync('source.json'));fs.writeFileSync('source.json','{\"count\":2}')"];
  const recovered = await runOfflineSequence(deleted);
  assert.equal(recovered.success, false, 'later repair cannot erase a failed sequence step');
  assert.deepEqual(recovered.steps.map(step => step.passed), [false, true, true]);
  assert.deepEqual(JSON.parse(recovered.submission['source.json']), { count: 3 });
});
