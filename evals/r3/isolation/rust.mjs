// SPDX-License-Identifier: AGPL-3.0-or-later
// Dependency-free black-box Rust grading. Expected outputs never enter guests.
import { createHash } from 'node:crypto';
import path from 'node:path';
import { runOfflineExecution, validateExecutionInput, validateExecutionOutput } from './execution.mjs';

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

function nativeHarness(compiler) {
  // Compiler identity is checked against the operator's complete toolchain
  // inventory before dispatch. Only public probe inputs enter this scratch.
  return `
const {spawnSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path');
const probe=JSON.parse(fs.readFileSync('harness/probe.json','utf8'));
const failed=r=>r.error||r.signal||[0xc0000135,0xc0000142].includes(r.status>>>0);
// Avoid libuv UV_CREATE_PIPE's non-LOCAL named-pipe namespace in AppContainer.
// Probe inputs, not hidden expectations, enter these scratch files. Results
// remain untrusted and bounded; the outer helper owns the process-tree deadline.
const execute=(exe,args,input,timeout)=>{
  const root=fs.mkdtempSync(path.join('harness','stdio-')),fds=[];
  try {
    fs.writeFileSync(path.join(root,'stdin'),input,{flag:'wx'});
    fds.push(fs.openSync(path.join(root,'stdin'),'r'));
    for(const name of ['stdout','stderr'])fds.push(fs.openSync(path.join(root,name),'wx+'));
    const result=spawnSync(exe,args,{timeout,windowsHide:true,stdio:fds});
    const sizes=fds.slice(1).map(fd=>fs.fstatSync(fd).size);
    if(sizes[0]+sizes[1]>1048576)return {error:true};
    const read=(fd,size)=>{const bytes=Buffer.alloc(size);let n=0;while(n<size){const count=fs.readSync(fd,bytes,n,size-n,n);if(!count)throw Error('changed output');n+=count;}return bytes.toString('utf8');};
    return {...result,stdout:read(fds[1],sizes[0]),stderr:read(fds[2],sizes[1])};
  } catch { return {error:true}; }
  finally {for(const fd of fds)fs.closeSync(fd);}
};
const compiled=execute(${JSON.stringify(compiler.path)},[
  '--edition=2024','-C','debuginfo=0','submission/src/main.rs','-o','program.exe'
],'',20000);
if(failed(compiled)) process.exit(70);
if(compiled.status!==0) process.exit(65);
const run=execute(path.resolve('program.exe'),probe.argv,probe.stdin,5000);
if(run.error?.code==='EPIPE') process.exit(66);
if(failed(run)) process.exit(70);
if(run.status!==0) process.exit(66);
process.stdout.write(run.stdout);
`;
}

function boundary({ image, execution, compiler }) {
  if (execution?.kind === 'windows-native-v1') {
    if (image !== undefined || !compiler || Object.keys(compiler).sort().join(',') !== 'path,sha256'
        || typeof compiler.path !== 'string' || !path.win32.isAbsolute(compiler.path)
        || path.win32.basename(compiler.path).toLowerCase() !== 'rustc.exe'
        || !/^[a-f0-9]{64}$/.test(compiler.sha256)) throw Error('native Rust requires an explicit pinned compiler, not a Linux image');
    const matches = execution.profile?.toolchains?.flatMap(tree => tree.files ?? []).filter(file =>
      typeof file.path === 'string' && file.path.toLowerCase() === compiler.path.toLowerCase());
    if (matches?.length !== 1 || matches[0].sha256 !== compiler.sha256) throw Error('native Rust compiler absent from reviewed toolchain');
    return { execution };
  }
  if (compiler !== undefined) throw Error('native compiler cannot enter Linux grading');
  return execution === undefined ? { image } : { image, execution };
}

