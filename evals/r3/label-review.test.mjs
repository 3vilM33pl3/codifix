// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

test('label review guide names the current clean fixture tree and all 51 family sheets', async () => {
  const root = path.dirname(fileURLToPath(import.meta.url));
  const repo = path.resolve(root, '../..');
  const guide = await fs.readFile(path.join(root, 'label-review.md'), 'utf8');
  const git = args => spawnSync('git', args, { cwd: repo, encoding: 'utf8' });
  const tree = git(['rev-parse', '--verify', 'HEAD:evals/r3/representative']);
  assert.equal(tree.status, 0);
  assert(guide.includes('`' + tree.stdout.trim() + '`'), 'refresh the reviewed fixture identity after intentional fixture changes');
  assert.equal(git(['diff', '--quiet', 'HEAD', '--', 'evals/r3/representative']).status, 0,
    'uncommitted fixture changes cannot use the committed label-review identity');
  const untracked = git(['ls-files', '--others', '--exclude-standard', '--', 'evals/r3/representative']);
  assert.equal(untracked.status, 0);
  assert.equal(untracked.stdout.trim(), '', 'new fixture inputs need a committed review identity');
  const sheets = [...guide.matchAll(/\]\((representative\/[^)]+\/review\.md)\)/g)].map(match => match[1]);
  assert.equal(sheets.length, 51);
  assert.equal(new Set(sheets).size, 51);
  const counts = { development: 0, 'held-out': 0 };
  for (const sheet of sheets) {
    assert(/^representative\/(development|held-out)\/[a-z0-9-]+\/review\.md$/.test(sheet));
    assert((await fs.stat(path.join(root, sheet))).isFile());
    counts[sheet.split('/')[1]]++;
  }
  assert.deepEqual(counts, { development: 14, 'held-out': 37 });
});
