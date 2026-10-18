// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { transportName, verifyDownload } from './verify-download.mjs';

const version = '3.0.0-rc.1';
function fixture(run) {
  const root = mkdtempSync(path.join(tmpdir(), 'codifix-download-'));
  const candidate = path.join(root, 'candidate'), download = path.join(root, 'download');
  mkdirSync(candidate); mkdirSync(download);
  const names = ['SHA256SUMS', ...['amd64', 'arm64'].flatMap(arch =>
    [`codifix_3.0.0~rc.1_${arch}.deb`, `codifix_3.0.0~rc.1_${arch}.deb.sha256`])];
  for (const name of names) {
    writeFileSync(path.join(candidate, name), `original ${name}`);
    writeFileSync(path.join(download, transportName(name, version)), `original ${name}`);
  }
  try { run(candidate, download); } finally { rmSync(root, { recursive: true }); }
}

test('known GitHub transport names preserve every candidate byte', () => fixture((candidate, download) => {
  const result = verifyDownload(candidate, download, version);
  assert.equal(result.verified_files, 5);
  assert.equal(result.filename_mapping.length, 4);
  assert.equal(transportName('codifix_3.0.0_amd64.deb', '3.0.0'), 'codifix_3.0.0_amd64.deb');
  assert.equal(transportName('unrelated~rc.1.txt', version), 'unrelated~rc.1.txt');
}));

test('unexpected downloads are rejected', () => fixture((candidate, download) => {
  writeFileSync(path.join(download, 'extra'), 'unexpected');
  assert.throws(() => verifyDownload(candidate, download, version), /names differ/);
}));

test('colliding candidate names are rejected', () => fixture((candidate, download) => {
  writeFileSync(path.join(candidate, 'codifix_3.0.0.rc.1_amd64.deb'), 'ambiguous');
  assert.throws(() => verifyDownload(candidate, download, version), /collision/);
}));

for (const name of ['codifix_3.0.0.rc.1_amd64.deb', 'SHA256SUMS']) {
  test(`altered ${name} is rejected`, () => fixture((candidate, download) => {
    writeFileSync(path.join(download, name), 'changed');
    assert.throws(() => verifyDownload(candidate, download, version), /changed asset/);
  }));
}

test('download symlinks are rejected', () => fixture((candidate, download) => {
  rmSync(path.join(download, 'SHA256SUMS'));
  symlinkSync(path.join(candidate, 'SHA256SUMS'), path.join(download, 'SHA256SUMS'));
  assert.throws(() => verifyDownload(candidate, download, version), /symlink/);
}));
test('download directories are rejected', () => fixture((candidate, download) => {
  mkdirSync(path.join(download, 'extra'));
  assert.throws(() => verifyDownload(candidate, download, version), /unexpected download directory/);
}));
