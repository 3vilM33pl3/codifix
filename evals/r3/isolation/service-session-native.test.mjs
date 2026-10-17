// SPDX-License-Identifier: AGPL-3.0-or-later
// Fake helpers exercise orchestration contracts, not native isolation evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import { createPreparedSessionOpener, openPreparedSession } from './service-session.mjs';
import { nativeProfileHash } from './native-windows.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const uuid = '12345678-1234-4234-8234-123456789abc';
const snapshot = hash('snapshot');

function fixture({ platform = 'win32', jobChange, ioChange, helperFailure, prepareFailure, corruptPin,
  serviceNotReady = false, shutdownFailure = false, configFailure = false } = {}) {
  const file = name => ({ path: `C:\\private\\tools\\${name}`, sha256: hash(name) });
  const helper = file('helper.exe'), node = file('node.exe'), guest = file('guest.mjs'),
    memory = file('memory.exe'), owner = file('snapshot.exe');
  const profile = { schema_version: 1, helper, io_root: 'C:\\private\\runs', node, guest,
    toolchains: [{ root: 'C:\\private\\tools', files: [node, guest, memory, owner] }],
    search_path: ['C:\\private\\tools'], library_path: [] };
  const execution = { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) };
  const options = { execution, snapshotBinary: owner.path, memoryBinary: memory.path,
    databaseUrl: 'postgresql://admin:DATABASE_SECRET@localhost/disposable', semanticTime: '2026-10-17T00:00:00Z',
    documents: [{ project: 'r3-alpha', source: 'public/source.md', text: 'Retain history.' }] };
  const events = [], writes = [], children = [], requests = [], helperCalls = [], removals = [];
  const directory = `${profile.io_root}\\${uuid}`;
  const pins = new Map([helper, node, guest, memory, owner].map(pin => [pin.path, Buffer.from(pin.path.split('\\').at(-1))]));
  const opener = createPreparedSessionOpener({ platform, callerPid: 123, requestId: () => uuid,
    reservePort: async () => 45678, wait: async () => {}, systemRoot: 'C:\\Windows',
    fileSystem: {
      stat: async name => ({ isFile: () => true, size: pins.get(name).length }),
      readFile: async name => name === corruptPin ? Buffer.from('changed') : pins.get(name),
      mkdtemp: async () => { events.push('mkdtemp'); return '/private/session'; },
      writeFile: async (name, bytes, settings) => {
        events.push('write-config');
        if (configFailure) throw Error('private secret write failure');
        writes.push({ name, bytes, settings });
      },
      rm: async (name, settings) => { events.push(settings?.recursive ? 'remove-directory' : 'remove-config'); removals.push(name); },
    },
    invokeHelper: async (binary, argv, env) => {
      helperCalls.push({ binary, argv, env }); events.push(argv[0]);
      assert.equal(writes.length, 0);
      if (helperFailure === argv[0]) throw Error('private helper diagnostic');
      if (argv[0] === '--verify-outer-job') {
        const receipt = { format: 'memory-windows-trusted-outer-job-v1', caller_pid: 123, helper_pid: 124,
          kill_on_close: true, breakaway_allowed: false };
        jobChange?.(receipt); return { stdout: JSON.stringify(receipt) };
      }
      assert.deepEqual(argv, ['--prepare-io', profile.io_root, '--request-id', uuid]);
      const receipt = { format: 'memory-windows-evaluation-io-v1', request_id: uuid, directory,
        root_identity: ['a', '1'], directory_identity: ['a', '2'] };
      ioChange?.(receipt); return { stdout: JSON.stringify(receipt) };
    },
    spawnProcess: (binary, argv, settings) => {
      const child = new EventEmitter();
      child.stdin = new PassThrough(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
      child.exitCode = null; child.signalCode = null;
      const isOwner = binary === options.snapshotBinary;
      events.push(isOwner ? 'spawn-owner' : 'spawn-service');
      const finish = (code, signal) => {
        child.exitCode = code; child.signalCode = signal;
        child.stdout.end(); child.stderr.end(); child.emit('close', code, signal);
      };
      child.kill = signal => { events.push(isOwner ? 'kill-owner' : 'kill-service'); queueMicrotask(() => finish(null, signal)); return true; };
      children.push({ child, binary, argv, settings });
      if (isOwner) {
        let pending = '';
        child.stdin.on('data', bytes => {
          pending += bytes.toString('utf8');
          while (pending.includes('\n')) {
            const index = pending.indexOf('\n'), request = JSON.parse(pending.slice(0,index)); pending = pending.slice(index + 1);
            requests.push(request); events.push(request.operation);
            if (request.operation === 'shutdown') { queueMicrotask(() => finish(shutdownFailure ? 1 : 0, null)); continue; }
            let response;
            if (request.operation === 'prepare') response = { prepared: !prepareFailure, official_evidence: false, snapshot_sha256: snapshot,
              bindings: [{ project: 'r3-alpha', source: 'public/source.md', memory_id: 'memory-1' }] };
            if (request.operation === 'fork') response = { snapshot_sha256: snapshot, id: uuid,
              database_url: 'postgresql://reader:CLONE_SECRET@localhost/disposable', project_tokens: { 'r3-alpha': 'SCOPED_TOKEN' } };
            if (request.operation === 'fingerprint') response = { snapshot_sha256: snapshot };
            if (request.operation === 'release') response = { released: true };
            queueMicrotask(() => child.stdout.write(JSON.stringify({ version: 1, ...response }) + '\n'));
          }
        });
      }
      return child;
    },
    fetchResponse: async (url, settings) => {
      if (url.endsWith('/healthz')) return { ok: !serviceNotReady };
      assert.equal(settings.headers.Authorization, 'Bearer SCOPED_TOKEN');
      assert.equal(JSON.parse(settings.body).project, 'r3-alpha');
      return new Response(JSON.stringify({ results: [{ memory_id: 'memory-1', project: 'r3-alpha' }] }));
    },
  });
  return { opener, options, events, writes, children, requests, helperCalls, removals, directory };
}

