// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, lstatSync, writeFileSync, renameSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function prepareAssets(directory, version, commit) {
  assert.match(version, /^\d+\.\d+\.\d+$/);
  assert.match(commit, /^[0-9a-f]{40}$/);
  const root = path.resolve(directory);
  const walk = dir => readdirSync(dir).flatMap(name => {
    const p = path.join(dir, name), stat = lstatSync(p);
    assert(!stat.isSymbolicLink(), 'artifact links are forbidden');
    return stat.isDirectory() ? walk(p) : [p];
  });
  const files = walk(root);
  const names = new Set();
  for (const file of files) {
    const name = path.basename(file);
    // Per-platform aggregate files are replaced by one release aggregate.
    if (name === 'SHA256SUMS') continue;
    assert(!names.has(name), `duplicate artifact basename: ${name}`);
    names.add(name);
    if (path.dirname(file) !== root) renameSync(file, path.join(root, name));
  }
  const win = `memory-layer-${version}-windows-x86_64-build.1`;
  const required = [
    `memory-layer_${version}_amd64.deb`, `memory-layer_${version}_arm64.deb`,
    ...['x86_64', 'aarch64'].flatMap(arch => [`memory-layer-${version}-macos-${arch}.pkg`, `memory-layer-${version}-macos-${arch}-application-update.pkg`]),
    `${win}.zip`, `${win}.msi`, `${win}-server-tools.msi`, `${win}.manifest.json`,
    `memory-${version}.tar.gz`,
  ];
  for (const name of required) assert(names.has(name), `missing required artifact: ${name}`);
  const manifest = JSON.parse(readFileSync(path.join(root, `${win}.manifest.json`), 'utf8').replace(/^\uFEFF/, ''));
  assert.equal(manifest.application_version, version);
  assert.equal(manifest.source_commit, commit);
  assert.equal(manifest.native_stack_bundled, false);
  const hash = name => createHash('sha256').update(readFileSync(path.join(root, name))).digest('hex');
  for (const artifact of manifest.artifacts) assert.equal(hash(artifact.name), artifact.sha256, artifact.name);
  const payloads = [...names].filter(name => !name.endsWith('.sha256') && !name.startsWith('SHA256SUMS')).sort();
  const artifacts = payloads.map(name => ({ name, size_bytes: lstatSync(path.join(root, name)).size, sha256: hash(name) }));
  for (const artifact of artifacts) assert(artifact.size_bytes > 0, `empty artifact ${artifact.name}`);
  const inventory = { format: 'memory-release-assets-v1', version, source_commit: commit, windows_status: 'preview', artifacts };
  writeFileSync(path.join(root, 'release-inventory.json'), JSON.stringify(inventory, null, 2) + '\n', { flag: 'wx' });
  payloads.push('release-inventory.json');
  const sums = payloads.sort().map(name => `${hash(name)}  ${name}\n`);
  payloads.forEach((name, index) => writeFileSync(path.join(root, `${name}.sha256`), sums[index]));
  writeFileSync(path.join(root, 'SHA256SUMS'), sums.join(''));
  return inventory;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(prepareAssets(...process.argv.slice(2))));
}
