// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { prepareAssets } from './assets.mjs';

test('an incomplete matrix cannot be published', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'memory-release-assets-'));
  try {
    writeFileSync(path.join(dir, 'memory-2.1.0.tar.gz'), 'synthetic');
    assert.throws(() => prepareAssets(dir, '2.1.0', 'a'.repeat(40)), /missing required artifact/);
    assert.throws(() => prepareAssets(dir, '../escape', 'a'.repeat(40)));
  } finally { rmSync(dir, { recursive: true }); }
});