test('native prepared service verifies outer Job and private ACL before any credentials or child launch', async () => {
  const f = fixture(), session = await f.opener(f.options);
  assert.deepEqual(f.events.slice(0,4), ['--verify-outer-job', '--prepare-io', 'spawn-owner', 'prepare']);
  let stale;
  const result = await session.withInstance('r3-alpha', async instance => {
    assert(!JSON.stringify(instance).includes('SECRET'));
    stale = instance.query;
    await assert.rejects(session.withInstance('r3-alpha', async () => {}), /busy/);
    await assert.rejects(session.close(), /active/);
    return instance.query('What is retained?');
  });
  assert.equal(result.beforeSha256, snapshot); assert.equal(result.afterSha256, snapshot);
  assert.equal(result.value.results.length, 1);
  await assert.rejects(stale('late question'), /invalid scoped/);
  assert.deepEqual(f.writes[0].settings, { flag: 'wx' });
  assert.equal(f.writes[0].name, `${f.directory}\\${uuid}.toml`);
  assert(f.writes[0].bytes.includes('CLONE_SECRET'));
  for (const call of f.helperCalls) assert.deepEqual(call.env, { SystemRoot: 'C:\\Windows', WINDIR: 'C:\\Windows', PATH: '' });
  for (const { settings } of f.children) {
    assert.equal(settings.cwd, f.directory); assert.equal(settings.shell, false); assert.equal(settings.windowsHide, true);
    assert.equal(settings.env.PATH, ''); assert.equal(settings.env.SystemRoot, 'C:\\Windows');
    assert.equal(settings.env.OPENAI_API_KEY, undefined); assert.equal(settings.env.ANTHROPIC_API_KEY, undefined);
    assert.equal(settings.env.TEMP, f.directory);
  }
  assert.equal(f.children[0].settings.env.CODIFIX_TEST_DATABASE_URL, f.options.databaseUrl);
  assert.equal(f.children[1].settings.env.CODIFIX_TEST_DATABASE_URL, undefined);
  assert(f.events.indexOf('kill-service') < f.events.indexOf('release'));
  assert(f.events.indexOf('release') < f.events.indexOf('remove-config'));
  await session.close();
  assert.equal(f.events.at(-1), 'remove-directory');
  await assert.rejects(session.withInstance('r3-alpha', async () => {}), /unavailable/);
});

