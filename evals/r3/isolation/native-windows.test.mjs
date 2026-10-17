// SPDX-License-Identifier: AGPL-3.0-or-later
// Fake helpers validate protocol/dispatch only, never Windows OS isolation.
import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createNativeWindowsExecutor, nativeCommandExitCode, nativeProfileHash, validateNativeInput } from './native-windows.mjs';
import { createExecutionRunner, validateExecutionInput } from './execution.mjs';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const id = '12345678-1234-4234-8234-123456789abc';

function fixture({ prepare, receipt: mutateReceipt, artifact: mutateArtifact, commandExitCode = 0 } = {}) {
  const storage = new Map();
  const code = (name, text) => { storage.set(name, Buffer.from(text)); return { path: name, sha256: sha(text) }; };
  const helper = code('C:\\private\\helper.exe', 'fake trusted helper');
  const node = code('C:\\private\\tool\\node.exe', 'fake native node');
  const guest = code('C:\\private\\tool\\guest.mjs', 'fake pinned guest');
  const profile = { schema_version: 1, helper, io_root: 'C:\\private\\runs', node, guest,
    toolchains: [{ root: 'C:\\private\\tool', files: [node, guest] }],
    search_path: ['C:\\private\\tool'], library_path: [] };
  const options = { execution: { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) },
    argv: [node.path, '-e', 'explicit native command'], files: { 'src/main.rs': 'selected source' },
    exportPaths: ['src/main.rs'], timeoutMs: 1000 };
  const calls = [], writes = [];
  let prepared = false;
  const fileSystem = {
    async stat(location) { const bytes = storage.get(location); if (!bytes) throw Error('missing fake artifact'); return { isFile: () => true, size: bytes.length }; },
    async readFile(location) { if (!storage.has(location)) throw Error('missing fake artifact'); return Buffer.from(storage.get(location)); },
    async writeFile(location, bytes, flags) {
      assert(prepared, 'plaintext cannot precede native private-root confirmation');
      assert.equal(flags.flag, 'wx'); assert.equal(flags.mode, undefined, 'mode is not Windows ACL evidence');
      assert(!storage.has(location)); storage.set(location, Buffer.from(bytes)); writes.push(location);
    },
  };
  const invoke = async (executable, args, timeout) => {
    calls.push({ executable, args, timeout });
    assert.equal(executable, helper.path);
    if (args[0] === '--prepare-io') {
      const value = { format: 'memory-windows-evaluation-io-v1', request_id: id,
        directory: path.win32.join(profile.io_root, id), root_identity: ['1','2'], directory_identity: ['1','3'] };
      prepare?.(value); prepared = true;
      return { stdout: JSON.stringify(value) };
    }
    assert.equal(args[0], '--descriptor');
    const descriptorBytes = storage.get(args[1]); assert.equal(sha(descriptorBytes), args[3]);
    const descriptor = JSON.parse(descriptorBytes);
    assert.equal(descriptor.allow_missing_exports, true);
    assert.deepEqual(descriptor.arguments,
      ['--preserve-symlinks','--preserve-symlinks-main',profile.guest.path]);
    const stdin = storage.get(descriptor.stdin_file.path);
    assert.equal(sha(stdin), descriptor.stdin_file.sha256);
    const request = JSON.parse(stdin);
    assert.equal(request.files['src/main.rs'], 'selected source');
    assert.deepEqual(Object.keys(request).sort(), ['argv','files','timeoutMs','version']);
    const output = Buffer.from(JSON.stringify({ version: 1, exitCode: commandExitCode, stdout: 'native synthetic result\n', stderr: '' }));
    const artifacts = [];
    for (const [source, file, bytes] of [['stdout','stdout.bin',output], ['stderr','stderr.bin',Buffer.alloc(0)],
      ['src/main.rs','export-000.bin',Buffer.from('edited source')]]) {
      const value = { source, file, sha256: sha(bytes), bytes: bytes.length };
      storage.set(path.win32.join(args[5], file), bytes); artifacts.push(value);
    }
    mutateArtifact?.(storage, args[5], artifacts);
    const receipt = { format: 'memory-windows-evaluation-result-v1', state: 'succeeded',
      descriptor_sha256: sha(descriptorBytes), stdin_sha256: sha(stdin), descriptor_verified: true,
      exit_code: 0, disk_hard_limit_enforced: false, profile: 'windows-appcontainer-evaluation-v1',
      architecture: 'x86_64', artifacts };
    mutateReceipt?.(receipt);
    return { stdout: JSON.stringify(receipt) };
  };
  return { options, calls, writes, storage,
    run: createNativeWindowsExecutor({ fileSystem, invoke, platform: 'win32', requestId: () => id }) };
}