function invocation(options, files, probe) {
  const contract = boundary(options);
  const native = options.execution?.kind === 'windows-native-v1';
  return { ...contract, files: guestFiles(files, probe, native ? nativeHarness(options.compiler) : harness),
    argv: native ? [options.execution.profile.node.path, 'harness/run.cjs'] : ['/usr/local/bin/node', '/work/harness/run.cjs'],
    timeoutMs: options.timeoutMs ?? 30000, profile: 'rust', exportPaths: [] };
}

export function validateRustCase(options) {
  boundary(options);
  const { files, probes } = options;
  if (!files || !Object.hasOwn(files, 'src/main.rs')) throw Error('Rust binary entry point required');
  if (!Array.isArray(probes) || probes.length < 1 || probes.length > 32) throw Error('invalid Rust probes');
  const ids = new Set();
  for (const probe of probes) {
    if (!probe || typeof probe.id !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(probe.id) || ids.has(probe.id)) throw Error('invalid probe identity');
    ids.add(probe.id);
    if (!Array.isArray(probe.argv) || probe.argv.length > 32 || probe.argv.some(a => typeof a !== 'string' || a.includes('\0') || a.length > 4096)) throw Error('invalid probe arguments');
    if (typeof probe.stdin !== 'string' || Buffer.byteLength(probe.stdin) > 65536
        || typeof probe.expectedStdout !== 'string' || Buffer.byteLength(probe.expectedStdout) > 65536) throw Error('invalid probe text');
    validateExecutionInput(invocation(options, files, probe));
  }
}

function guestFiles(files, probe, script) {
  return Object.fromEntries([
    ...Object.entries(files).map(([p, bytes]) => [`submission/${p}`, bytes]),
    ['harness/run.cjs', script],
    // Probe inputs are necessarily disclosed at execution; answers are not.
    ['harness/probe.json', JSON.stringify({ argv: probe.argv, stdin: probe.stdin })],
  ]);
}

async function grade(options, runExecution) {
  validateRustCase(options);
  const frozen = structuredClone(options);
  const { image, execution, files, probes } = frozen;
  const invoke = async (source, probe) => {
    const input = invocation(frozen, source, probe);
    const result = structuredClone(await runExecution(input));
    validateExecutionOutput(input, result);
    return result;
  };
  const started = performance.now();
  // A missing compiler or non-executable mount is broken measurement, not a
  // failed answer. Verify the toolchain independently of submitted code.
  const ready = await invoke({
    'src/main.rs': 'fn main() { println!("r3-toolchain-ready"); }',
  }, { argv: [], stdin: '' });
  if (ready.exitCode !== 0 || ready.stdout !== 'r3-toolchain-ready\n') throw Error('Rust grading toolchain unavailable');
  const results = [];
  for (const probe of probes) {
    const result = await invoke(files, probe);
    if (result.exitCode === 70) throw Error('Rust grading execution unavailable');
    results.push({ id: probe.id, passed: result.exitCode === 0 && result.stdout === probe.expectedStdout,
      exitCode: result.exitCode, elapsedMs: result.elapsedMs,
      ...(execution?.kind === 'windows-native-v1' ? { execution: result.execution } : {}) });
  }
  return { version: 1, kind: execution?.kind === 'windows-native-v1' ? 'native-windows-rust-blackbox' : 'offline-rust-blackbox', officialEvidence: false,
    ...(execution?.kind === 'windows-native-v1' ? { execution: { kind: execution.kind,
      profile_sha256: execution.profile_sha256, disk_hard_limit_enforced: false }, compiler: frozen.compiler,
      readinessExecution: ready.execution } : { image }),
    sourceSha256: hash(Object.entries(files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)),
    probeSha256: hash(probes), success: results.every(p => p.passed),
    elapsedMs: performance.now() - started, probes: results };
}

export function createRustGrader({ runExecution = runOfflineExecution } = {}) {
  if (typeof runExecution !== 'function') throw Error('invalid trusted Rust execution adapter');
  return options => grade(options, runExecution);
}
export const gradeRustCase = createRustGrader();
