// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted fake executors exercise sequence wiring, not native OS boundaries.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createSequenceRunner, validateSequence } from './sequence.mjs';
import { nativeProfileHash } from './native-windows.mjs';
import { validateRustCase } from './rust.mjs';
import { runOfflineKnowledge } from './knowledge.mjs';
import { createProviderExecutor } from './rehearsal-task.mjs';
const sha = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');

function fixture() {
  const file = name => ({ path: `C:\\private\\tool\\${name}`, sha256: 'a'.repeat(64) });
  const node = file('node.exe'), guest = file('guest.mjs');
  const profile = { schema_version: 1, helper: { path: 'C:\\private\\helper.exe', sha256: 'b'.repeat(64) },
    io_root: 'C:\\private\\runs', node, guest,
    toolchains: [{ root: 'C:\\private\\tool', files: [node, guest] }],
    search_path: ['C:\\private\\tool'], library_path: [] };
  return { execution: { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) },
    initialFiles: { 'source.json':'{"count":0}', 'public.txt':'fixed public context' },
    submissionPaths: ['source.json'], timeoutMs: 1000,
    steps: [1,2,3].map(index => ({ id:`step-${index}`, argv: [node.path, '-e', 'explicit native command'],
      inputFiles: { 'task.json':JSON.stringify({ index }), [`current-${index}.txt`]:'only this stage' },
      grader: { argv:[node.path, 'grader/check.mjs'], files: { 'check.mjs':'fixed reviewed check',
        'private.json': JSON.stringify({ expected:index, secret:`private-stage-${index}` }) } },
    })) };
}
function executor({ noOp = false, malformedIdentityAt, deletedFirst = false } = {}) {
  const calls = [];
  const run = createSequenceRunner({ runExecution: async input => {
    calls.push(structuredClone(input));
    await Promise.resolve();
    const grader = Object.hasOwn(input.files, 'grader/private.json');
    let files = {}, stdout = '', exitCode = 0;
    if (grader) {
      assert.deepEqual(input.exportPaths, []);
      const check = JSON.parse(input.files['grader/private.json']);
      const submission = input.files['submission/source.json'];
      exitCode = submission && JSON.parse(submission).count === check.expected ? 0 : 1;
      stdout = check.secret; // Must never become next-agent context or a result.
    } else {
      assert(!Object.keys(input.files).some(name => name.startsWith('grader/')));
      assert.equal(input.files['public.txt'], 'fixed public context');
      const { index } = JSON.parse(input.files['task.json']);
      for (const stage of [1,2,3]) assert.equal(Object.hasOwn(input.files, `current-${stage}.txt`), stage === index);
      const prior = input.files['source.json'] ? JSON.parse(input.files['source.json']).count : 0;
      if (!(deletedFirst && index === 1)) files['source.json'] = Buffer.from(JSON.stringify({count:noOp ? prior : index})).toString('base64');
      stdout = 'Agent claims success; only the grader decides';
    }
    return { version:1, exitCode, stdout, stderr:'', files, elapsedMs:1,
      execution:{ kind:'windows-native-v1', profile_sha256: calls.length === malformedIdentityAt ? '0'.repeat(64) : input.execution.profile_sha256,
        descriptor_sha256:sha(input), receipt_sha256:sha([input, calls.length]), disk_hard_limit_enforced:false },
      privateOperation:`C:\\private\\runs\\operation-${calls.length}` };
  } });
  return { run, calls };
}

test('native sequence carries source only and grades each step in a fresh explicit call', async () => {
  const fixtureValue = fixture(), value = executor();
  const pending = value.run(fixtureValue);
  fixtureValue.steps[2].grader.files['private.json'] = '{"expected":999}';
  const result = await pending;
  assert.equal(value.calls.length, 6); assert.equal(result.success, true);
  assert.equal(result.kind, 'native-windows-sequence-smoke');
  assert.equal(result.image, undefined); assert.equal(result.officialEvidence, false);
  assert.equal(result.execution.disk_hard_limit_enforced, false);
  assert.deepEqual(JSON.parse(result.submission['source.json']), {count:3});
  assert(!JSON.stringify(result).includes('private-stage-'));
  assert(result.steps.every(step => step.agentExecution.profile_sha256 === result.execution.profile_sha256));
  const repeated = await executor().run(fixture());
  assert.equal(result.initialSha256, repeated.initialSha256);
  assert.equal(result.steps[0].agentInputSha256, repeated.steps[0].agentInputSha256);
});

test('native no-op and deleted source remain negative results despite later recovery', async () => {
  assert.equal((await executor({noOp:true}).run(fixture())).success, false);
  const deleted = await executor({deletedFirst:true}).run(fixture());
  assert.deepEqual(deleted.steps.map(step => step.passed), [false,true,true]);
  assert.equal(deleted.success, false);
});

test('native identity mismatch, unpinned Rust and unintegrated model/provider campaign paths fail closed', async () => {
  await assert.rejects(executor({malformedIdentityAt:2}).run(fixture()), /native execution identity/);
  const value = fixture();
  value.steps[2].grader = {kind:'rust-blackbox', probes:[]};
  const exec = executor();
  await assert.rejects(exec.run(value), /explicit pinned compiler/);
  assert.equal(exec.calls.length, 0);
  const model = fixture(); model.runAgent = async () => { throw Error('must not run'); };
  assert.throws(() => validateSequence(model), /native commands/);
  assert.throws(() => validateRustCase({execution:fixture().execution}), /explicit pinned compiler/);
  await assert.rejects(runOfflineKnowledge({execution:fixture().execution}), /no Linux fallback/);
  await assert.rejects(createProviderExecutor({offline:true,split:'development',configuration:{execution:fixture().execution}}), /native provider campaign orchestration/);
});
