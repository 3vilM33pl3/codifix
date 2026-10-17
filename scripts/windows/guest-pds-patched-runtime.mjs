// SPDX-License-Identifier: AGPL-3.0-or-later
// Fresh offline staging only. Never modifies/starts the original PDS runtime.
import fs from 'node:fs/promises';
import { createReadStream, constants } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const run = promisify(execFile);
const hashBytes = bytes => createHash('sha256').update(bytes).digest('hex');
export const patchSha256 = '4ba316f92a401d8a9854fe96595232bb845dfe14fa33aefe51837aacc5efcef0';
const hashPattern = /^[0-9a-f]{64}$/;
const maxFiles = 65536;
export async function assertPlainPath(target) {
  for (let current = path.resolve(target);;) {
    const info = await fs.lstat(current);
    if (info.isSymbolicLink()) throw Error('Runtime staging cannot traverse links/reparse points');
    if (path.dirname(current) === current) break;
    current = path.dirname(current);
  }
}
export function checkedInventory(files) {
  if (!Array.isArray(files) || !files.length || files.length > maxFiles) throw Error('Bounded complete preparation inventory required');
  const names = new Set();
  let bytes = 0;
  for (const item of files) {
    if (!item || Object.keys(item).sort().join(',') !== 'bytes,path,sha256'
      || typeof item.path !== 'string' || !item.path.length || item.path.length > 1024
      || item.path.split('/').some(part => !part || part === '.' || part === '..' || /[\\:\x00-\x1f]/.test(part))
      || !hashPattern.test(item.sha256) || !Number.isSafeInteger(item.bytes) || item.bytes < 0 || item.bytes > 512 * 1024 * 1024
      || names.has(item.path.toLowerCase())) throw Error('Malformed, aliased or unsafe preparation inventory');
    names.add(item.path.toLowerCase()); bytes += item.bytes;
  }
  if (bytes > 2 * 1024 * 1024 * 1024) throw Error('Prepared runtime exceeds bounded payload size');
  return files;
}
export async function fileDigest(file) {
  await assertPlainPath(file);
  const info = await fs.lstat(file);
  if (!info.isFile() || info.nlink !== 1) throw Error('Regular unaliased preparation file required');
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(file)) hash.update(bytes);
  return { sha256: hash.digest('hex'), bytes: info.size };
}
async function treeNames(root) {
  await assertPlainPath(root);
  const result = [];
  let entries = 0;
  async function walk(relative) {
    for (const item of await fs.readdir(path.join(root, relative), { withFileTypes: true })) {
      if (++entries > 100000) throw Error('Runtime directory inventory exceeds its bound');
      const name = relative ? `${relative}/${item.name}` : item.name;
      if (item.isSymbolicLink()) throw Error('Runtime contains a link/reparse point');
      if (item.isDirectory()) await walk(name);
      else if (item.isFile()) result.push(name);
      else throw Error('Runtime contains a non-regular member');
      if (result.length > maxFiles) throw Error('Runtime inventory exceeds its bound');
    }
  }
  await walk('');
  return result.sort();
}
function sameNames(actual, files) {
  const expected = files.map(item => item.path).sort();
  if (actual.length !== expected.length || actual.some((name, index) => name !== expected[index])) throw Error('Runtime differs from complete prepared inventory');
}
export async function copyVerifiedRuntime(source, destination, files) {
  checkedInventory(files);
  sameNames(await treeNames(source), files);
  // Destination must be fresh, including when an earlier attempt was partial.
  await fs.mkdir(destination);
  let cursor = 0, failed;
  const copied = new Array(files.length);
  await Promise.all(Array.from({ length: Math.min(16, files.length) }, async () => {
    while (!failed) {
      const index = cursor++;
      if (index >= files.length) return;
      const item = files[index], input = path.join(source, item.path), output = path.join(destination, item.path);
      try {
        const digest = await fileDigest(input);
        if (digest.sha256 !== item.sha256 || digest.bytes !== item.bytes) throw Error('Original runtime no longer matches preparation receipt');
        await fs.mkdir(path.dirname(output), { recursive: true });
        await fs.copyFile(input, output, constants.COPYFILE_EXCL);
        const result = await fileDigest(output);
        if (result.sha256 !== item.sha256 || result.bytes !== item.bytes) throw Error('Copied runtime differs from preparation receipt');
        copied[index] = { ...item };
      } catch (error) { failed ??= error; }
    }
  }));
  if (failed) throw failed; // All in-flight workers finished; retain every byte.
  sameNames(await treeNames(destination), files);
  return copied;
}
async function boundedJson(file, limit = 32 * 1024 * 1024) {
  await assertPlainPath(file);
  const info = await fs.lstat(file);
  if (!info.isFile() || info.nlink !== 1 || info.size > limit) throw Error('Invalid bounded preparation receipt');
  const bytes = await fs.readFile(file);
  return { bytes, value: JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, '')) };
}
export async function preparePatchedRuntime({ baseline, destination, files, patcher, originals }) {
  const copied = await copyVerifiedRuntime(baseline, path.join(destination, 'runtime'), files);
  const changes = [];
  // Validate and prepare both transformations before changing either file.
  for (const name of Object.keys(originals)) {
    const relative = `node_modules/@atproto/pds/dist/${name}`;
    const item = copied.find(entry => entry.path === relative);
    if (!item || item.sha256 !== originals[name]) throw Error('Original PDS source absent from complete preparation inventory');
    const target = path.join(destination, 'runtime', relative);
    const bytes = patcher(name, await fs.readFile(target));
    changes.push({ relative, target, bytes, original_sha256: item.sha256 });
  }
  for (const change of changes) {
    await fs.writeFile(change.target, change.bytes);
    const digest = await fileDigest(change.target);
    if (digest.sha256 !== hashBytes(change.bytes)) throw Error('Patched file write was not preserved');
    Object.assign(copied.find(item => item.path === change.relative), digest);
  }
  sameNames(await treeNames(path.join(destination, 'runtime')), copied);
  return { files: copied, changes: changes.map(change => ({ path: change.relative,
    original_sha256: change.original_sha256, patched_sha256: hashBytes(change.bytes) })) };
}
async function main(args) {
  if (process.platform !== 'win32' || process.env.COMPUTERNAME !== 'MEMORY-WINLAB'
    || args.length !== 4 || args[0] !== '--operation' || args[2] !== '--source-root'
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(args[1])
    || !/^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)*$/.test(args[3]) || args[3].split('\\').includes('..')) {
    throw Error('Explicit fresh operation and copied source required in native Memory lab');
  }
  const operation = args[1], sourceRoot = args[3], root = 'C:\\MemoryLab\\NativePds';
  const baselinePath = path.join(root, 'prepared.json'), baseline = await boundedJson(baselinePath);
  const manifest = await boundedJson(path.join(sourceRoot, 'scripts/windows/native-pds.json'), 1024 * 1024);
  if (baseline.value.format !== 'memory-windows-pds-proof-result-v1' || baseline.value.native_sqlite_verified !== true
    || baseline.value.manifest_sha256 !== hashBytes(manifest.bytes) || manifest.value.pds_version !== '0.5.27'
    || manifest.value.node_version !== '24.18.1' || baseline.value.source_revision !== '374cf1d4ba782d4391bbb73e4e2d3f320d4846d6'
    || manifest.value.source_revision !== baseline.value.source_revision || baseline.value.files.length !== 40509
    || process.version !== 'v24.18.1' || (await fileDigest(process.execPath)).sha256 !== baseline.value.node_sha256) {
    throw Error('Unchanged pinned native Node/PDS baseline and complete original receipt required');
  }
  const patchFile = path.join(sourceRoot, 'packaging/windows-pds/windows-paths.mjs');
  if ((await fileDigest(patchFile)).sha256 !== patchSha256) throw Error('Filesystem patch differs from reviewed exact script');
  const patch = await import(pathToFileURL(patchFile).href);
  const destination = path.join(root, `patched-${operation}`);
  await assertPlainPath(root);
  await fs.mkdir(destination); // Never reuse a failed or previously patched copy.
  const powershell = path.join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe');
  const identity = await run(powershell, ['-NoProfile', '-NonInteractive', '-Command', '[Security.Principal.WindowsIdentity]::GetCurrent().User.Value'], { timeout: 10000, maxBuffer: 4096, windowsHide: true });
  const sid = identity.stdout.trim();
  if (!/^S-1-5-21-(?:\d+-){3}\d+$/.test(sid)) throw Error('Exact lab operator SID required');
  await run(path.join(process.env.SystemRoot, 'System32/icacls.exe'), [destination, '/inheritance:r', '/grant:r',
    `*${sid}:(OI)(CI)F`, '*S-1-5-18:(OI)(CI)F', '*S-1-5-32-544:(OI)(CI)F'], { timeout: 10000, maxBuffer: 8192, windowsHide: true });
  const updated = await preparePatchedRuntime({ baseline: path.join(root, 'runtime'), destination,
    files: baseline.value.files, patcher: patch.patchPdsFile, originals: patch.originalFiles });
  const receipt = { format: 'memory-windows-pds-patched-proof-v1', operation_id: operation,
    original_prepared_sha256: hashBytes(baseline.bytes), manifest_sha256: baseline.value.manifest_sha256,
    path_patch: { layout: 'windows-sha256-did-paths-v1', patch_sha256: patchSha256,
      files: updated.changes.map(change => ({ name: change.path.slice('node_modules/@atproto/pds/dist/'.length),
        original_sha256: change.original_sha256, patched_sha256: change.patched_sha256 })) },
    preparation_script_sha256: (await fileDigest(fileURLToPath(import.meta.url))).sha256,
    source_revision: baseline.value.source_revision,
    pds_version: manifest.value.pds_version, node_version: manifest.value.node_version, node_sha256: baseline.value.node_sha256,
    native_sqlite_verified: baseline.value.native_sqlite_verified, native_sqlite_verification_scope: 'unmodified baseline only',
    pds_protocol_verified: false, account_verified: false, fleet_verified: false, activated: false, files: updated.files };
  await fs.writeFile(path.join(destination, 'prepared.json'), JSON.stringify(receipt), { flag: 'wx' });
  console.log(JSON.stringify({ destination, receipt_sha256: hashBytes(Buffer.from(JSON.stringify(receipt))), files: updated.files.length,
    changed_files: updated.changes, pds_protocol_verified: false, activated: false }));
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) await main(process.argv.slice(2));
