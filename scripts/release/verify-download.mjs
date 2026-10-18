// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { lstatSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { releaseIdentity } from './identity.mjs';

// GitHub normalizes ~ in uploaded asset names, including checksum sidecars.
// Preserve candidate bytes and map only the known Debian candidate names.
export function transportName(name, version) {
  const { debianVersion } = releaseIdentity(version);
  for (const arch of ['amd64', 'arm64']) {
    for (const suffix of ['', '.sha256']) {
      if (name === `codifix_${debianVersion}_${arch}.deb${suffix}`) {
        return name.replace('~rc.', '.rc.');
      }
    }
  }
  return name;
}

export function verifyDownload(candidate, download, version) {
  releaseIdentity(version);
  const files = (directory, allowDirectories = false) => readdirSync(directory).filter(name => {
    const stat = lstatSync(path.join(directory, name));
    assert(!stat.isSymbolicLink(), `asset symlink forbidden: ${name}`);
    // The collector retains empty platform directories beside upload files.
    if (stat.isDirectory()) {
      assert(allowDirectories, `unexpected download directory: ${name}`);
      return false;
    }
    assert(stat.isFile(), `asset is not a regular file: ${name}`);
    return true;
  }).sort();
  const expected = files(candidate, true);
  assert(expected.length > 0, 'empty candidate');
  const mapped = expected.map(name => transportName(name, version));
  assert.equal(new Set(mapped).size, mapped.length, 'transport name collision');
  assert.deepEqual(files(download), [...mapped].sort(), 'download asset names differ');
  const digest = filename => createHash('sha256').update(readFileSync(filename)).digest('hex');
  const mapping = [];
  for (const [index, original] of expected.entries()) {
    const fetched = mapped[index];
    assert.equal(digest(path.join(download, fetched)), digest(path.join(candidate, original)), `changed asset: ${original}`);
    if (original !== fetched) mapping.push({ candidate: original, download: fetched });
  }
  return { verified_files: expected.length, filename_mapping: mapping };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(verifyDownload(...process.argv.slice(2))));
}
