// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted fake I/O verifies bootstrap contracts; it is not native OS evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { rehearse } from './rehearse.mjs';
import { nativeProfileHash } from './isolation/native-windows.mjs';
import { validateExecutionInput } from './isolation/execution.mjs';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const uuid = '12345678-1234-4234-8234-123456789abc';

function fixture({ native = true, jobChange, ioChange, helperFailure, corruptCopy, corruptPin } = {}) {
  const paths = native ? path.win32 : path;
  const root = native ? `C:\\private\\runs\\${uuid}\\pack` : '/private/pack';
  const file = name => ({ path: `C:\\private\\tools\\${name}`, sha256: hash(name) });
  const node = file('node.exe'), guest = file('guest.mjs'), helper = file('helper.exe'), compiler = file('rustc.exe');
  const bins = { memory: file('memory.exe'), snapshot: file('r3_snapshot.exe'), score: file('r3_score.exe'), retriever: file('r3_retriever.exe') };
  const profile = { schema_version: 1, helper, io_root: 'C:\\private\\runs', node, guest,
    toolchains: [{ root: 'C:\\private\\tools', files: [node, guest, compiler, ...Object.values(bins)] }],
    search_path: ['C:\\private\\tools'], library_path: [] };
  const execution = { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) };
  const bytes = new Map([node, guest, helper, compiler, ...Object.values(bins)].map(pin => [pin.path, Buffer.from(paths.basename(pin.path))]));
  const executionFile = 'C:\\private\\execution.json'; bytes.set(executionFile, Buffer.from(JSON.stringify(execution)));
  const environment = { CODIFIX_TEST_REQUIRE_DB: '1', CODIFIX_TEST_DATABASE_URL: 'postgresql://PRIVATE_SECRET@localhost/disposable',
    MEMORY_R3_CLI_BIN: bins.memory.path, MEMORY_R3_SNAPSHOT_BIN: bins.snapshot.path,
    MEMORY_R3_SCORE_BIN: bins.score.path, MEMORY_R3_RETRIEVER_BIN: bins.retriever.path,
    SystemRoot: 'C:\\Windows', OPENAI_API_KEY: 'PROVIDER_SECRET', ANTHROPIC_API_KEY: 'PROVIDER_SECRET',
    ...(native ? { MEMORY_R3_NATIVE_EXECUTION: executionFile, MEMORY_R3_NATIVE_COMPILER: compiler.path }
      : { MEMORY_R3_TEST_IMAGE: `sha256:${hash('node-image')}`, MEMORY_R3_RUST_IMAGE: `sha256:${hash('rust-image')}` }) };
  if (!native) for (const key of ['MEMORY_R3_CLI_BIN', 'MEMORY_R3_SNAPSHOT_BIN', 'MEMORY_R3_SCORE_BIN', 'MEMORY_R3_RETRIEVER_BIN']) {
    const value = Buffer.from(key); environment[key] = '/reviewed/' + key; bytes.set(environment[key], value);
  }
  const writes = [], directories = [], copies = [], chmods = [], removals = [], runs = [], helpers = [], logs = [];
  let privateReady = false;
  const dependencies = { platform: native ? 'win32' : 'linux', processExecutable: native ? node.path : '/reviewed/node', callerPid: 123,
    log: line => logs.push(JSON.parse(line)), fileSystem: {
      readFile: async name => bytes.has(name) ? name === corruptPin ? Buffer.from('changed') : bytes.get(name) : fs.readFile(name),
      stat: async name => ({ isFile: () => true, size: bytes.get(name).length }),
      realpath: async name => name,
      mkdir: async (name, settings) => {
        if (native) { assert(privateReady, 'private ACL preflight must precede every directory write'); assert.equal(settings?.mode, undefined); }
        directories.push(name);
      },
      copyFile: async (from, to, flags) => { copies.push({ from, to, flags }); bytes.set(to, Buffer.from(corruptCopy === from ? 'changed' : bytes.get(from))); },
      chmod: async (name, mode) => chmods.push({ name, mode }),
      mkdtemp: async () => { assert(!native); return '/temporary/controller'; },
      writeFile: async (name, value, settings) => {
        if (settings?.flag === 'wx') assert(!bytes.has(name), 'exclusive artifact creation');
        if (native) assert.equal(settings?.mode, undefined);
        writes.push(name); bytes.set(name, Buffer.from(value));
      },
      rm: async name => { removals.push(name); },
    },
    runProcess: async (binary, args, settings) => {
      assert(args[0].endsWith('build-representative-pack.mjs'), 'prepare-only must not launch a controller');
      if (native) assert(privateReady);
      assert.equal(settings.env.CODIFIX_TEST_DATABASE_URL, undefined);
      assert.equal(settings.env.OPENAI_API_KEY, undefined);
      runs.push({ binary, args, settings });
      bytes.set(paths.join(root, 'campaign.json'), Buffer.from(JSON.stringify({ smoke_only: true, label_status: 'draft',
        limits: { microusd: 0, requests: 75 }, assets: [], model: {}, cases: [{ sources: ['public/source.md'] }] })));
      return 0;
    },
    invokeHelper: async (binary, argv, env) => {
      helpers.push({ binary, argv, env });
      assert.equal(writes.length, 0); assert.equal(directories.length, 0);
      assert.equal(env.OPENAI_API_KEY, undefined); assert.equal(env.CODIFIX_TEST_DATABASE_URL, undefined);
      if (helperFailure === argv[0]) throw Error('private helper failure');
      if (argv[0] === '--verify-outer-job') {
        const value = { format: 'memory-windows-trusted-outer-job-v1', caller_pid: 123, helper_pid: 124, kill_on_close: true, breakaway_allowed: false };
        jobChange?.(value); return { stdout: JSON.stringify(value) };
      }
      assert.deepEqual(argv, ['--prepare-io', profile.io_root, '--request-id', uuid]);
      const value = { format: 'memory-windows-evaluation-io-v1', request_id: uuid,
        directory: paths.dirname(root), root_identity: ['a', '1'], directory_identity: ['a', '2'] };
      ioChange?.(value); privateReady = true; return { stdout: JSON.stringify(value) };
    },
  };
  const argv = [root, ...(native ? ['--native'] : []), '--one-family-per-track', '--prepare-only'];
  return { root, argv, environment, dependencies, execution, bytes, writes, directories, copies, chmods, removals, runs, helpers, logs };
}

