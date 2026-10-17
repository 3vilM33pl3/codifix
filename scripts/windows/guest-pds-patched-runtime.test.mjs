// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { checkedInventory, copyVerifiedRuntime, preparePatchedRuntime, patchSha256, fileDigest } from './guest-pds-patched-runtime.mjs';
import { patchPdsFile, originalFiles } from '../../packaging/windows-pds/windows-paths.mjs';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
async function fixture(fn) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-pds-stage-'));
  try { await fn(root); } finally { await fs.rm(root, { recursive: true, force: true }); }
}
async function put(root, relative, bytes) {
  await fs.mkdir(path.dirname(path.join(root, relative)), { recursive: true });
  await fs.writeFile(path.join(root, relative), bytes);
  return { path: relative, sha256: sha(bytes), bytes: Buffer.byteLength(bytes) };
}
test('inventory rejects aliases, traversal, malformed digests and unbounded bytes', () => {
  const item = { path: 'node_modules/public/LICENSE', sha256: 'a'.repeat(64), bytes: 12 };
  assert.equal(checkedInventory([item]).length, 1);
  for (const changed of [{ path: '../outside' }, { path: 'C:/secret' }, { path: 'a\\b' }, { sha256: 'bad' }, { bytes: -1 }, { bytes: 2 ** 30 }, { extra: true }]) {
    assert.throws(() => checkedInventory([{ ...item, ...changed }]));
  }
  assert.throws(() => checkedInventory([item, { ...item, path: item.path.toUpperCase() }]));
});
test('fresh copy preserves exact bytes and refuses changed or extra inputs', async () => fixture(async root => {
  const source = path.join(root, 'original');
  const files = [await put(source, 'package.json', '{"version":"synthetic"}'), await put(source, 'node_modules/public/test.key', 'public fixture')];
  const copied = await copyVerifiedRuntime(source, path.join(root, 'fresh'), files);
  assert.deepEqual(copied, files);
  assert.equal(await fs.readFile(path.join(root, 'fresh/node_modules/public/test.key'), 'utf8'), 'public fixture');
  await assert.rejects(copyVerifiedRuntime(source, path.join(root, 'fresh'), files), /EEXIST/);
  await fs.writeFile(path.join(source, 'package.json'), 'changed');
  await assert.rejects(copyVerifiedRuntime(source, path.join(root, 'changed'), files), /preparation receipt/);
  assert.equal(await fs.readFile(path.join(source, 'package.json'), 'utf8'), 'changed');
  await put(source, 'extra.json', 'unrecorded');
  await assert.rejects(copyVerifiedRuntime(source, path.join(root, 'extra'), files), /complete prepared inventory/);
  await assert.rejects(fs.stat(path.join(root, 'extra')), /ENOENT/);
}));
test('baseline links are rejected without copying their target', async () => fixture(async root => {
  const source = path.join(root, 'original');
  const item = await put(source, 'package.json', 'original');
  await fs.symlink(source, path.join(source, 'alias'), process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(copyVerifiedRuntime(source, path.join(root, 'copy'), [item]), /link/);
}));
test('pinned package hardlinks are flattened without relaxing output or receipt checks', async () => fixture(async root => {
  const source = path.join(root, 'baseline');
  const first = await put(source, 'node_modules/a/LICENSE', 'pinned package notice');
  const alias = 'node_modules/b/LICENSE';
  await fs.mkdir(path.dirname(path.join(source, alias)), { recursive: true });
  await fs.link(path.join(source, first.path), path.join(source, alias));
  const files = [first, { ...first, path: alias }];
  await assert.rejects(fileDigest(path.join(source, first.path)), /unaliased/);
  const destination = path.join(root, 'flattened');
  assert.deepEqual(await copyVerifiedRuntime(source, destination, files), files);
  for (const file of files) {
    assert.equal((await fs.stat(path.join(source, file.path))).nlink, 2);
    assert.equal((await fs.stat(path.join(destination, file.path))).nlink, 1);
    assert.equal((await fileDigest(path.join(destination, file.path))).sha256, file.sha256);
    assert.equal(sha(await fs.readFile(path.join(source, file.path))), file.sha256);
  }
  await fs.writeFile(path.join(source, alias), 'changed through alias');
  await assert.rejects(copyVerifiedRuntime(source, path.join(root, 'changed'), files), /preparation receipt/);
  assert.equal((await fileDigest(path.join(destination, first.path))).sha256, first.sha256);
}));
test('reviewed patch script pin matches the source actually imported', async () => {
  assert.equal(sha(await fs.readFile(new URL('../../packaging/windows-pds/windows-paths.mjs', import.meta.url))), patchSha256);
});
const sourceFixture = process.env.MEMORY_PDS_PATH_FIXTURES;
test('complete pinned copy changes exactly two PDS files and leaves baseline untouched', { skip: !sourceFixture }, async () => fixture(async root => {
  const source = path.join(root, 'baseline'), destination = path.join(root, 'patched');
  const files = [await put(source, 'package.json', '{"version":"synthetic-wrapper"}')];
  for (const name of Object.keys(originalFiles)) {
    const bytes = await fs.readFile(path.join(sourceFixture, path.basename(name)));
    assert.equal(sha(bytes), originalFiles[name]);
    files.push(await put(source, `node_modules/@atproto/pds/dist/${name}`, bytes));
  }
  await fs.mkdir(destination);
  const updated = await preparePatchedRuntime({ baseline: source, destination, files, patcher: patchPdsFile, originals: originalFiles });
  assert.equal(updated.files.length, files.length);
  assert.equal(updated.changes.length, 2);
  assert.deepEqual(updated.files[0], files[0]);
  for (const original of files.slice(1)) {
    assert.equal(sha(await fs.readFile(path.join(source, original.path))), original.sha256);
    assert.notEqual(updated.files.find(item => item.path === original.path).sha256, original.sha256);
  }
  await assert.rejects(preparePatchedRuntime({ baseline: source, destination, files, patcher: patchPdsFile, originals: originalFiles }), /EEXIST/);
}));
