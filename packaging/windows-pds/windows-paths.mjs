// SPDX-License-Identifier: AGPL-3.0-or-later
// Narrow, hash-bound filesystem portability patch for upstream PDS0.5.27.
// No DID, CID, repository, signature, auth or network behavior is rewritten.
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export const originalFiles = Object.freeze({
  'actor-store/actor-store.js': '33ba4bcf0b0afb06868b80a4cb0025388a2a7e8006338ce80a70785d2079af4f',
  'disk-blobstore.js': 'cc18ec422b23ba3bc06baea321146e512ce9cb14608ec90ccfe5e9bc71359811',
});
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
function replaceExactly(text, from, to, count) {
  const parts = text.split(from);
  if (parts.length !== count + 1) throw Error('Pinned Windows path patch replacement count changed');
  return parts.join(to);
}
export function patchPdsFile(name, bytes) {
  if (!Object.hasOwn(originalFiles, name) || sha(bytes) !== originalFiles[name]) throw Error('PDS source differs from exact reviewed upstream bytes');
  let text = Buffer.from(bytes).toString('utf8');
  if (name === 'actor-store/actor-store.js') {
    text = replaceExactly(text, 'path.join(this.cfg.directory, didHash.slice(0, 2), did)',
      "path.join(this.cfg.directory, didHash.slice(0, 2), process.platform === 'win32' ? didHash : did)", 1);
    for (const [identity, count] of [['did', 2], ['keyDid', 2], ['signingKeyOrDid', 1]]) {
      text = replaceExactly(text, `path.join(this.reservedKeyDir, ${identity})`,
        `path.join(this.reservedKeyDir, process.platform === 'win32' ? await crypto.sha256Hex(${identity}) : ${identity})`, count);
    }
  } else {
    text = replaceExactly(text, "import path from 'node:path';",
      "import path from 'node:path';\nimport { createHash } from 'node:crypto';", 1);
    text = replaceExactly(text, 'this.did = did;',
      "this.did = did;\n        this.pathDid = process.platform === 'win32' ? createHash('sha256').update(did).digest('hex') : did;", 1);
    for (const directory of ['location','tmpLocation','quarantineLocation']) {
      text = replaceExactly(text, `path.join(this.${directory}, this.did`, `path.join(this.${directory}, this.pathDid`, 3);
    }
  }
  return Buffer.from(text);
}

async function main() {
  const [runtime, receiptPath] = process.argv.slice(2);
  const relative = runtime && receiptPath ? path.relative(runtime, receiptPath) : '';
  const outside = relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative);
  if (process.platform !== 'win32' || !runtime || !receiptPath || !path.isAbsolute(runtime)
      || !path.isAbsolute(receiptPath) || !outside) {
    throw Error('Explicit native runtime copy and separate new receipt path required');
  }
  try { await fs.lstat(receiptPath); throw Error('Existing patch receipt; no files changed'); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const base = path.join(runtime, 'node_modules/@atproto/pds');
  if (JSON.parse(await fs.readFile(path.join(base, 'package.json'))).version !== '0.5.27') throw Error('Pinned PDS version required');
  // Validate all original files before modifying either one. The caller owns a
  // fresh private staging copy; never apply to a running/previously patched PDS.
  const files = [];
  for (const name of Object.keys(originalFiles)) {
    const file = path.join(base, 'dist', name), metadata = await fs.lstat(file);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.nlink !== 1) throw Error('Regular unaliased staging file required');
    const bytes = patchPdsFile(name, await fs.readFile(file));
    files.push({ name, file, bytes });
  }
  const receipt = { format: 'memory-windows-pds-path-patch-v1', upstream_version: '0.5.27',
    layout: 'windows-sha256-did-paths-v1', patch_sha256: sha(await fs.readFile(process.argv[1])),
    linux_paths_unchanged: true, wire_identity_unchanged: true, files: [] };
  for (const { name, file, bytes } of files) {
    await fs.writeFile(file, bytes);
    receipt.files.push({ name, original_sha256: originalFiles[name], patched_sha256: sha(bytes) });
  }
  await fs.writeFile(receiptPath, JSON.stringify(receipt), { flag: 'wx' });
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) await main();
