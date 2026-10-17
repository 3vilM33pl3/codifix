// SPDX-License-Identifier: AGPL-3.0-or-later
// Packaging-only fixtures: no approvals, database, executable/provider calls.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { runtimeFiles, packagePaidRuntime, verifyPaidRuntime, digest } from './paid-runtime.mjs';

function dependencies(source) {
  return [...source.matchAll(/(?:import|export)\s+(?:[^'";]+?\s+from\s*)?['"](\.{1,2}\/[^'"]+\.mjs)['"]/g)].map(match => match[1]);
}
function assertClosure(files, contents) {
  const selected = new Set(files);
  assert.equal(selected.size, files.length, 'runtime closure must have no duplicate assets');
  for (const file of files) for (const imported of dependencies(contents.get(file))) {
    const dependency = path.posix.normalize(path.posix.join(path.posix.dirname(file), imported));
    assert(selected.has(dependency), `${file} imports unlocked ${dependency}`);
  }
}

test('paid runtime asset list closes every relative module import, including native seams', async () => {
  const contents = new Map(await Promise.all(runtimeFiles.map(async name => [name,
    await fs.readFile(new URL(name, import.meta.url), 'utf8')])));
  assertClosure(runtimeFiles, contents);
  for (const required of ['windows-dispatch-auth.mjs', 'windows-dispatch-client.mjs', 'windows-host-journal.mjs',
    'isolation/execution.mjs', 'isolation/native-windows.mjs', 'isolation/native-guest.mjs']) assert(runtimeFiles.includes(required));
});

test('new draft packaging locks the complete module graph without executing any binary', async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-paid-closure-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const root = path.join(directory, 'pack'); await fs.mkdir(path.join(root, 'private'), { recursive: true });
  await fs.writeFile(path.join(root, 'campaign.json'), JSON.stringify({ label_status: 'draft', provider_profile: { version: 2 }, assets: [] }));
  const fake = path.join(directory, 'never-executed'); await fs.writeFile(fake, 'Non-executable package-fixture bytes.');
  const binaries = Object.fromEntries(['bao', 'budget', 'memory', 'node', 'retriever', 'score', 'snapshot'].map(name => [name, fake]));
  const images = { node: `sha256:${digest('node fixture')}`, rust: `sha256:${digest('rust fixture')}` };
  const runtime = await packagePaidRuntime(root, { binaries, images });
  const { manifest } = await verifyPaidRuntime(root);
  assert.equal(manifest.label_status, 'draft');
  assert.equal(runtime.files.length, runtimeFiles.length);
  const contents = new Map();
  for (const pin of runtime.files) {
    const bytes = await fs.readFile(path.join(root, pin.path));
    assert.equal(digest(bytes), pin.sha256);
    assert(manifest.assets.some(asset => asset.path === pin.path && asset.sha256 === pin.sha256 && asset.kind === 'configuration'));
    contents.set(pin.path.slice('runtime/'.length), bytes.toString('utf8'));
  }
  assertClosure(runtimeFiles, contents);
  const auth = path.join(root, 'runtime/windows-dispatch-auth.mjs');
  await fs.chmod(auth, 0o600); await fs.appendFile(auth, '\n// test-owned changed dependency\n');
  await assert.rejects(verifyPaidRuntime(root), /runtime source changed/);
});
