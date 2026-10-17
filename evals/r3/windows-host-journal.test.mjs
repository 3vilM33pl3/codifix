// SPDX-License-Identifier: AGPL-3.0-or-later
// Fake helper/process adapters verify contracts, not native OS evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { createHash } from 'node:crypto';
import { createHostJournalHelperInvoker, createWindowsHostJournal } from './windows-host-journal.mjs';
import { nativeProfileHash } from './isolation/native-windows.mjs';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const directory = 'C:\\private\\dispatch';

function fixture({ receiptChange, helperFailure, statChange } = {}) {
  const helperBytes = Buffer.from('reviewed helper');
  const helperPath = 'C:\\private\\tools\\helper.exe';
  const node = { path: 'C:\\private\\tools\\node.exe', sha256: hash('node') },
    guest = { path: 'C:\\private\\tools\\guest.mjs', sha256: hash('guest') };
  const profile = { schema_version: 1, helper: { path: helperPath, sha256: hash(helperBytes) },
    node, guest, io_root: 'C:\\private\\runs', toolchains: [{ root: 'C:\\private\\tools', files: [node, guest] }],
    search_path: ['C:\\private\\tools'], library_path: [] };
  const execution = { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) };
  const calls = [], files = new Map();
  let changed = false, identity = ['a', '1'];
  const dependencies = { platform: 'win32', callerPid: 123, systemRoot: 'C:\\Windows',
    fileSystem: {
      stat: async name => {
        assert.equal(name, helperPath);
        const stat = { isFile: () => true, nlink: 1, size: helperBytes.length };
        statChange?.(stat); return stat;
      },
      readFile: async name => { assert.equal(name, helperPath); return changed ? Buffer.from('changed') : helperBytes; },
    },
    invokeHelper: async (binary, args, input, env) => {
      calls.push({ binary, args, input: Buffer.from(input), env });
      if (helperFailure) throw Error('PRIVATE_SECRET helper path and diagnostic');
      assert.equal(binary, helperPath);
      assert.equal(args[0], '--host-journal'); assert.deepEqual(args.slice(4), ['--caller-pid', '123']);
      assert.deepEqual(env, { SystemRoot: 'C:\\Windows', WINDIR: 'C:\\Windows', PATH: '' });
      const [, operation, parent, name] = args;
      const key = path.win32.join(parent, name).toLowerCase();
      const receipt = { format: 'memory-windows-host-journal-v1', operation, name, directory_identity: identity.slice() };
      if (operation === 'write') {
        assert(!files.has(key), 'publication must be exclusive');
        files.set(key, Buffer.from(input)); receipt.sha256 = hash(input);
      } else if (operation === 'read') {
        assert.equal(input.length, 0);
        receipt.sha256 = hash(files.get(key)); receipt.value = JSON.parse(files.get(key));
      } else assert.equal(input.length, 0);
      receiptChange?.(receipt, operation);
      return JSON.stringify(receipt);
    },
  };
  return { execution, dependencies, calls, files, changePin: () => { changed = true; },
    changeIdentity: () => { identity = ['a', '2']; } };
}

test('journal pins helper, validates native receipts, and snapshots caller-owned values before awaits', async () => {
  const f = fixture(), pending = createWindowsHostJournal(f.execution, f.dependencies);
  f.execution.profile.helper.path = 'C:\\wrong\\helper.exe';
  const journal = await pending;
  await journal.create(directory);
  const value = { private: 'PRIVATE_SECRET', version: 1 };
  const write = journal.write(directory, 'identity.json', value); value.private = 'caller mutation'; await write;
  assert.deepEqual(await journal.read('c:/private/dispatch', 'identity.json'), { private: 'PRIVATE_SECRET', version: 1 });
  assert.deepEqual(f.calls.map(call => call.args[1]), ['create', 'write', 'read']);
  assert.equal(f.calls[0].args[2], 'C:\\private'); assert.equal(f.calls[0].args[3], 'dispatch');
  assert.equal(f.calls[2].args[2], 'c:\\private\\dispatch');
});

