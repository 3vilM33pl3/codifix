// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';

const binary = process.env.MEMORY_R3_CLI_BIN;
if (process.env.MEMORY_R3_REQUIRE_REPRESENTATIVE_PACK === '1' && !binary) throw Error('required campaign CLI missing');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
test('representative development packs are reproducible, private, whole-family and hash-locked', async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-representative-pack-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const first = path.join(directory, 'first'), second = path.join(directory, 'second'), minimal = path.join(directory, 'minimal');
  const generate = (out, ...options) => spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', out, ...options], {
    encoding: 'utf8', env: { PATH: '/usr/bin:/bin', HOME: directory }, timeout: 10000 });
  for (const out of [first, second]) assert.equal(generate(out).status, 0);
  assert.equal(generate(first).status, 1, 'never overwrite even an existing draft');
  assert.equal(generate(minimal, '--one-family-per-track').status, 0);
  const bytes = await fs.readFile(path.join(first, 'campaign.json'));
  assert.deepEqual(bytes, await fs.readFile(path.join(second, 'campaign.json')));
  const manifest = JSON.parse(bytes);
  assert(manifest.smoke_only && manifest.label_status === 'draft' && manifest.limits.microusd === 0);
  assert.equal(manifest.cases.filter(c => c.track === 'knowledge').length, 32);
  assert.equal(manifest.cases.filter(c => c.track === 'coding').length, 4);
  assert(manifest.cases.every(c => c.split === 'development'));
  for (const asset of manifest.assets) {
    const file = path.join(first, asset.path);
    assert.equal(hash(await fs.readFile(file)), asset.sha256, asset.path);
    assert(!asset.path.includes('/target/'));
    assert.equal((await fs.stat(file)).mode & 0o777, 0o600);
  }
  assert.equal((await fs.stat(first)).mode & 0o777, 0o700);
  for (const item of manifest.cases.filter(c => c.track === 'knowledge')) {
    const rubricBytes = await fs.readFile(path.join(first, 'private/rubrics', `${item.id}.json`));
    assert.equal(JSON.parse(rubricBytes).case_id, item.id);
    assert.equal(rubricBytes.at(-1), 10);
    assert.equal(item.sources.length, 5);
    assert(item.sources.every(source => source.startsWith(`public/sources/${item.cluster}/`)));
  }
  const small = JSON.parse(await fs.readFile(path.join(minimal, 'campaign.json')));
  assert.equal(small.cases.length, 5);
  assert.equal(new Set(small.cases.map(c => c.cluster)).size, 2);
});

test('existing campaign CLI loads representative suites and refuses tampering or promotion', { skip: !binary }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-representative-cli-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const out = path.join(directory, 'pack');
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, MEMORY_LAYER_PROFILE: 'prod' };
  assert.equal(spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', out, '--one-family-per-track'], { env }).status, 0);
  const manifestPath = path.join(out, 'campaign.json');
  const plan = split => spawnSync(binary, ['eval', 'campaign', 'plan', '--manifest', manifestPath, '--split', split], {
    env, encoding: 'utf8', timeout: 10000, maxBuffer: 1048576 });
  const result = plan('development');
  assert.equal(result.status, 0, result.stderr);
  const prepared = JSON.parse(result.stdout);
  assert.equal(prepared.work.length, 75);
  assert(prepared.readiness_errors.some(reason => reason.includes('smoke-only')));
  assert.equal(plan('held-out').status, 1, 'there is no materialized held-out split');
  const manifest = JSON.parse(await fs.readFile(manifestPath));
  const asset = manifest.assets.find(a => a.kind === 'source');
  const target = path.join(out, asset.path);
  const original = await fs.readFile(target);
  await fs.appendFile(target, 'changed source');
  assert.equal(plan('development').status, 1);
  await fs.writeFile(target, original);
  manifest.smoke_only = false;
  manifest.label_status = 'reviewed';
  await fs.writeFile(manifestPath, JSON.stringify(manifest));
  assert.equal(plan('development').status, 1, 'changing flags cannot make partial counts official');
});