test('missing outer Job, changed pins and invalid ACL receipts write no payload or credentials', async () => {
  for (const settings of [
    { helperFailure: '--verify-outer-job' }, { helperFailure: '--prepare-io' },
    { corruptPin: 'C:\\private\\tools\\helper.exe' }, { corruptPin: 'C:\\private\\tools\\snapshot.exe' },
    { corruptPin: 'C:\\private\\tools\\memory.exe' },
    { jobChange: receipt => { receipt.caller_pid++; } }, { jobChange: receipt => { receipt.helper_pid = receipt.caller_pid; } },
    { jobChange: receipt => { receipt.kill_on_close = false; } }, { jobChange: receipt => { receipt.breakaway_allowed = true; } },
    { ioChange: receipt => { receipt.directory = 'C:\\public'; } },
    { ioChange: receipt => { receipt.request_id = 'wrong'; } }, { ioChange: receipt => { receipt.root_identity = []; } },
  ]) {
    const f = fixture(settings);
    await assert.rejects(f.opener(f.options));
    assert.equal(f.writes.length, 0); assert.equal(f.children.length, 0); assert.equal(f.removals.length, 0);
  }
  const noProfile = fixture(); delete noProfile.options.execution;
  await assert.rejects(noProfile.opener(noProfile.options), /explicit native/);
  const crossHost = fixture({ platform: 'linux' });
  await assert.rejects(crossHost.opener(crossHost.options), /requires Windows/);
  for (const f of [noProfile, crossHost]) assert.equal(f.events.length, 0);
  for (const change of [
    options => { options.execution.profile_sha256 = hash('changed'); },
    options => { options.memoryBinary = 'C:\\unreviewed\\memory.exe'; },
    options => { options.snapshotBinary = '/linux/snapshot'; },
  ]) {
    const f = fixture(); change(f.options);
    await assert.rejects(f.opener(f.options));
    assert.equal(f.events.length, 0);
  }
});

test('native failures stop exact owned children, release clones, and retain uncertain private staging', async () => {
  for (const settings of [{ serviceNotReady: true }, { configFailure: true }]) {
    const f = fixture(settings), session = await f.opener(f.options);
    await assert.rejects(session.withInstance('r3-alpha', async () => {}));
    assert(f.requests.some(request => request.operation === 'release'));
    if (!settings.configFailure) assert(f.events.includes('kill-service'));
    await session.close();
  }
  const f = fixture({ shutdownFailure: true }), session = await f.opener(f.options);
  await assert.rejects(session.close(), /private staging retained/);
  assert(!f.events.includes('remove-directory'));
  const failedPrepare = fixture({ prepareFailure: true });
  await assert.rejects(failedPrepare.opener(failedPrepare.options), /preparation failed/);
  assert(failedPrepare.events.includes('shutdown'));
});

test('POSIX session retains private mode, explicit environment and graceful owned-child stop', async () => {
  const f = fixture({ platform: 'linux' });
  delete f.options.execution; f.options.snapshotBinary = '/reviewed/snapshot'; f.options.memoryBinary = '/reviewed/memory';
  const session = await f.opener(f.options);
  await session.withInstance('r3-alpha', async instance => instance.query('History?'));
  assert.equal(f.helperCalls.length, 0);
  assert.deepEqual(f.writes[0].settings, { flag: 'wx', mode: 0o600 });
  assert.deepEqual(f.children[1].settings.env, { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: '/private/session', CODIFIX_PROFILE: 'prod' });
  assert.equal(f.children[1].child.signalCode, 'SIGINT');
  await session.close();
});

const nativeExecutionFile = process.env.CODIFIX_R3_NATIVE_EXECUTION;
const live = process.platform === 'win32' && nativeExecutionFile && process.env.CODIFIX_R3_SNAPSHOT_BIN
  && process.env.CODIFIX_R3_CLI_BIN && process.env.CODIFIX_TEST_DATABASE_URL;
if (process.env.CODIFIX_R3_REQUIRE_NATIVE_SERVICE === '1' && !live) throw Error('required native prepared-service configuration missing');
test('real native prepared session resets scoped reads under the outer trusted Job', { skip: !live, timeout: 120000 }, async () => {
  const execution = JSON.parse(await fs.readFile(nativeExecutionFile, 'utf8'));
  const session = await openPreparedSession({ execution, snapshotBinary: process.env.CODIFIX_R3_SNAPSHOT_BIN,
    memoryBinary: process.env.CODIFIX_R3_CLI_BIN, databaseUrl: process.env.CODIFIX_TEST_DATABASE_URL,
    semanticTime: '2026-10-17T00:00:00Z',
    documents: [{ project: 'r3-native', source: 'public/source.md', text: 'Retain acknowledgement history before retry.' }] });
  try {
    assert.equal(session.preparation.official_evidence, false);
    for (let repeat = 0; repeat < 2; repeat++) {
      const result = await session.withInstance('r3-native', async instance => instance.query('acknowledgement'));
      assert(result.value.results.length > 0);
      assert.equal(result.beforeSha256, result.afterSha256);
    }
  } finally { await session.close(); }
});
