// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import vm from 'node:vm';
import { createHash } from 'node:crypto';
import { patchPdsFile, originalFiles } from './windows-paths.mjs';

test('unknown or altered PDS sources fail before transformation', () => {
  assert.throws(() => patchPdsFile('other.js', Buffer.from('unreviewed')), /exact reviewed/);
  for (const name of Object.keys(originalFiles)) assert.throws(() => patchPdsFile(name, Buffer.from('changed')), /exact reviewed/);
});
const fixture = process.env.MEMORY_PDS_PATH_FIXTURES;
const hash = value => createHash('sha256').update(value).digest('hex');
async function exercise(text, kind, platform) {
  const files = new Map(), removed = [];
  const fileSystem = {
    async mkdir() {}, async writeFile(p, value) { files.set(p, Buffer.from(value)); },
    async readFile(p) { assert(files.has(p)); return files.get(p); },
    async rename(a, b) { assert(files.has(a)); files.set(b, files.get(a)); files.delete(a); },
    async rm(p) { files.delete(p); },
  };
  const remove = async p => { removed.push(p); for (const name of files.keys()) if (name === p || name.startsWith(p + (platform === 'win32' ? '\\' : '/'))) files.delete(name); };
  const did = 'did:web:localhost%3A12345', keyDid = 'did:key:zsynthetic';
  const key = { did: () => keyDid, export: async () => Buffer.from('synthetic private fixture') };
  const context = { assert, Buffer, process: { platform }, path: platform === 'win32' ? path.win32 : path.posix,
    fs: fileSystem, fsSync: {}, mkdir: fileSystem.mkdir, createHash,
    crypto: { sha256Hex: async value => hash(value), Secp256k1Keypair: { create: async () => key, import: async () => key } },
    fileExists: async p => files.has(p), readIfExists: async p => files.get(p), rmIfExists: remove,
    blobStoreLogger: {}, log: {}, randomStr: () => 'temporary-key', aggregateErrors: errors => new AggregateError(errors),
    chunkArray: values => [values], isErrnoException: () => false, BlobNotFoundError: Error };
  // Execute the exact pinned classes with filesystem/crypto facades. Real PDS,
  // Windows filesystem, signing and account lifecycle remain VM acceptance.
  const className = kind === 'actor' ? 'ActorStore' : 'DiskBlobStore';
  const executable = text.replace(/^import[\s\S]*?from ['"][^'"]+['"];\r?\n/gm, '')
    .replace(`export class ${className}`, `class ${className}`) + `\nglobalThis.Subject=${className};`;
  vm.runInNewContext(executable, context);
  const base = platform === 'win32' ? 'C:\\private' : '/private';
  const join = context.path.join;
  let observed;
  if (kind === 'actor') {
    const store = new context.Subject({ directory: base }, {});
    observed = await store.getLocation(did);
    assert.equal(await store.reserveKeypair(did), keyDid);
    assert.equal(await store.reserveKeypair(), keyDid);
    assert.equal((await store.getReservedKeypair(did)).did(), keyDid);
    assert.equal((await store.getReservedKeypair(keyDid)).did(), keyDid);
    const segment = platform === 'win32' ? hash(did) : did;
    assert(files.has(join(base, 'reserved_keys', segment)));
    assert.equal(observed.directory, join(base, hash(did).slice(0, 2), segment));
    await store.clearReservedKeypair(keyDid, did);
    assert.equal(files.size, 0);
  } else {
    const store = new context.Subject(did, join(base, 'blobs'), join(base, 'temp'), join(base, 'quarantine'));
    assert.equal(store.did, did);
    const cid = { toString: () => 'bafy-synthetic' };
    const key = await store.putTemp(Buffer.from('reviewed bytes'));
    await store.makePermanent(key, cid);
    assert.equal((await store.getBytes(cid)).toString(), 'reviewed bytes');
    await store.quarantine(cid);
    assert.equal(await store.hasStored(cid), false);
    await store.unquarantine(cid);
    assert.equal((await store.getBytes(cid)).toString(), 'reviewed bytes');
    observed = { stored: store.getStoredPath(cid), temporary: store.getTmpPath('next'), quarantine: store.getQuarantinePath(cid) };
    assert.equal(observed.stored, join(base, 'blobs', platform === 'win32' ? hash(did) : did, cid.toString()));
    await store.deleteAll();
    assert.equal(files.size, 0);
    assert.equal(removed.length, 3);
  }
  return JSON.parse(JSON.stringify(observed));
}
for (const [name, kind] of [['actor-store/actor-store.js', 'actor'], ['disk-blobstore.js', 'blob']]) {
  test(`pinned ${kind}: Windows paths and unchanged Linux lifecycle`, { skip: !fixture }, async () => {
    const original = await fs.readFile(path.join(fixture, path.basename(name)));
    assert.equal(hash(original), originalFiles[name]);
    const patched = patchPdsFile(name, original).toString();
    assert.throws(() => patchPdsFile(name, Buffer.from(patched)), /exact reviewed/);
    await exercise(patched, kind, 'win32');
    assert.deepEqual(await exercise(patched, kind, 'linux'), await exercise(original.toString(), kind, 'linux'));
  });
}
