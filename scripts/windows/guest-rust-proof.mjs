// SPDX-License-Identifier: AGPL-3.0-or-later
// Actual native compiler controls; no provider, label approval or campaign claim.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
const [source, root] = process.argv.slice(2);
if (process.platform !== 'win32' || process.env.COMPUTERNAME !== 'MEMORY-WINLAB'
    || !/^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$/.test(source ?? '')
    || !/^C:\\MemoryLab\\runs\\[0-9a-f-]{36}\\native-rust-eval$/.test(root ?? '')) throw Error('Disposable proof only');
const load = name => import(pathToFileURL(path.join(source, 'evals/r3/isolation', name)));
const { nativeProfileHash } = await load('native-windows.mjs');
const { createRustGrader } = await load('rust.mjs');
const { runOfflineExecution } = await load('execution.mjs');
const inventory = JSON.parse(await fs.readFile(path.join(root, 'inventory.json')));
const bound = async file => ({ path: file, sha256: createHash('sha256').update(await fs.readFile(file)).digest('hex') });
const code = path.join(root, 'code');
const node = await bound(path.join(code, 'node.exe'));
const compiler = await bound(path.join(code, 'rust/bin/rustc.exe'));
const profile = { schema_version: 1, helper: await bound(path.join(root, 'evaluation-helper.exe')),
  io_root: path.join(root, 'io'), node, guest: await bound(path.join(code, 'native-guest.mjs')),
  toolchains: [{ root: code, files: inventory.files }],
  search_path: [code, path.join(code, 'rust/bin'), path.join(code, 'vc/bin')],
  library_path: ['vc/lib','sdk/um','sdk/ucrt'].map(p => path.join(code, p)) };
const execution = { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) };
await fs.writeFile(path.join(root, 'profile.json'), JSON.stringify(profile), { flag: 'wx' });
const observations = [];
const grader = createRustGrader({ async runExecution(options) {
  const result = await runOfflineExecution(options);
  observations.push({ execution: result.execution, exitCode: result.exitCode, privateOperation: result.privateOperation });
  return result;
} });
const options = { execution, compiler, timeoutMs: 60000,
  files: { 'src/main.rs': 'fn main(){println!("correct");}' },
  probes: [{ id: 'synthetic', argv: [], stdin: '', expectedStdout: 'correct\n' }] };
const positive = await grader(options);
assert.equal(positive.success, true);
const negative = await grader({ ...options, files: { 'src/main.rs': 'fn main(){println!("wrong");}' } });
assert.equal(negative.success, false);
assert.equal(negative.probes[0].exitCode, 0, 'wrong output must fail scoring, not compilation or execution');
const broken = await grader({ ...options, files: { 'src/main.rs': 'not valid rust' } });
assert.equal(broken.success, false);
assert.equal(broken.probes[0].exitCode, 65);
await fs.writeFile(path.join(root, 'result.json'), JSON.stringify({
  format: 'memory-native-rust-evaluation-proof-v1', state: 'succeeded',
  compiler, profile_sha256: execution.profile_sha256, observations,
  positive_compilation_verified: true, wrong_answer_rejected: true, broken_source_rejected: true,
  real_provider_requests: 0, official_evidence: false, campaign_verified: false,
  sequence_verified: false, disk_hard_limit_enforced: false,
}), { flag: 'wx' });
