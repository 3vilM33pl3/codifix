// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';

const binary = process.env.CODIFIX_R3_CLI_BIN;
if (process.env.CODIFIX_R3_REQUIRE_REPRESENTATIVE_PACK === '1' && !binary) throw Error('required campaign CLI missing');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
test('calibration selects only the proposed whole development families without approving labels', async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-calibration-families-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const root = path.join(directory, 'pack');
  const result = spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', root, '--calibration'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const m = JSON.parse(await fs.readFile(path.join(root, 'campaign.json')));
  assert.deepEqual([...new Set(m.cases.map(c => c.cluster))].sort(), ['artifact-origin', 'configuration-rollout']);
  assert.equal(m.cases.length, 5);
  assert(m.cases.every(c => c.split === 'development'));
  assert.equal(m.label_status, 'draft'); assert.equal(m.smoke_only, true);
});
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
  assert.equal(manifest.limits.requests, manifest.cases.length * 5 * 3);
  assert.equal(manifest.cases.filter(c => c.track === 'knowledge').length, 40);
  assert.equal(manifest.cases.filter(c => c.track === 'coding').length, 4);
  assert(manifest.cases.every(c => c.split === 'development'));
  assert(manifest.cases.every(c => !['telemetry-units', 'framed-stream'].includes(c.cluster)));
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
  const families = JSON.parse(await fs.readFile(path.join(first, 'private/families.json')));
  const materialized = (await fs.readdir('evals/r3/representative/development')).sort();
  assert.deepEqual(families.map(f => f.id).sort(), materialized, 'every materialized family participates');
  assert.equal(new Set(families.map(f => f.id)).size, 14);
  for (const family of families) {
    assert.equal(family.cases.length, family.track === 'knowledge' ? 4 : 1);
    assert(family.cases.every(id => manifest.cases.some(item => item.id === id && item.cluster === family.id)));
  }
});

test('explicit held-out fixture packs remain separate, draft and non-promotable', { skip: !binary }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-held-out-cli-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const out = path.join(directory, 'pack');
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, CODIFIX_PROFILE: 'prod' };
  assert.equal(spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', out, '--held-out'], { env }).status, 0);
  const manifestPath = path.join(out, 'campaign.json');
  const manifest = JSON.parse(await fs.readFile(manifestPath));
  assert.equal(manifest.cases.length, 112);
  assert(manifest.cases.every(item => item.split === 'held_out'));
  const families = JSON.parse(await fs.readFile(path.join(out, 'private/families.json')));
  assert.deepEqual(families.map(f => f.id).sort(), (await fs.readdir('evals/r3/representative/held-out')).sort());
  for (const asset of manifest.assets) assert.equal(hash(await fs.readFile(path.join(out, asset.path))), asset.sha256);
  const plan = split => spawnSync(binary, ['eval', 'campaign', 'plan', '--manifest', manifestPath, '--split', split], {
    env, encoding: 'utf8', timeout: 10000, maxBuffer: 1048576 });
  const result = plan('held-out');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).work.length, 1680);
  assert.equal(manifest.limits.requests, JSON.parse(result.stdout).work.length);
  assert.equal(plan('development').status, 1, 'held-out data is not a calibration pack');
  manifest.smoke_only = false;
  manifest.label_status = 'reviewed';
  await fs.writeFile(manifestPath, JSON.stringify(manifest));
  assert.equal(plan('held-out').status, 1, 'partial held-out fixtures are not approved official evidence');
});

test('existing campaign CLI loads representative suites and refuses tampering or promotion', { skip: !binary }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-representative-cli-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const out = path.join(directory, 'pack');
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, CODIFIX_PROFILE: 'prod' };
  assert.equal(spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', out, '--one-family-per-track'], { env }).status, 0);
  const manifestPath = path.join(out, 'campaign.json');
  const plan = split => spawnSync(binary, ['eval', 'campaign', 'plan', '--manifest', manifestPath, '--split', split], {
    env, encoding: 'utf8', timeout: 10000, maxBuffer: 1048576 });
  const result = plan('development');
  assert.equal(result.status, 0, result.stderr);
  const prepared = JSON.parse(result.stdout);
  assert.equal(prepared.work.length, 75);
  assert(prepared.readiness_errors.some(reason => reason.includes('labels')));
  assert(prepared.readiness_errors.some(reason => reason.includes('budget')));
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

test('complete development allocation plans all 660 executions but cannot become held-out evidence', { skip: !binary }, async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-development-cli-'));
  t.after(() => fs.rm(directory, { recursive: true }));
  const out = path.join(directory, 'pack');
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory, CODIFIX_PROFILE: 'prod' };
  assert.equal(spawnSync(process.execPath, ['evals/r3/build-representative-pack.mjs', out], { env }).status, 0);
  const manifestPath = path.join(out, 'campaign.json');
  const plan = split => spawnSync(binary, ['eval', 'campaign', 'plan', '--manifest', manifestPath, '--split', split], {
    env, encoding: 'utf8', timeout: 10000, maxBuffer: 4 * 1048576 });
  const result = plan('development');
  assert.equal(result.status, 0, result.stderr);
  const prepared = JSON.parse(result.stdout);
  assert.equal(prepared.work.length, 660);
  assert.equal(JSON.parse(await fs.readFile(manifestPath)).limits.requests, prepared.work.length);
  assert.equal(new Set(prepared.work.map(work => `${work.case_id}:${work.condition}:${work.repeat}`)).size, 660);
  assert.equal(new Set(prepared.work.map(work => work.case_id)).size, 44);
  assert(prepared.readiness_errors.some(reason => reason.includes('labels')));
  assert(prepared.readiness_errors.some(reason => reason.includes('budget')));
  assert.equal(plan('held-out').status, 1);
  const manifest = JSON.parse(await fs.readFile(manifestPath));
  manifest.smoke_only = false;
  manifest.label_status = 'reviewed';
  await fs.writeFile(manifestPath, JSON.stringify(manifest));
  assert.equal(plan('development').status, 1, 'complete development is not the complete approved benchmark');
});