test('explicit native bootstrap locks executable .exe copies, Node sidecar and runtime closure', async () => {
  const f = fixture(), result = await rehearse(f.argv, f.environment, f.dependencies);
  assert.equal(f.helpers.length, 2); assert.equal(f.runs.length, 1); assert.equal(f.chmods.length, 0);
  assert.equal(result.manifest.smoke_only, true); assert.equal(result.manifest.provider_budget, undefined);
  const config = result.configuration;
  assert.deepEqual(config.dispatch, { kind: 'windows-loopback-v1', endpoint: 'private/dispatch-endpoint.json' });
  assert.equal(config.images, undefined); assert.equal(config.socket, undefined); assert.equal(config.syntheticWire, false);
  assert(Object.values(config.binaries).every(pin => pin.path.startsWith(f.root + '\\private\\executables\\') && pin.path.endsWith('.exe')));
  assert.equal(config.nativeScorer.path, f.root + '\\private\\executables\\r3_score.exe');
  assert.equal(config.compiler.path, f.environment.MEMORY_R3_NATIVE_COMPILER);
  assert.equal(config.execution.profile.node.path, config.binaries.node.path);
  assert.equal(config.execution.profile.guest.path, f.root + '\\private\\native-driver\\guest.mjs');
  assert.equal(config.execution.profile_sha256, nativeProfileHash(config.execution.profile));
  validateExecutionInput({ execution: config.execution, argv: [config.binaries.node.path], files: {}, exportPaths: [] });
  const sidecar = JSON.parse(f.bytes.get(f.root + '\\runtime\\offline-driver.mjs.windows-runtime.json'));
  assert.deepEqual(sidecar, { schema_version: 1, node: 'private/executables/node.exe', node_sha256: config.binaries.node.sha256 });
  const assets = result.manifest.assets;
  for (const pin of Object.values(config.binaries)) assert(assets.some(asset =>
    path.win32.join(f.root, asset.path) === pin.path && asset.sha256 === pin.sha256 && asset.kind === 'configuration'));
  for (const module of ['execution', 'native-windows', 'native-guest']) assert(assets.some(asset => asset.path === `runtime/isolation/${module}.mjs`));
  for (const asset of assets.filter(asset => asset.path.startsWith('runtime/') && asset.path.endsWith('.mjs'))) {
    const text = f.bytes.get(path.win32.join(f.root, asset.path)).toString('utf8');
    for (const [, dependency] of text.matchAll(/(?:import|export)\s+(?:[^'";]+?\s+from\s*)?['"](\.{1,2}\/[^'"]+\.mjs)['"]/g)) {
      const selected = path.posix.normalize(path.posix.join(path.posix.dirname(asset.path), dependency));
      assert(assets.some(asset => asset.path === selected), `${asset.path} needs locked ${selected}`);
    }
  }
  assert(assets.some(asset => asset.path === 'runtime/offline-driver.mjs.windows-runtime.json'));
  assert(!JSON.stringify(config).includes('SECRET')); assert(!JSON.stringify(result.manifest).includes('SECRET'));
  assert.deepEqual(f.removals, [path.win32.join(path.win32.dirname(f.root), 'controller')]);
  assert.deepEqual(f.logs, [{ prepared: true, root: f.root, native: true, official_evidence: false, execution_started: false }]);
});

test('native invalid profile, Job, I/O receipt and paid modes fail before output writes', async () => {
  for (const settings of [
    { helperFailure: '--verify-outer-job' }, { helperFailure: '--prepare-io' },
    { corruptPin: 'C:\\private\\tools\\helper.exe' }, { corruptPin: 'C:\\private\\tools\\memory.exe' },
    { jobChange: receipt => { receipt.caller_pid++; } }, { jobChange: receipt => { receipt.kill_on_close = false; } },
    { jobChange: receipt => { receipt.breakaway_allowed = true; } },
    { ioChange: receipt => { receipt.directory = 'C:\\public'; } }, { ioChange: receipt => { receipt.root_identity = []; } },
  ]) {
    const f = fixture(settings);
    await assert.rejects(rehearse(f.argv, f.environment, f.dependencies));
    assert.equal(f.writes.length, 0); assert.equal(f.directories.length, 0); assert.equal(f.runs.length, 0);
  }
  for (const change of [
    f => { f.argv.push('--wire'); }, f => { f.argv.push('--mediated'); },
    f => { f.environment.MEMORY_R3_NATIVE_COMPILER = 'C:\\unreviewed\\rustc.exe'; },
    f => { f.environment.MEMORY_R3_TEST_IMAGE = `sha256:${hash('docker')}`; },
    f => { f.dependencies.platform = 'linux'; }, f => { f.dependencies.processExecutable = 'C:\\wrong\\node.exe'; },
    f => { f.argv[0] = 'C:\\public\\pack'; }, f => { f.argv.splice(f.argv.indexOf('--native'), 1); },
  ]) {
    const f = fixture(); change(f);
    await assert.rejects(rehearse(f.argv, f.environment, f.dependencies));
    assert.equal(f.writes.length, 0); assert.equal(f.directories.length, 0); assert.equal(f.runs.length, 0);
  }
});

test('native copy corruption is rejected and execution remains closed pending protected IPC integration', async () => {
  const corrupt = fixture({ corruptCopy: 'C:\\private\\tools\\memory.exe' });
  await assert.rejects(rehearse(corrupt.argv, corrupt.environment, corrupt.dependencies), /changed while preparing/);
  assert.equal(corrupt.writes.length, 0);
  const f = fixture(); f.argv.splice(f.argv.indexOf('--prepare-only'), 1);
  await assert.rejects(rehearse(f.argv, f.environment, f.dependencies), /protected controller IPC integration/);
  assert.equal(f.runs.length, 1);
  assert(f.bytes.has(f.root + '\\private\\runtime.json'));
});

test('Linux bootstrap retains frozen extensionless images and includes native import dependencies', async () => {
  const f = fixture({ native: false }), result = await rehearse(f.argv, f.environment, f.dependencies);
  assert.equal(f.helpers.length, 0); assert.equal(f.chmods.length, 4); assert(f.chmods.every(value => value.mode === 0o500));
  assert(Object.values(result.configuration.binaries).every(pin => !pin.path.endsWith('.exe')));
  assert(result.configuration.images.node.startsWith('sha256:'));
  assert.equal(result.configuration.socket, '/temporary/controller/dispatch.sock');
  assert.equal(result.configuration.execution, undefined);
  for (const name of ['execution', 'native-windows', 'native-guest']) assert(result.manifest.assets.some(asset => asset.path === `runtime/isolation/${name}.mjs`));
});
