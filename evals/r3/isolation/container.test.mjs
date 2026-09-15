// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { runOfflineContainer, validateInput, validateResult } from './container.mjs';

const image = process.env.MEMORY_R3_TEST_IMAGE;
if (process.env.MEMORY_R3_REQUIRE_CONTAINER === '1' && !image) throw Error('required container test image missing');
const valid = { image: `sha256:${'a'.repeat(64)}`, argv: ['/usr/local/bin/node', '-e', ''], files: { 'src/lib.rs': 'fixture' } };
test('unsafe or excessive input fails before Docker dispatch', () => {
  assert.doesNotThrow(() => validateInput(valid));
  for (const change of [
    { image: 'node:latest' }, { image: '--privileged' }, { argv: ['node'] },
    { argv: ['/bin/sh', '\0'] }, { timeoutMs: 0 }, { timeoutMs: 60001 },
    { files: { '../private/answers': 'no' } }, { files: { '/etc/config': 'no' } },
    { files: { 'a': 'file', 'a/b': 'conflict' } }, { files: { 'huge': 'x'.repeat(1048577) } },
    { exportPaths: ['../private'] }, { exportPaths: ['src', 'src/lib.rs'] },
    { exportPaths: ['a', 'a'] }, { exportPaths: [null] },
    { profile: 'unlimited' },
  ]) assert.throws(() => validateInput({ ...valid, ...change }));
});

test('explicit source exports exclude build products and freeze the invocation', { skip: !image, timeout: 30000 }, async () => {
  const options = { image, files: { 'src/lib.rs': 'reviewed' }, exportPaths: ['src/lib.rs'],
    argv: ['/usr/local/bin/node', '-e', `
      const fs=require('fs');
      fs.mkdirSync('target');fs.writeFileSync('target/build.bin',Buffer.alloc(1500000));
      fs.symlinkSync('/etc/passwd','target/link');
      require('assert').equal(fs.readFileSync('src/lib.rs','utf8'),'reviewed');
    `] };
  const pending = runOfflineContainer(options);
  options.files['src/lib.rs'] = 'changed after dispatch';
  options.exportPaths.push('target/build.bin');
  options.argv[2] = 'process.exit(42)';
  const result = await pending;
  assert.equal(result.exitCode, 0);
  assert.deepEqual(result.files, { 'src/lib.rs': Buffer.from('reviewed').toString('base64') });
  for (const code of [
    "fs.rmSync('src',{recursive:true});fs.symlinkSync('/etc','src')",
    "fs.rmSync('src/lib.rs');fs.mkdirSync('src/lib.rs')",
    "fs.linkSync('src/lib.rs','alias')",
  ]) await assert.rejects(runOfflineContainer({ image, files: { 'src/lib.rs': 'reviewed' },
    exportPaths: ['src/lib.rs'], argv: ['/usr/local/bin/node', '-e', "const fs=require('fs');" + code] }), /offline execution failed/);
  const empty = await runOfflineContainer({ image, files: {}, exportPaths: [],
    argv: ['/usr/local/bin/node', '-e', "require('fs').writeFileSync('private-feedback','not exported')"] });
  assert.deepEqual(empty.files, {});
});

test('guest output remains untrusted and bounded before submission import', () => {
  const result = { version: 1, exitCode: 0, stdout: '', stderr: '', files: { 'src/lib.rs': 'b2s=' } };
  assert.doesNotThrow(() => validateResult(result));
  for (const change of [
    { exitCode: null }, { exitCode: -1 }, { stdout: 7 },
    { isolation: { verified: true } },
    { files: { '../private': 'b2s=' } }, { files: { file: 'not-base64!' } },
    { files: { file: Buffer.alloc(1048577).toString('base64') } },
  ]) assert.throws(() => validateResult({ ...result, ...change }));
});

test('offline container: boundary, reset, submission transfer, timeout and symlink rejection', { skip: !image, timeout: 90000 }, async t => {
  const hidden = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-hidden-grader-'));
  await fs.writeFile(path.join(hidden, 'answers.json'), 'synthetic hidden scoring answers');
  t.after(() => fs.rm(hidden, { recursive: true }));
  const options = { image, files: { 'selected.txt': 'approved synthetic context' }, timeoutMs: 4000 };
  const result = await runOfflineContainer({ ...options, argv: ['/usr/local/bin/node', '-e', `
    const fs=require('fs'),assert=require('assert'),net=require('net');
    assert.equal(process.getuid(),${process.getuid()});
    assert.deepEqual(Object.keys(process.env).sort(),['HOME','LANG','PATH']);
    for(const p of ['/private','/host','/var/run/docker.sock','/home/olivier/.codex','/other-condition']) assert(!fs.existsSync(p),p);
    assert(!fs.existsSync(${JSON.stringify(hidden)}),'hidden host grader was mounted');
    assert.throws(()=>fs.writeFileSync('/input/files/selected.txt','tampered'));
    assert.throws(()=>fs.writeFileSync('/etc/r3-test','tampered'));
    assert.equal(fs.readFileSync('selected.txt','utf8'),'approved synthetic context');
    assert.equal(fs.readdirSync('/sys/class/net').join(','),'lo');
    const socket=net.connect({host:'192.0.2.1',port:443});
    socket.on('connect',()=>{throw Error('network escape')});
    socket.on('error',()=>{fs.writeFileSync('submission.txt','step-one');console.log('isolated')});
  `] });
  assert.equal(result.exitCode, 0);
  assert.equal(result.stdout.trim(), 'isolated');
  assert.equal(Buffer.from(result.files['submission.txt'], 'base64').toString(), 'step-one');
  const next = await runOfflineContainer({ ...options, files: { 'submission.txt': 'step-one' }, argv: ['/usr/local/bin/node', '-e', `
    const fs=require('fs'),a=require('assert');a(!fs.existsSync('selected.txt'));
    a.equal(fs.readFileSync('submission.txt','utf8'),'step-one');fs.writeFileSync('submission.txt','step-two');
  `] });
  assert.equal(Buffer.from(next.files['submission.txt'], 'base64').toString(), 'step-two');
  const fresh = await runOfflineContainer({ ...options, argv: ['/usr/local/bin/node', '-e', "require('assert')(!require('fs').existsSync('submission.txt'))"] });
  assert.equal(fresh.exitCode, 0);
  await assert.rejects(runOfflineContainer({ ...options, argv: ['/usr/local/bin/node', '-e', "require('fs').symlinkSync('/etc/passwd','escape')"] }), /offline execution failed/);
  await assert.rejects(runOfflineContainer({ ...options, timeoutMs: 1000, argv: ['/usr/local/bin/node', '-e', 'setInterval(()=>{},1000)'] }), /offline execution failed/);
  await assert.rejects(runOfflineContainer({ ...options, argv: ['/usr/local/bin/node', '-e', "process.stdout.write('x'.repeat(1048577))"] }), /offline execution failed/);
});
