// SPDX-License-Identifier: AGPL-3.0-or-later
// Fake execution verifies adapter contracts, never native OS acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRustGrader, validateRustCase } from './rust.mjs';
import { createSequenceRunner } from './sequence.mjs';
import { nativeProfileHash } from './native-windows.mjs';
const sha = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
function fixture() {
  const file = name => ({ path: `C:\\private\\tools\\${name}`, sha256: 'a'.repeat(64) });
  const node = file('node.exe'), guest = file('guest.mjs'), compiler = file('rustc.exe');
  const profile = { schema_version: 1, helper: file('helper.exe'), io_root: 'C:\\private\\runs', node, guest,
    toolchains: [{ root: 'C:\\private\\tools', files: [node, guest, compiler] }],
    search_path: ['C:\\private\\tools'], library_path: [] };
  return { execution: { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) }, compiler,
    files: { 'src/main.rs': 'fn main() { println!("answer-1"); }' },
    probes: [{ id: 'contract', argv: ['1'], stdin: '', expectedStdout: 'answer-1\n' }], timeoutMs: 30000 };
}
function fake({ exitCode = 0, mismatch = false } = {}) {
  const calls = [];
  const runExecution = async input => {
    calls.push(structuredClone(input));
    await Promise.resolve();
    let stdout = '', files = {}, status = 0;
    if (input.files['harness/probe.json']) {
      const source = input.files['submission/src/main.rs'];
      assert(!JSON.stringify(input).includes('expectedStdout'));
      assert.equal(input.profile, 'rust'); assert.deepEqual(input.exportPaths, []);
      assert.deepEqual(input.argv, [input.execution.profile.node.path, 'harness/run.cjs']);
      assert(input.files['harness/run.cjs'].includes('submission/src/main.rs'));
      assert(!input.files['harness/run.cjs'].includes('/usr/local/'));
      if (source.includes('r3-toolchain-ready')) stdout = 'r3-toolchain-ready\n';
      else { stdout = (source.match(/answer-\d/)?.[0] ?? 'wrong') + '\n'; status = exitCode; }
    } else {
      assert(!Object.keys(input.files).some(name => name.startsWith('harness/')));
      const index = JSON.parse(input.files['task.json']).index;
      files['src/main.rs'] = Buffer.from(`fn main() { println!("answer-${index}"); }`).toString('base64');
    }
    return { version: 1, exitCode: status, stdout, stderr: '', files, elapsedMs: 1,
      execution: { kind: 'windows-native-v1', profile_sha256: mismatch ? '0'.repeat(64) : input.execution.profile_sha256,
        descriptor_sha256: sha(input), receipt_sha256: sha(calls.length), disk_hard_limit_enforced: false } };
  };
  return { calls, runExecution };
}
test('native Rust validates pinned compiler and all probes before execution', async () => {
  const value = fixture();
  assert.doesNotThrow(() => validateRustCase(value));
  for (const changed of [ { image: `sha256:${'a'.repeat(64)}` }, { compiler: undefined },
    { compiler: { ...value.compiler, sha256: 'b'.repeat(64) } },
    { compiler: { ...value.compiler, path: 'C:\\other\\rustc.exe' } },
    { probes: [...value.probes, { ...value.probes[0], id: 'late', argv: ['\0'] }] } ]) {
    const execution = fake();
    await assert.rejects(createRustGrader(execution)({ ...value, ...changed }));
    assert.equal(execution.calls.length, 0);
  }
});
test('native grading retains receipts and keeps expected output host-side', async () => {
  const execution = fake(), value = fixture();
  const pending = createRustGrader(execution)(value);
  value.probes[0].expectedStdout = 'caller mutation';
  const result = await pending;
  assert.equal(result.success, true); assert.equal(result.officialEvidence, false);
  assert.equal(result.kind, 'native-windows-rust-blackbox'); assert.equal(result.image, undefined);
  assert.equal(execution.calls.length, 2);
  assert(result.readinessExecution.receipt_sha256);
  assert(result.probes[0].execution.receipt_sha256);
  assert.equal(result.execution.disk_hard_limit_enforced, false);
  assert(!JSON.stringify(result).includes('expectedStdout'));
});
test('compile failure is negative; missing execution and changed receipts invalidate grading', async () => {
  const failed = await createRustGrader(fake({ exitCode: 65 }))(fixture());
  assert.equal(failed.success, false); assert.equal(failed.probes[0].exitCode, 65);
  await assert.rejects(createRustGrader(fake({ exitCode: 70 }))(fixture()), /execution unavailable/);
  await assert.rejects(createRustGrader(fake({ exitCode: 0xc0000135 }))(fixture()), /loader failure/);
  await assert.rejects(createRustGrader(fake({ mismatch: true }))(fixture()), /execution identity/);
});
test('three-step native Rust sequence carries only source and retains failed middle step', async () => {
  const { execution, compiler, files } = fixture();
  const options = { execution, initialFiles: files, submissionPaths: ['src/main.rs'], timeoutMs: 30000,
    steps: [1,2,3].map(index => ({ id: `step-${index}`, argv: [execution.profile.node.path, '-e', 'fixed agent'],
      inputFiles: { 'task.json': JSON.stringify({ index }) },
      grader: { kind: 'rust-blackbox', compiler, probes: [{ id: 'contract', argv: [], stdin: '', expectedStdout: `answer-${index}\n` }] } })) };
  const adapter = fake();
  const result = await createSequenceRunner(adapter)(options);
  assert.equal(result.success, true); assert.equal(adapter.calls.length, 9);
  assert(result.steps.every(step => step.graderReadinessExecution.receipt_sha256 && step.graderProbes[0].execution.receipt_sha256));
  const negative = structuredClone(options); negative.steps[1].grader.probes[0].expectedStdout = 'wrong';
  const failed = await createSequenceRunner(fake())(negative);
  assert.deepEqual(failed.steps.map(step => step.passed), [true, false, true]);
  assert.equal(failed.success, false);
});
