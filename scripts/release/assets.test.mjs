// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { prepareAssets } from './assets.mjs';

function completeFixture(dir, commit = 'a'.repeat(40), version = '3.0.0') {
  const debianVersion = version.replace('-rc.', '~rc.');
  const win = `codifix-${version}-windows-x86_64-build.${version === '3.0.0' ? 2 : 1}`;
  const artifacts = [`${win}.zip`, `${win}.msi`, `${win}-server-tools.msi`].map(name => ({ name, sha256: createHash('sha256').update(name).digest('hex') }));
  const files = [`codifix_${debianVersion}_amd64.deb`, `codifix_${debianVersion}_arm64.deb`, `codifix-${version}.tar.gz`,
    ...artifacts.map(a => a.name), ...['x86_64', 'aarch64'].flatMap(arch => [`codifix-${version}-macos-${arch}.pkg`, `codifix-${version}-macos-${arch}-application-update.pkg`])];
  for (const name of files) writeFileSync(path.join(dir, name), name);
  writeFileSync(path.join(dir, `${win}.manifest.json`), JSON.stringify({ application_version: version, source_commit: commit, native_stack_bundled: false, artifacts }));
  for (const name of ['build-amd64.json', 'build-arm64.json', ...['x86_64', 'aarch64'].map(arch => `codifix-${version}-macos-${arch}-build.json`)]) {
    const arch = name.match(/^build-(amd64|arm64)\.json$/)?.[1];
    const selected = arch ? [`codifix_${debianVersion}_${arch}.deb`] : files.filter(file => file.startsWith(name.replace('-build.json', '')) && file.endsWith('.pkg'));
    const digests = Object.fromEntries(selected.map(file => [file, createHash('sha256').update(readFileSync(path.join(dir, file))).digest('hex')]));
    writeFileSync(path.join(dir, name), JSON.stringify({ application_version: version, source_commit: commit, architecture: arch, artifacts: digests }));
  }
}

test('an incomplete matrix cannot be published', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'codifix-release-assets-'));
  try {
    writeFileSync(path.join(dir, 'codifix-3.0.0.tar.gz'), 'synthetic');
    assert.throws(() => prepareAssets(dir, '3.0.0', 'a'.repeat(40)), /missing required artifact/);
    assert.throws(() => prepareAssets(dir, '../escape', 'a'.repeat(40)));
  } finally { rmSync(dir, { recursive: true }); }
});

test('complete frozen matrix produces verified inventory and checksums', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'codifix-release-assets-'));
  try {
    completeFixture(dir);
    const inventory = prepareAssets(dir, '3.0.0', 'a'.repeat(40));
    assert.equal(inventory.artifacts.length, 15);
    for (const row of readFileSync(path.join(dir, 'SHA256SUMS'), 'utf8').trim().split('\n')) {
      const [expected, name] = row.split('  ');
      assert.equal(createHash('sha256').update(readFileSync(path.join(dir, name))).digest('hex'), expected);
      assert.equal(readFileSync(path.join(dir, `${name}.sha256`), 'utf8'), `${row}\n`);
    }
  } finally { rmSync(dir, { recursive: true }); }
});

test('a changed platform source commit blocks publication', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'codifix-release-assets-'));
  try {
    completeFixture(dir);
    writeFileSync(path.join(dir, 'build-arm64.json'), JSON.stringify({ application_version: '3.0.0', source_commit: 'b'.repeat(40) }));
    assert.throws(() => prepareAssets(dir, '3.0.0', 'a'.repeat(40)), /build-arm64.json/);
  } finally { rmSync(dir, { recursive: true }); }
});

test('package substitution cannot reuse a valid build receipt', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'codifix-release-assets-'));
  try {
    completeFixture(dir);
    writeFileSync(path.join(dir, 'codifix_3.0.0_arm64.deb'), 'stale or changed package');
    assert.throws(() => prepareAssets(dir, '3.0.0', 'a'.repeat(40)), /build-arm64.json/);
  } finally { rmSync(dir, { recursive: true }); }
});

test('candidate matrix binds Debian ordering and allocated Windows build', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'codifix-release-candidate-'));
  try {
    completeFixture(dir, 'a'.repeat(40), '3.0.0-rc.1');
    const inventory = prepareAssets(dir, '3.0.0-rc.1', 'a'.repeat(40));
    assert(inventory.artifacts.some(a => a.name === 'codifix_3.0.0~rc.1_amd64.deb'));
    assert(inventory.artifacts.some(a => a.name === 'codifix-3.0.0-rc.1-windows-x86_64-build.1.msi'));
    assert.throws(() => prepareAssets(dir, '3.0.0-rc.2', 'a'.repeat(40)), /unallocated/);
  } finally { rmSync(dir, { recursive: true }); }
});
