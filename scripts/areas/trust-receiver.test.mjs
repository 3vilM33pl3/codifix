// SPDX-License-Identifier: AGPL-3.0-or-later
// Opt-in end-to-end execution of the portable Python receiver against a
// protected locally generated bundle and a fresh network-none PostgreSQL 17.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash, randomUUID } from 'node:crypto';
import { mkdtemp, mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { TRUST_RECEIVER_FIXTURE, TRUST_RECEIVER_PROFILE } from './trust-receiver-profile.mjs';

const clean = { PATH: '/usr/bin:/bin', LC_ALL: 'C', LANG: 'C', TZ: 'UTC' };
const hash = value => createHash('sha256').update(value).digest('hex');
const RUNNER = 'sha256:3dd0714e9d5b1fdb988447902fb33c42566274d8f00dc1ecceb20ef38d11dcd0';

test('portable receiver inspects, restores, verifies and refuses replay', {
  skip: process.env.CODIFIX_AREA_TEST_RECEIVER !== '1', timeout: 300_000,
}, async t => {
  const bundle = resolve(process.env.CODIFIX_TRUST_RECEIVER_BUNDLE ?? '');
  const manifestSha = process.env.CODIFIX_TRUST_RECEIVER_MANIFEST_SHA256 ?? '';
  assert.match(manifestSha, /^[0-9a-f]{64}$/);
  const manifestBytes = await readFile(join(bundle, 'manifest.json'));
  assert.equal(hash(manifestBytes), manifestSha);
  const manifest = JSON.parse(manifestBytes);
  const root = await mkdtemp(join(tmpdir(), 'memory-trust-receiver-e2e-'));
  const socket = join(root, 'socket'), work = join(root, 'work'); await mkdir(socket, { mode: 0o777 }); await mkdir(work, { mode: 0o700 });
  const run = (command, args, options = {}) => {
    const result = spawnSync(command, args, { env: clean, timeout: 180_000, maxBuffer: 8 * 1024 * 1024, ...options });
    return result;
  };
  const name = `memory-trust-receiver-e2e-${randomUUID()}`;
  const started = run('docker', ['run', '--pull=never', '--detach', '--name', name, '--network', 'none', '--read-only',
    '--memory', '1g', '--cpus', '2', '--tmpfs', '/var/lib/postgresql/data:rw,size=512m', '--tmpfs', '/tmp:rw',
    '--mount', `type=bind,source=${socket},target=/var/run/postgresql`, '--env', 'POSTGRES_HOST_AUTH_METHOD=trust',
    TRUST_RECEIVER_FIXTURE.image_id, '-c', 'unix_socket_directories=/var/run/postgresql',
    '-c', `port=${TRUST_RECEIVER_PROFILE.database_port}`]);
  assert.equal(started.status, 0); const server = started.stdout.toString().trim();
  t.after(async () => {
    spawnSync('docker', ['exec', server, 'chmod', '0777', '/var/run/postgresql'], { env: clean, stdio: 'ignore', timeout: 5000 });
    spawnSync('docker', ['rm', '--force', server], { env: clean, stdio: 'ignore', timeout: 30_000 });
    await rm(root, { recursive: true, force: true });
  });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    const main = run('docker', ['exec', server, 'cat', '/proc/1/comm'], { encoding: 'utf8', timeout: 5000 });
    const health = run('docker', ['exec', server, 'pg_isready', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres'],
      { stdio: 'ignore', timeout: 5000 });
    if (main.stdout?.trim() === 'postgres' && health.status === 0) { ready = true; break; } await delay(100);
  }
  assert.equal(ready, true);
  for (const args of [
    ['createuser', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres', '--login', TRUST_RECEIVER_PROFILE.role],
    ['createdb', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres', '--owner', TRUST_RECEIVER_PROFILE.role,
      TRUST_RECEIVER_PROFILE.database],
  ]) assert.equal(run('docker', ['exec', server, ...args]).status, 0);
  const target = { format: 'memory-trust-receiver-target-v1',
    connection: { kind: 'unix', directory: '/receiver-socket', port: TRUST_RECEIVER_PROFILE.database_port },
    database: TRUST_RECEIVER_PROFILE.database, admin_username: 'postgres', owner_role: TRUST_RECEIVER_PROFILE.role };
  const targetBytes = Buffer.from(JSON.stringify(target)); const targetPath = join(work, 'target.json');
  await writeFile(targetPath, targetBytes, { mode: 0o600, flag: 'wx' });
  const receipt = join(work, 'receipt.json'), script = resolve('scripts/areas/trust_receiver.py');
  const receiver = (command, extra = []) => run('docker', ['run', '--rm', '--pull=never', '--network', 'none',
    '--user', `${process.getuid()}:${process.getgid()}`, '--mount', `type=bind,source=${socket},target=/receiver-socket`,
    '--mount', `type=bind,source=${bundle},target=/bundle,readonly`, '--mount', `type=bind,source=${work},target=/work`,
    '--mount', `type=bind,source=${script},target=/trust_receiver.py,readonly`, RUNNER, 'python3', '/trust_receiver.py', command,
    '--bundle', '/bundle', '--manifest-sha256', manifestSha, '--target', '/work/target.json', '--target-sha256', hash(targetBytes), ...extra]);
  const inspected = receiver('inspect');
  assert.equal(inspected.status, 0, inspected.stderr.toString().trim());
  assert.equal(JSON.parse(inspected.stdout).fresh, true);
  const restored = receiver('restore', ['--receipt', '/work/receipt.json', '--execute', '--writers-stopped', '--fresh-database']);
  assert.equal(restored.status, 0, restored.stderr.toString().trim()); const result = JSON.parse(restored.stdout);
  assert.equal(result.status, 'restored_verified_inactive_not_activated');
  assert.equal(result.verified_rows, manifest.result.verified_rows);
  assert.equal(result.projected_sources, manifest.result.projected_sources);
  const receiptBefore = await readFile(receipt); assert.deepEqual(JSON.parse(receiptBefore), result);
  assert.equal((await stat(receipt)).mode & 0o777, 0o600);
  const replay = receiver('restore', ['--receipt', '/work/replay.json', '--execute', '--writers-stopped', '--fresh-database']);
  assert.notEqual(replay.status, 0); assert.equal((await readFile(receipt)).compare(receiptBefore), 0);
  const after = receiver('inspect'); assert.equal(after.status, 0); assert.equal(JSON.parse(after.stdout).fresh, false);
});
