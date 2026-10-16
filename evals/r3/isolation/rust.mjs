// SPDX-License-Identifier: AGPL-3.0-or-later
// Dependency-free black-box Rust grading. Expected outputs never enter guests.
import { createHash } from 'node:crypto';
import { runOfflineContainer, validateInput } from './container.mjs';

const harness = `
const {spawnSync}=require('node:child_process');
const fs=require('node:fs');
const probe=JSON.parse(fs.readFileSync('/work/harness/probe.json','utf8'));
const compiled=spawnSync('/usr/local/bin/rustc',[
  '--edition=2024','-C','debuginfo=0','/work/submission/src/main.rs','-o','/work/program'
],{encoding:'utf8',timeout:20000,maxBuffer:1048576});
if(compiled.error || compiled.signal) process.exit(70);
if(compiled.status!==0) process.exit(65);
const run=spawnSync('/work/program',probe.argv,{
  input:probe.stdin,encoding:'utf8',timeout:5000,maxBuffer:1048576
});
// An evaluated program may exit without consuming stdin. That can race the
// host pipe writer (EPIPE); it is a failed submission, not missing infrastructure.
if(run.error) process.exit(run.error.code === 'EPIPE' ? 66 : 70);
if(run.status!==0) process.exit(66);
process.stdout.write(run.stdout);
`;
const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');

export function validateRustCase(options) {
  if (options.execution !== undefined) throw Error('native Rust-blackbox grading is not integrated; no Linux fallback');
  const { image, files, probes, timeoutMs = 30000 } = options;
  if (!files || !Object.hasOwn(files, 'src/main.rs')) throw Error('Rust binary entry point required');
  if (!Array.isArray(probes) || probes.length < 1 || probes.length > 32) throw Error('invalid Rust probes');
  const ids = new Set();
  for (const probe of probes) {
    if (!probe || typeof probe.id !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(probe.id) || ids.has(probe.id)) throw Error('invalid probe identity');
    ids.add(probe.id);
    if (!Array.isArray(probe.argv) || probe.argv.length > 32 || probe.argv.some(a => typeof a !== 'string' || a.includes('\0') || a.length > 4096)) throw Error('invalid probe arguments');
    if (typeof probe.stdin !== 'string' || Buffer.byteLength(probe.stdin) > 65536
        || typeof probe.expectedStdout !== 'string' || Buffer.byteLength(probe.expectedStdout) > 65536) throw Error('invalid probe text');
    validateInput({ image, files: guestFiles(files, probe), argv: ['/usr/local/bin/node', '/work/harness/run.cjs'], timeoutMs, profile: 'rust', exportPaths: [] });
  }
}

function guestFiles(files, probe) {
  return Object.fromEntries([
    ...Object.entries(files).map(([p, bytes]) => [`submission/${p}`, bytes]),
    ['harness/run.cjs', harness],
    // Probe inputs are necessarily disclosed at execution; answers are not.
    ['harness/probe.json', JSON.stringify({ argv: probe.argv, stdin: probe.stdin })],
  ]);
}

export async function gradeRustCase(options) {
  validateRustCase(options);
  const { image, files, probes, timeoutMs = 30000 } = structuredClone(options);
  const started = performance.now();
  // A missing compiler or non-executable mount is broken measurement, not a
  // failed answer. Verify the toolchain independently of submitted code.
  const ready = await runOfflineContainer({ image, files: guestFiles({
    'src/main.rs': 'fn main() { println!("r3-toolchain-ready"); }',
  }, { argv: [], stdin: '' }), argv: ['/usr/local/bin/node', '/work/harness/run.cjs'],
  timeoutMs, profile: 'rust', exportPaths: [] });
  if (ready.exitCode !== 0 || ready.stdout !== 'r3-toolchain-ready\n') throw Error('Rust grading toolchain unavailable');
  const results = [];
  for (const probe of probes) {
    const result = await runOfflineContainer({ image, files: guestFiles(files, probe),
      argv: ['/usr/local/bin/node', '/work/harness/run.cjs'], timeoutMs, profile: 'rust', exportPaths: [] });
    if (result.exitCode === 70) throw Error('Rust grading execution unavailable');
    results.push({ id: probe.id, passed: result.exitCode === 0 && result.stdout === probe.expectedStdout,
      exitCode: result.exitCode, elapsedMs: result.elapsedMs });
  }
  return { version: 1, kind: 'offline-rust-blackbox', officialEvidence: false,
    image, sourceSha256: hash(Object.entries(files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)),
    probeSha256: hash(probes), success: results.every(p => p.passed),
    elapsedMs: performance.now() - started, probes: results };
}