test('changed helper bytes, aliases and filesystem failures stop before helper invocation', async () => {
  const f = fixture(), journal = await createWindowsHostJournal(f.execution, f.dependencies);
  f.changePin();
  await assert.rejects(journal.create(directory), /helper changed/);
  assert.equal(f.calls.length, 0);
  for (const statChange of [stat => { stat.nlink = 2; }, stat => { stat.size = 0; },
    stat => { stat.size = 513 * 1024 * 1024; }, stat => { stat.isFile = () => false; },
    () => { throw Error('PRIVATE_SECRET path unavailable'); }]) {
    const f = fixture({ statChange });
    await assert.rejects(createWindowsHostJournal(f.execution, f.dependencies), error => !error.message.includes('PRIVATE_SECRET'));
    assert.equal(f.calls.length, 0);
  }
});

test('fresh protected reads bind original bytes despite Rust JSON object-key reordering', async () => {
  const raw = Buffer.from('{"z":{"b":1,"a":2},"a":3}');
  const f = fixture({ receiptChange: (receipt, operation) => {
    if (operation === 'read') receipt.value = { a: 3, z: { a: 2, b: 1 } };
  } });
  f.files.set(path.win32.join(directory, 'runtime.json').toLowerCase(), raw);
  const journal = await createWindowsHostJournal(f.execution, f.dependencies);
  const value = await journal.read(directory, 'runtime.json', { sha256: hash(raw) });
  assert.deepEqual(value, JSON.parse(raw));
  assert.notEqual(JSON.stringify(value), raw.toString());
  await assert.rejects(journal.read(directory, 'runtime.json', { sha256: 'a'.repeat(64) }), /expected digest/);
  await assert.rejects(journal.read(directory, 'runtime.json', { sha256: 'bad' }), /explicit.*digest/);
});

test('unknown and mixed profiles, unsafe directories and components never reach native helper', async () => {
  for (const change of [
    f => { f.dependencies.platform = 'linux'; }, f => { f.dependencies.systemRoot = 'C:\\public'; },
    f => { f.dependencies.callerPid = 0; }, f => { f.execution.profile_sha256 = hash('changed'); },
    f => { f.execution.kind = 'linux-container-v1'; }, f => { f.execution.extra = true; },
  ]) {
    const f = fixture(); change(f); await assert.rejects(createWindowsHostJournal(f.execution, f.dependencies));
    assert.equal(f.calls.length, 0);
  }
  const f = fixture(), journal = await createWindowsHostJournal(f.execution, f.dependencies);
  for (const unsafe of ['C:\\', '\\private', '\\\\server\\private', '/private', 'C:private',
    'C:\\private\\..\\outside', 'C:\\private.\\dispatch', 'C:\\private\\dispatch:stream',
    'C:\\private\\dispatch\\', 'C:\\private\\CON', 'C:\\private\\bad\0name']) {
    await assert.rejects(journal.create(unsafe)); await assert.rejects(journal.read(unsafe, 'identity.json'));
  }
  for (const name of ['../outside', 'a..json', 'a:stream', 'C:\\secret', 'UPPER.json', '.hidden', 'tail.', 'con', 'nul.json', '', 'a'.repeat(129)]) {
    await assert.rejects(journal.write(directory, name, {})); await assert.rejects(journal.read(directory, name));
  }
  assert.equal(f.calls.length, 0);
});

test('private JSON is bounded and serialization/helper errors are opaque', async () => {
  const f = fixture(), journal = await createWindowsHostJournal(f.execution, f.dependencies);
  const cyclic = {}; cyclic.self = cyclic;
  for (const value of [undefined, 1n, cyclic, 'x'.repeat(16 * 1024 * 1024),
    { toJSON() { throw Error('PRIVATE_SECRET in serializer'); } }]) {
    await assert.rejects(journal.write(directory, 'identity.json', value), error => !error.message.includes('PRIVATE_SECRET'));
  }
  assert.equal(f.calls.length, 0);
  const failed = fixture({ helperFailure: true }), blocked = await createWindowsHostJournal(failed.execution, failed.dependencies);
  await assert.rejects(blocked.write(directory, 'identity.json', { secret: 'PRIVATE_SECRET' }),
    error => error.message === 'protected native journal operation failed; retain private evidence');
});

