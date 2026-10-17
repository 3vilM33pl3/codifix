// SPDX-License-Identifier: AGPL-3.0-or-later
// Fake host callbacks/executors only: not native isolation or paid R3 evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createModelAgent, isNativeModelAgent } from './model-agent.mjs';
import { createSequenceRunner, validateSequence } from './sequence.mjs';
import { nativeProfileHash } from './native-windows.mjs';
const sha = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const model = { provider: 'synthetic', resolved_id: 'fake-native-node-v1', reasoning: 'none' };
const usage = { input_tokens: 10, output_tokens: 5, microusd: 0 };
function fixture() {
  const node = { path: 'C:\\private\\tool\\node.exe', sha256: 'a'.repeat(64) };
  const guest = { path: 'C:\\private\\tool\\guest.mjs', sha256: 'b'.repeat(64) };
  const profile = { schema_version: 1, helper: { path: 'C:\\private\\helper.exe', sha256: 'c'.repeat(64) },
    io_root: 'C:\\private\\runs', node, guest, toolchains: [{ root: 'C:\\private\\tool', files: [node, guest] }],
    search_path: ['C:\\private\\tool'], library_path: [] };
  return { execution: { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) },
    argv: [node.path, '-e', 'public fixture command'], files: { 'source.json': '{"count":0}', 'public.txt': 'fixed context' },
    exportPaths: ['source.json'], timeoutMs: 1000, stepId: 'step-1' };
}
function adapters({ action, corrupt, maxCalls = 6, deadlineMs } = {}) {
  const input = fixture(), calls = [], requests = [];
  const runNative = async command => {
    calls.push(structuredClone(command));
    const final = /^process\.exit\(([01])\)$/.exec(command.argv[2] ?? '');
    const files = Object.fromEntries(command.exportPaths.filter(name => Object.hasOwn(command.files, name))
      .map(name => [name, Buffer.from(command.files[name]).toString('base64')]));
    if (!final && command.argv[2] === 'increment') {
      files['source.json'] = Buffer.from(JSON.stringify({ count: JSON.parse(command.files['source.json']).count + 1 })).toString('base64');
    }
    const result = { version: 1, exitCode: final ? Number(final[1]) : 0, stdout: final ? '' : 'sandbox feedback', stderr: '',
      files, elapsedMs: 1, privateOperation: 'C:\\private\\runs\\never-model-context',
      execution: { kind: 'windows-native-v1', profile_sha256: command.execution.profile_sha256,
        descriptor_sha256: sha(command), receipt_sha256: sha([command, calls.length]), disk_hard_limit_enforced: false } };
    corrupt?.(result, { final: !!final });
    return result;
  };
  const agent = createModelAgent({ taskId: 'native-task', model, allowance: { input_tokens: 100, output_tokens: 100, microusd: 0 },
    nativeExecution: input.execution, runNative, maxCalls, deadlineMs, invoke() {},
    mediator: { async dispatch(request) {
      requests.push(structuredClone(request.payload));
      const serialized = JSON.stringify(request.payload);
      assert(!serialized.includes('C:\\') && !serialized.includes('grader/') && !serialized.includes('PRIVATE_GRADER'));
      assert.equal(request.payload.tools.protocol, 'isolated-windows-command-v1');
      assert.deepEqual(request.payload.tools.command, { kind: 'command', tool: 'node', args: ['arguments'] });
      return { output: action ? await action(request.payload) : request.payload.previous ? { kind: 'finish' }
        : { kind: 'command', tool: 'node', args: ['-e', 'increment'] }, usage, invoked: true };
    } } });
  return { input, agent, calls, requests, runNative };
}

test('native logical commands and final exports retain real adapter receipts without host-path prompts', async () => {
  const value = adapters();
  assert(isNativeModelAgent(value.agent, value.input.execution));
  const result = await value.agent(value.input);
  assert.equal(result.exitCode, 0); assert.equal(result.image, undefined);
  assert.equal(result.commandExecutions.length, 2);
  assert.deepEqual(result.commandExecutions.map(item => item.kind), ['model-command', 'final-export']);
  assert.deepEqual(result.execution, result.commandExecutions[1].execution);
  assert.equal(JSON.parse(Buffer.from(result.files['source.json'], 'base64')).count, 1);
  assert.equal(value.requests[1].files['public.txt'], 'fixed context');
  assert.equal(value.requests[1].previous.stdout, 'sandbox feedback');
  assert.equal(result.usage.provider_requests, 2); assert.equal(result.usage.microusd, 0);
});

