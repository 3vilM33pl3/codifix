// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { gradeRustCase, validateRustCase } from './rust.mjs';
import { runOfflineSequence } from './sequence.mjs';

const image = process.env.MEMORY_R3_RUST_IMAGE;
if (process.env.MEMORY_R3_REQUIRE_RUST === '1' && !image) throw Error('required Rust image missing');
const files = {
  'src/main.rs': `mod policy;
fn main() {
  let args: Vec<String> = std::env::args().collect();
  let input: u64 = args[1].parse().unwrap();
  println!("{}", policy::retry(input));
}`,
  'src/policy.rs': 'pub fn retry(attempt: u64) -> u64 { 10 * 2u64.pow(attempt.min(5) as u32) }',
};
const probes = [
  { id: 'initial', argv: ['0'], stdin: '', expectedStdout: '10\n' },
  { id: 'backoff', argv: ['3'], stdin: '', expectedStdout: '80\n' },
  { id: 'bounded', argv: ['99'], stdin: '', expectedStdout: '320\n' },
];
test('Rust grading validates all probes before dispatch', () => {
  const valid = { image: `sha256:${'a'.repeat(64)}`, files, probes };
  assert.doesNotThrow(() => validateRustCase(valid));
  for (const changes of [
    { files: {} }, { probes: [] }, { probes: [probes[0], probes[0]] },
    { probes: [...probes, { ...probes[0], id: 'late', argv: ['\0'] }] },
    { files: { ...files, '../harness/run.cjs': 'override' } },
  ]) assert.throws(() => validateRustCase({ ...valid, ...changes }));
});

test('three-step Rust sequence retains source only and grades outside the program', { skip: !image, timeout: 120000 }, async () => {
  const steps = [10, 20, 30].map((base, i) => ({
    id: `step-${i + 1}`,
    argv: ['/usr/local/bin/node', '-e', `
      const fs=require('fs'),assert=require('assert');
      assert(!fs.existsSync('harness'));assert(!fs.existsSync('grader'));
      assert(!fs.existsSync('target'));
      ${i ? `assert(fs.readFileSync('src/policy.rs','utf8').includes('${base - 10} *'));` : ''}
      fs.writeFileSync('src/policy.rs',${JSON.stringify(`pub fn retry(attempt: u64) -> u64 { ${base} * 2u64.pow(attempt.min(5) as u32) }`)});
      fs.mkdirSync('target');fs.writeFileSync('target/artifact','discard');
    `],
    grader: { kind: 'rust-blackbox', probes: [{ id: 'contract', argv: ['0'], stdin: '', expectedStdout: `${base}\n` }] },
  }));
  const options = { image, initialFiles: files, submissionPaths: Object.keys(files), steps, timeoutMs: 30000 };
  const positive = await runOfflineSequence(options);
  assert.equal(positive.success, true);
  assert(positive.steps.every(s => s.graderKind === 'rust-blackbox'));
  const negative = structuredClone(options);
  negative.steps[1].argv = ['/usr/local/bin/node', '-e', ''];
  // Keep the third agent independent of the failed second-stage assertion.
  negative.steps[2].argv = ['/usr/local/bin/node', '-e', `require('fs').writeFileSync('src/policy.rs','pub fn retry(_: u64) -> u64 { 30 }')`];
  const failed = await runOfflineSequence(negative);
  assert.equal(failed.success, false);
  assert.deepEqual(failed.steps.map(s => s.passed), [true, false, true]);
  assert(!JSON.stringify(positive.submission).includes('expectedStdout'));
});

test('real Rust: multi-file success, wrong answer, compile failure and hidden expectations', { skip: !image, timeout: 120000 }, async () => {
  const result = await gradeRustCase({ image, files, probes });
  assert.equal(result.success, true);
  assert.equal(result.probes.length, 3);
  assert.equal(result.officialEvidence, false);
  const wrong = await gradeRustCase({ image, files: { 'src/main.rs': 'fn main() { println!("passed"); }' }, probes: [probes[0]] });
  assert.equal(wrong.success, false); // Exit zero is not a grading verdict.
  const broken = await gradeRustCase({ image, files: { 'src/main.rs': 'not Rust' }, probes: [probes[0]] });
  assert.equal(broken.success, false);
  assert.equal(broken.probes[0].exitCode, 65);
  const privacy = await gradeRustCase({ image, files: { 'src/main.rs': `fn main() {
    let probe = std::fs::read_to_string("/work/harness/probe.json").unwrap();
    assert!(!probe.contains("expectedStdout"));
    assert!(!probe.contains("HOST_ONLY_EXPECTATION"));
    assert!(!std::path::Path::new("/private").exists());
    println!("no expected output available");
  }` }, probes: [{ id: 'privacy', argv: [], stdin: '', expectedStdout: 'HOST_ONLY_EXPECTATION' }] });
  assert.equal(privacy.success, false);
  assert.equal(privacy.probes[0].exitCode, 0);
  assert(!JSON.stringify(privacy).includes('HOST_ONLY_EXPECTATION'));
});