test('changed directory identity and immutable file digest fail across case and separator aliases', async () => {
  const f = fixture(), journal = await createWindowsHostJournal(f.execution, f.dependencies);
  await journal.create(directory); await journal.write(directory, 'identity.json', { version: 1 });
  f.changeIdentity();
  await assert.rejects(journal.read('c:/private/dispatch', 'identity.json'), /directory identity changed/);
  const content = fixture(), bound = await createWindowsHostJournal(content.execution, content.dependencies);
  await bound.write(directory, 'identity.json', { version: 1 });
  content.files.set((directory + '\\identity.json').toLowerCase(), Buffer.from('{"version":2}'));
  await assert.rejects(bound.read(directory, 'identity.json'), /immutable content changed/);
});

test('malformed native receipts, mismatched operation/name/hash and excessive reads fail closed', async () => {
  for (const receiptChange of [
    receipt => { receipt.extra = 'PRIVATE_SECRET'; }, receipt => { receipt.format = 'wrong'; },
    receipt => { receipt.operation = 'read'; }, receipt => { receipt.name = 'other.json'; },
    receipt => { receipt.directory_identity = ['not-hex', '1']; }, receipt => { receipt.sha256 = hash('changed'); },
  ]) {
    const f = fixture({ receiptChange }), journal = await createWindowsHostJournal(f.execution, f.dependencies);
    await assert.rejects(journal.write(directory, 'identity.json', {}), error => !error.message.includes('PRIVATE_SECRET'));
  }
  const f = fixture({ receiptChange: (receipt, operation) => { if (operation === 'read') receipt.value = 'x'.repeat(16 * 1024 * 1024); } });
  const journal = await createWindowsHostJournal(f.execution, f.dependencies);
  await journal.write(directory, 'identity.json', {});
  await assert.rejects(journal.read(directory, 'identity.json'), /bounded native journal payload/);
});

function processFixture({ failure, output = '{}', holdClose = false } = {}) {
  const child = new EventEmitter(); child.stdin = new PassThrough(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
  let started; const ready = new Promise(resolve => { started = resolve; });
  const state = { killed: 0, closed: 0 };
  const close = (code, signal = null) => { state.closed++; child.emit('close', code, signal); };
  child.kill = signal => { assert.equal(signal, 'SIGKILL'); state.killed++; if (!holdClose) queueMicrotask(() => close(null, signal)); return true; };
  child.stdin.on('finish', () => { started(); queueMicrotask(() => {
    if (failure === 'hang') return;
    if (failure === 'pipe') { child.stdin.emit('error', Error('PRIVATE_SECRET pipe')); return; }
    if (failure === 'spawn') { child.emit('error', Error('PRIVATE_SECRET spawn')); return; }
    if (failure === 'overflow') {
      child.stdout.write(Buffer.alloc(20 * 1024 * 1024)); child.stderr.write(Buffer.alloc(20 * 1024 * 1024 + 1)); return;
    }
    child.stdout.write(output); child.stderr.write('PRIVATE_SECRET ignored stderr'); close(0);
  }); });
  const invoke = createHostJournalHelperInvoker({ spawnProcess: (_binary, _args, settings) => {
    assert.equal(settings.shell, false); assert.equal(settings.windowsHide, true);
    assert.deepEqual(settings.env, { PATH: '' }); return child;
  } });
  return { ready, state, close, call: () => invoke('C:\\private\\helper.exe', [], Buffer.from('PRIVATE_SECRET input'), { PATH: '' }) };
}

test('helper subprocess bounds combined diagnostics, rejects invalid UTF-8 and reaps on errors', async () => {
  assert.equal(await processFixture().call(), '{}');
  for (const failure of ['pipe', 'spawn', 'overflow']) {
    const f = processFixture({ failure });
    await assert.rejects(f.call(), error => error.message === 'protected host journal helper failed');
    assert.equal(f.state.killed, 1); assert.equal(f.state.closed, 1);
  }
  await assert.rejects(processFixture({ output: Buffer.from([0xff]) }).call(), /output invalid/);
  const pending = processFixture({ failure: 'pipe', holdClose: true });
  let settled = false; const result = pending.call().finally(() => { settled = true; });
  const rejection = assert.rejects(result, /helper failed/);
  await pending.ready; await Promise.resolve(); assert.equal(settled, false);
  pending.close(1); await rejection;
});

test('helper timeout kills only its owned process and waits for close', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = processFixture({ failure: 'hang' }), result = f.call(), rejection = assert.rejects(result, /helper failed/);
  await f.ready; t.mock.timers.tick(30000); await rejection;
  assert.equal(f.state.killed, 1); assert.equal(f.state.closed, 1);
});
