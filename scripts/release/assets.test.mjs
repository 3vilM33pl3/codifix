// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { prepareAssets } from './assets.mjs';

function completeFixture(dir, commit = 'a'.repeat(40)) {
  const version = '2.1.0', win = `memory-layer-${version}-windows-x86_64-build.1`;
  const artifacts = [`${win}.zip`, `${win}.msi`, `${win}-server-tools.msi`].map(name => ({ name, sha256: createHash('sha256').update(name).digest('hex') }));
  const files = [`memory-layer_${version}_amd64.deb`, `memory-layer_${version}_arm64.deb`, `memory-${version}.tar.gz`,
    ...artifacts.map(a => a.name), ...['x86_64', 'aarch64'].flatMap(arch => [`memory-layer-${version}-macos-${arch}.pkg`, `memory-layer-${version}-macos-${arch}-application-update.pkg`])];
  for (const name of files) writeFileSync(path.join(dir, name), name);
  writeFileSync(path.join(dir, `${win}.manifest.json`), JSON.stringify({ application_version: version, source_commit: commit, native_stack_bundled: false, artifacts }));
  for (const name of ['build-amd64.json', 'build-arm64.json', ...['x86_64', 'aarch64'].map(arch => `memory-layer-${version}-macos-${arch}-build.json`)]) {
    writeFileSync(path.join(dir, name), JSON.stringify({ application_version: version, source_commit: commit }));
  }
}

test('an incomplete matrix cannot be published', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'memory-release-assets-'));
  try {
    writeFileSync(path.join(dir, 'memory-2.1.0.tar.gz'), 'synthetic');
    assert.throws(() => prepareAssets(dir, '2.1.0', 'a'.repeat(40)), /missing required artifact/);
    assert.throws(() => prepareAssets(dir, '../escape', 'a'.repeat(40)));
  } finally { rmSync(dir, { recursive: true }); }
});

test('complete frozen matrix produces verified inventory and checksums', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'memory-release-assets-'));
  try {
    completeFixture(dir);
    const inventory = prepareAssets(dir, '2.1.0', 'a'.repeat(40));
    assert.equal(inventory.artifacts.length, 15);
    for (const row of readFileSync(path.join(dir, 'SHA256SUMS'), 'utf8').trim().split('\n')) {
      const [expected, name] = row.split('  ');
      assert.equal(createHash('sha256').update(readFileSync(path.join(dir, name))).digest('hex'), expected);
      assert.equal(readFileSync(path.join(dir, `${name}.sha256`), 'utf8'), `${row}\n`);
    }
  } finally { rmSync(dir, { recursive: true }); }
});

test('a changed platform source commit blocks publication', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'memory-release-assets-'));
  try {
    completeFixture(dir);
    writeFileSync(path.join(dir, 'build-arm64.json'), JSON.stringify({ application_version: '2.1.0', source_commit: 'b'.repeat(40) }));
    assert.throws(() => prepareAssets(dir, '2.1.0', 'a'.repeat(40)), /build-arm64.json/);
  } finally { rmSync(dir, { recursive: true }); }
});