test('explicit backend dispatch preserves Linux defaults and never infers native Windows', async () => {
  const calls = [];
  const run = createExecutionRunner({ linux: async input => { calls.push(input); return 'linux'; }, windows: async () => 'windows' });
  const linux = { image: `sha256:${'a'.repeat(64)}`, argv: ['/usr/bin/node'], files: {} };
  assert.equal(await run(linux), 'linux');
  assert.deepEqual(calls[0], linux);
  const native = fixture().options;
  assert.equal(await run(native), 'windows');
  assert.throws(() => validateExecutionInput({ ...linux, execution: { kind: 'auto' } }));
  assert.throws(() => validateExecutionInput({ ...native, image: linux.image }));
});

test('fake native helper protocol binds private preparation, source inputs and result bytes', async () => {
  const value = fixture();
  const pending = value.run(value.options);
  value.options.files['src/main.rs'] = 'mutated caller';
  const result = await pending;
  assert.equal(result.exitCode, 0); assert.equal(result.stdout, 'native synthetic result\n');
  assert.equal(Buffer.from(result.files['src/main.rs'], 'base64').toString(), 'edited source');
  assert.equal(result.image, undefined);
  assert.equal(result.execution.kind, 'windows-native-v1');
  assert.equal(result.execution.disk_hard_limit_enforced, false);
  assert.equal(value.calls.length, 2); assert.equal(value.writes.length, 2);
});

test('native validation rejects POSIX translation, changed profile, unbound executables and unsafe exports', () => {
  const { options } = fixture();
  assert.doesNotThrow(() => validateNativeInput(options));
  for (const change of [{ argv: ['/bin/sh','-c','no'] }, { argv: ['C:\\Windows\\cmd.exe'] },
    { files: { '../hidden': 'no' } }, { exportPaths: ['NUL.rs'] }, { exportPaths: ['src','src/main.rs'] },
    { files: { 'A.rs':'one', 'a.rs':'two' } }, { exportPaths: ['src/main.rs','SRC/MAIN.RS'] }, { timeoutMs: 0 }]) assert.throws(() => validateNativeInput({ ...options, ...change }));
  options.execution.profile.io_root = 'C:\\changed';
  assert.throws(() => validateNativeInput(options), /digest/);
});

test('failed native ACL preparation never writes plaintext', async () => {
  const value = fixture({ prepare: receipt => { receipt.directory = 'C:\\wrong'; } });
  await assert.rejects(value.run(value.options), /I\/O receipt/);
  assert.equal(value.writes.length, 0);
});

test('native receipt, artifact hashes and source-to-file bindings fail closed', async () => {
  for (const change of [
    { receipt: value => { value.descriptor_sha256 = '0'.repeat(64); } },
    { receipt: value => { value.disk_hard_limit_enforced = true; } },
    { artifact: (files, root) => files.set(path.win32.join(root, 'export-000.bin'), Buffer.from('changed')) },
    { artifact: (_files, _root, items) => { items[2].file = 'stdout.bin'; } },
  ]) { const value = fixture(change); await assert.rejects(value.run(value.options), /native/); }
});

test('changed helper bytes fail before preparing I/O or writing payloads', async () => {
  const value = fixture();
  value.storage.set(value.options.execution.profile.helper.path, Buffer.from('different helper'));
  await assert.rejects(value.run(value.options), /bytes changed/);
  assert.equal(value.calls.length, 0); assert.equal(value.writes.length, 0);
});

test('deleted selected sources remain absent; oversized artifact sets cannot pass', async () => {
  const deleted = fixture({ receipt: value => { value.artifacts = value.artifacts.slice(0, 2); } });
  assert.deepEqual(Object.keys((await deleted.run(deleted.options)).files), []);
  const excessive = fixture({ artifact: (files, root, items) => {
    const bytes = Buffer.alloc(4 * 1024 * 1024 + 1);
    items[2].bytes = bytes.length; items[2].sha256 = sha(bytes);
    files.set(path.win32.join(root, items[2].file), bytes);
  } });
  await assert.rejects(excessive.run(excessive.options), /byte limit/);
});

test('non-Windows host never falls back to Docker or claims native evidence', async () => {
  const value = fixture();
  const run = createNativeWindowsExecutor({ platform: 'linux', invoke: async () => { throw Error('must not run'); } });
  await assert.rejects(run(value.options), /explicit Windows host/);
});

test('loader NTSTATUS exits invalidate fake native results in signed and unsigned form', async () => {
  for (const status of [0xc0000135, 0xc0000142]) {
    for (const code of [status, status | 0]) {
      assert.throws(() => nativeCommandExitCode(code), /loader failure/);
      const value = fixture({ commandExitCode: code });
      await assert.rejects(value.run(value.options), /loader failure invalidates execution/);
    }
  }
  // Compiler/test failures and submitted-program crashes remain ordinary
  // negative outcomes; only known infrastructure loader failures are excluded.
  for (const code of [1, 65, 66, 101, 0xc0000005, 0xc0000005 | 0]) {
    const value = fixture({ commandExitCode: code });
    assert.equal((await value.run(value.options)).exitCode, code >>> 0);
  }
  for (const code of [null, -0x80000001, 0x100000000, 1.5]) assert.throws(() => nativeCommandExitCode(code));
});
