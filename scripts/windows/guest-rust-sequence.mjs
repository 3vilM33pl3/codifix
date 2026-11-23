// SPDX-License-Identifier: AGPL-3.0-or-later
// Reuse a successful private toolchain proof; do not recopy the SDK per case.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
const [source, root] = process.argv.slice(2);
if (process.platform !== 'win32' || process.env.COMPUTERNAME !== 'MEMORY-WINLAB'
    || !/^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$/.test(source ?? '')
    || !/^C:\\MemoryLab\\runs\\[0-9a-f-]{36}\\native-rust-eval$/.test(root ?? '')) throw Error('Explicit disposable proof required');
const previous = JSON.parse(await fs.readFile(path.join(root, 'result.json')));
assert.equal(previous.state, 'succeeded');
assert.equal(previous.positive_compilation_verified, true);
// The previous actual wrong-output run must have executed normally, not failed
// for an unrelated compiler/runtime error. Retained readiness/probe order is fixed.
assert.deepEqual(previous.observations.map(o => o.exitCode), [0,0,0,0,0,65]);
const profile = JSON.parse(await fs.readFile(path.join(root, 'profile.json')));
const { nativeProfileHash } = await import(pathToFileURL(path.join(source, 'evals/r3/isolation/native-windows.mjs')));
assert.equal(nativeProfileHash(profile), previous.profile_sha256);
const { runOfflineSequence } = await import(pathToFileURL(path.join(source, 'evals/r3/isolation/sequence.mjs')));
const execution = { kind: 'windows-native-v1', profile, profile_sha256: previous.profile_sha256 };
const steps = [1,9,10].map((value, i) => ({
  id: `step-${i + 1}`,
  argv: [profile.node.path, '-e', `
    const fs=require('node:fs'),p='src/main.rs';
    const expected=${[0,1,9][i]};
    if(!fs.readFileSync(p,'utf8').includes('println!("'+expected+'")')) process.exit(42);
    if(fs.existsSync('unselected.txt')) process.exit(43);
    fs.writeFileSync(p,'fn main(){println!("${value}");}');
    fs.writeFileSync('unselected.txt','not carried to the next step');
  `],
  grader: { kind: 'rust-blackbox', compiler: previous.compiler,
    probes: [{ id: `hidden-${i}`, argv: [], stdin: '', expectedStdout: `${[1,2,10][i]}\n` }] },
}));
const result = await runOfflineSequence({ execution, initialFiles: { 'src/main.rs': 'fn main(){println!("0");}' },
  submissionPaths: ['src/main.rs'], steps, timeoutMs: 60000 });
assert.equal(result.success, false);
assert.deepEqual(result.steps.map(s => s.passed), [true,false,true]);
assert.deepEqual(Object.keys(result.submission), ['src/main.rs']);
assert(result.submission['src/main.rs'].includes('println!("10")'));
assert.equal(result.steps[1].graderProbes[0].exitCode, 0);
await fs.writeFile(path.join(root, 'sequence-proof.json'), JSON.stringify({
  format: 'memory-native-rust-sequence-proof-v1', state: 'succeeded',
  real_provider_requests: 0, official_evidence: false, campaign_verified: false,
  result, selected_source_only: true, failed_middle_step_retained: true,
}), { flag: 'wx' });