test('native malformed or unbound tools are negative, never translated or run on the host', async () => {
  for (const action of [{ kind: 'command', argv: ['/bin/sh', '-c', 'echo bad'] },
    { kind: 'command', tool: 'powershell', args: [] }, { kind: 'command', tool: 'node', args: ['\0'] },
    { kind: 'finish', extra: true }]) {
    const value = adapters({ action: () => action });
    const result = await value.agent(value.input);
    assert.equal(result.exitCode, 1); assert.equal(value.calls.length, 1);
    assert.equal(value.calls[0].argv[2], 'process.exit(1)');
  }
  const value = adapters(); const bad = structuredClone(value.input);
  bad.execution.profile_sha256 = '0'.repeat(64);
  await assert.rejects(value.agent(bad), /profile mismatch/); assert.equal(value.calls.length, 0); assert.equal(value.requests.length, 0);
});

test('native bad receipt, output, final export or depleted deadline cannot fabricate success', async () => {
  for (const corrupt of [result => { result.execution.receipt_sha256 = 'invalid'; },
    result => { result.files['unselected.txt'] = Buffer.from('bad').toString('base64'); },
    result => { result.files['source.json'] = Buffer.from([255]).toString('base64'); },
    (result, { final }) => { if (final) result.files['source.json'] = Buffer.from('changed').toString('base64'); }]) {
    const value = adapters({ corrupt });
    await assert.rejects(value.agent(value.input));
  }
  const deadline = adapters({ deadlineMs: performance.now() + 500 });
  await assert.rejects(deadline.agent(deadline.input), /finalization deadline/);
  assert.equal(deadline.requests.length, 0); assert.equal(deadline.calls.length, 0);
  const limited = adapters({ maxCalls: 1 }); const result = await limited.agent(limited.input);
  assert.equal(result.exitCode, 1); assert.equal(limited.requests.length, 1);
  assert.equal(result.commandExecutions.length, 2);
});

test('native sequence accepts only matching branded agents and isolates hidden grading from subsequent prompts', async () => {
  const value = adapters();
  const sequence = { execution: value.input.execution, initialFiles: value.input.files,
    submissionPaths: value.input.exportPaths, timeoutMs: 1000,
    steps: [1, 2, 3].map(n => ({ id: `step-${n}`, argv: value.input.argv,
      inputFiles: { 'current-task.txt': String(n) },
      grader: { argv: [value.input.execution.profile.node.path, 'grader/check.mjs'],
        files: { 'check.mjs': 'PRIVATE_GRADER', 'expected.json': String(n) } } })), runAgent: value.agent };
  const runner = createSequenceRunner({ runExecution: async command => {
    assert.equal(command.files['grader/check.mjs'], 'PRIVATE_GRADER');
    const output = await value.runNative(command);
    output.exitCode = JSON.parse(command.files['submission/source.json']).count === Number(command.files['grader/expected.json']) ? 0 : 1;
    output.stdout = 'PRIVATE_GRADER'; return output;
  } });
  const result = await runner(sequence);
  assert.equal(result.success, true); assert.equal(result.officialEvidence, false);
  assert.equal(result.providerUsage.provider_requests, 6); assert.equal(result.steps.length, 3);
  assert(result.steps.every(step => step.modelCommandExecutions.length === 2));
  assert.equal(JSON.parse(result.submission['source.json']).count, 3);
  assert.equal(value.requests[2].files['current-task.txt'], '2');
  assert(!JSON.stringify(value.requests).includes('PRIVATE_GRADER'));
  assert.throws(() => validateSequence({ ...sequence, runAgent: async () => {} }), /profile-bound/);
  const different = structuredClone(sequence.execution); different.profile.io_root = 'C:\\other\\runs'; different.profile_sha256 = nativeProfileHash(different.profile);
  assert.throws(() => validateSequence({ ...sequence, execution: different }), /profile-bound/);
});

test('native final export cannot turn an overrun into a timely result', async t => {
  let now = 0;
  t.mock.method(performance, 'now', () => now);
  const value = adapters({ action: () => ({ kind: 'finish' }), corrupt: (_result, { final }) => { if (final) now = 60001; } });
  await assert.rejects(value.agent(value.input), /total deadline exceeded/);
  assert.equal(value.calls.length, 1);
});
