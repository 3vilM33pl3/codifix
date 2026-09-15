// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { openRequestMediator } from '../request-mediator.mjs';
import { createModelAgent } from './model-agent.mjs';
import { runOfflineSequence } from './sequence.mjs';

const binary = process.env.MEMORY_R3_BUDGET_BIN, image = process.env.MEMORY_R3_TEST_IMAGE;
if (process.env.MEMORY_R3_REQUIRE_MODEL_AGENT === '1' && (!binary || !image)) throw Error('required isolated model agent configuration missing');
const model = { provider: 'synthetic', resolved_id: 'fake-command-v1', reasoning: 'none' };
const allowance = { input_tokens: 100, output_tokens: 100, microusd: 10 };
const usage = { input_tokens: 20, output_tokens: 10, microusd: 2 };

function sequence() {
  return { image, timeoutMs: 4000, initialFiles: { 'counter.json': '{"value":0}', 'context.txt': 'Increment once for each current task.' },
    submissionPaths: ['counter.json'], steps: [1, 2, 3].map(n => ({ id: `step-${n}`,
      argv: ['/usr/local/bin/node', '-e', 'process.exit(1)'],
      inputFiles: { 'task.txt': String(n) },
      grader: { argv: ['/usr/local/bin/node', '/work/grader/test.mjs'], files: {
        // .mjs uses imports; the hidden sentinel never goes to the model.
        'test.mjs': `import fs from 'node:fs'; import assert from 'node:assert/strict';
          assert.equal(JSON.parse(fs.readFileSync('submission/counter.json')).value,${n});
          console.log('HIDDEN_GRADING_SENTINEL');`,
      } },
    })) };
}

test('six budgeted model calls span three isolated steps without leaking future tasks or hidden checks', { skip: !binary || !image, timeout: 60000 }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-model-agent-'));
  const limits = { requests: 6, tokens: 2000, microusd: 100, task_seconds: 60 };
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: 'f'.repeat(64), limits, taskLimits: limits });
  let calls = 0;
  try {
    const runAgent = createModelAgent({ mediator, taskId: 'three-step-task', model, allowance, maxCalls: 6,
      invoke: async payload => {
        calls++;
        const text = JSON.stringify(payload);
        assert(!text.includes('HIDDEN_GRADING_SENTINEL'));
        assert(!text.includes('grader'));
        assert(!text.includes(directory));
        assert.equal(payload.files['task.txt'], payload.step_id.slice(-1));
        assert.equal(payload.files['context.txt'], 'Increment once for each current task.');
        const expected = Number(payload.files['task.txt']);
        if (payload.previous) {
          assert.equal(payload.previous.exit_code, 0);
          assert.equal(payload.previous.stdout.trim(), 'isolated');
          assert.equal(JSON.parse(payload.files['counter.json']).value, expected);
          return { output: { kind: 'finish' }, usage };
        }
        assert.equal(JSON.parse(payload.files['counter.json']).value, expected - 1);
        return { output: { kind: 'command', argv: ['/usr/local/bin/node', '-e', `
          const fs=require('fs'),a=require('assert');
          a(!fs.existsSync('/var/run/docker.sock'));a(!fs.existsSync('/grader'));
          a(!Object.keys(process.env).some(k=>/TOKEN|KEY|MEMORY/.test(k)));
          const x=JSON.parse(fs.readFileSync('counter.json'));x.value++;
          fs.writeFileSync('counter.json',JSON.stringify(x));
          fs.writeFileSync('context.txt','UNTRUSTED_REPLACEMENT');console.log('isolated');
        `] }, usage };
      } });
    const result = await runOfflineSequence({ ...sequence(), runAgent });
    assert.equal(result.success, true);
    assert.equal(result.officialEvidence, false);
    assert.equal(calls, 6);
    assert.deepEqual(JSON.parse(result.submission['counter.json']), { value: 3 });
    assert(result.steps.every(step => step.agentElapsedMs > 0 && step.graderElapsedMs > 0));
    assert.deepEqual((await mediator.status()).ledger, { requests: 6, tokens: 180, microusd: 12 });
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});

test('malformed model command cannot become a host command or pass hidden scoring', { skip: !binary || !image, timeout: 60000 }, async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-model-agent-negative-'));
  const limits = { requests: 3, tokens: 1000, microusd: 50, task_seconds: 30 };
  const mediator = await openRequestMediator({ binary, directory, manifestSha256: '1'.repeat(64), limits, taskLimits: limits });
  try {
    const runAgent = createModelAgent({ mediator, taskId: 'negative', model, allowance, maxCalls: 3,
      invoke: async () => ({ output: { kind: 'command', argv: ['relative-host-command'] }, usage }) });
    const result = await runOfflineSequence({ ...sequence(), runAgent });
    assert.equal(result.success, false);
    assert(result.steps.every(step => step.agentExitCode === 1 && !step.passed));
    assert.deepEqual(JSON.parse(result.submission['counter.json']), { value: 0 });
  } finally { await mediator.close(); await fs.rm(directory, { recursive: true }); }
});
