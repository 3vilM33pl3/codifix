// SPDX-License-Identifier: AGPL-3.0-or-later
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { validate, execute, operationLock } from './preview-backup.mjs';
const fixture = () => ({ schema_version: 1, base: '/private/state', directory: '/private/state/dependable-backup-test',
  nas_directory: '/nas/encrypted', nas_mount: '/nas //example/share cifs', recipient: 'A'.repeat(40),
  worker: 'codifix-workshop-question-worker.service',
  writers: ['workshop-preview-app', 'homelab-preview-app', 'company-gateway', 'company-pds'].map((s, i) => ({ id: String(i).repeat(64), name: `codifix-${s}` })),
  databases: ['workshop', 'homelab', 'gateway'].map((label, i) => ({ id: String(i + 4).repeat(64), name: `codifix-company-${label}-db`, user: 'postgres', database: 'synthetic', label })),
  archive_paths: ['codifix-company-20260919/gateway/gateway.json'] });
test('explicit preview-only contract accepts a bounded complete topology', () => assert.equal(validate(fixture()).writers.length, 4));
test('unsafe scope, duplicate identities and path escapes are rejected', () => {
  for (const mutate of [c => { c.archive_paths = ['codifix-company-20260919/../unrelated']; }, c => { c.archive_paths = ['/etc']; },
    c => { c.writers[0].name = 'production-db'; }, c => { c.writers[0].id = c.databases[0].id; },
    c => { c.directory = '/'; }, c => { c.worker = 'unrelated.service'; }, c => { c.extra = true; },
    c => { c.maintenance_worker = 'unrelated.service'; },
    c => { c.databases[0].database = 'db; DROP DATABASE other'; }]) {
    const c = fixture(); mutate(c); assert.throws(() => validate(c));
  }
});

function sandbox(t) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'codifix-backup-test-')); fs.chmodSync(base, 0o700);
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const c = { ...fixture(), base, directory: `${base}/dependable-backup-test`, nas_directory: `${base}/nas` };
  const calls = [];
  const io = { nas: () => {}, inspect: v => ({ Id: v.id, Name: `/${v.name}`, State: { Running: true } }),
    run: (cmd, args) => { calls.push([cmd, args]); return args.includes('show') ? 'inactive' : ''; } };
  return { c, calls, io };
}
function privateJson(file, value) { fs.writeFileSync(file, JSON.stringify(value), { mode: 0o600 }); }
async function interrupted(t, maintenance = false) {
  const s = sandbox(t);
  if (maintenance) s.c.maintenance_worker = 'codifix-pds-refresh-only.service';
  const { review } = await execute(s.c, 'plan', null, s.io);
  const fail = { ...s.io, run: (cmd, args) => {
    s.calls.push([cmd, args]);
    if (cmd === 'docker' && args[0] === 'stop' && args.at(-1) === s.c.writers[1].id) throw Error('simulated_interruption');
    return args.includes('show') ? 'active' : '';
  } };
  // Without an explicitly enrolled maintainer, it is inactive in this fixture.
  if (!maintenance) {
    const run = fail.run;
    fail.run = (cmd, args) => args.includes('show') && args.at(-1) === 'codifix-pds-refresh-only.service' ? 'inactive' : run(cmd, args);
  }
  await assert.rejects(execute(s.c, 'preserve', review, fail), /simulated_interruption/);
  assert.ok(fs.existsSync(`${s.c.directory}/intent.json`));
  const preserveCalls = [...s.calls];
  s.calls.length = 0;
  return { ...s, review, preserveCalls };
}
test('interrupted shutdown recovers only exact writers, with NAS offline and no successful-backup claim', async t => {
  const { c, calls, io, review } = await interrupted(t, true);
  const intent = JSON.parse(fs.readFileSync(`${c.directory}/intent.json`));
  assert.equal(intent.maintenance_worker, 'codifix-pds-refresh-only.service');
  assert.equal(intent.maintenance_was_active, true);
  const result = await execute(c, 'recover-shutdown', review, { ...io, nas: () => { throw Error('NAS offline'); } });
  assert.equal(result.backup_verified, false);
  assert.equal(result.paid_worker_resumed, false);
  assert.equal(result.maintenance_worker_resumed, false);
  assert.equal(result.health_verified, false);
  assert.deepEqual(calls, [...c.writers].reverse().map(w => ['docker', ['start', w.id]]));
  assert.ok(fs.existsSync(`${c.directory}/shutdown-recovered.json`));
  assert.ok(!fs.existsSync(`${c.directory}/receipt.json`));
  await assert.rejects(execute(c, 'resume', review, io), /ENOENT/);
  calls.length = 0;
  await execute(c, 'recover-shutdown', review, io); // exact retry is harmless
  assert.equal(calls.length, 4);
});
test('changed approval, intent or database state cannot trigger recovery starts', async t => {
  const { c, calls, io, review } = await interrupted(t);
  await assert.rejects(execute(c, 'recover-shutdown', '0'.repeat(64), io), /exact_review_required/);
  await assert.rejects(execute(c, 'recover-shutdown', review, { ...io, inspect: v => ({ State: { Running: v.id !== c.databases[0].id } }) }), /database_recovery_requires_operator_review/);
  const file = `${c.directory}/intent.json`, intent = JSON.parse(fs.readFileSync(file));
  privateJson(file, { ...intent, writers: [...intent.writers].reverse() });
  await assert.rejects(execute(c, 'recover-shutdown', review, io), /shutdown_intent_binding_changed/);
  privateJson(file, intent);
  privateJson(`${c.directory}/shutdown-recovered.json`, { review: 'altered' });
  await assert.rejects(execute(c, 'recover-shutdown', review, io), /recovery_receipt_binding_changed/);
  assert.deepEqual(calls, []);
});
test('active maintenance requires explicit approval and is stopped before containers', async t => {
  const { c, io, calls } = sandbox(t);
  const active = { ...io, run: (cmd, args) => { calls.push([cmd, args]); return 'active'; } };
  const { review } = await execute(c, 'plan', null, io);
  await assert.rejects(execute(c, 'preserve', review, active), /active_maintenance_must_be_explicitly_quiesced/);
  assert.ok(!fs.existsSync(c.directory));
  assert.ok(calls.every(([cmd, args]) => cmd === 'systemctl' && args.includes('show')));
  const s = await interrupted(t, true);
  const intent = JSON.parse(fs.readFileSync(`${s.c.directory}/intent.json`));
  assert.equal(intent.maintenance_was_active, true);
  assert.deepEqual(s.preserveCalls.filter(([, args]) => !args.includes('show')).slice(0, 3), [
    ['systemctl', ['--user', 'stop', s.c.worker]],
    ['systemctl', ['--user', 'stop', s.c.maintenance_worker]],
    ['docker', ['stop', '--time', '30', s.c.writers[0].id]],
  ]);
});
test('kernel lock excludes competing processes and releases on abrupt process death', async t => {
  const { c } = sandbox(t), file = `${c.directory}.lock`;
  const script = `import { operationLock } from ${JSON.stringify(new URL('./preview-backup.mjs', import.meta.url).href)}; operationLock(${JSON.stringify(file)}); process.stdout.write('ready'); setInterval(() => {}, 1000);`;
  const child = spawn(process.execPath, ['--input-type=module', '-e', script], { stdio: ['ignore', 'pipe', 'pipe'] });
  t.after(() => child.kill('SIGKILL'));
  const ready = await Promise.race([once(child.stdout, 'data'), once(child, 'exit').then(() => { throw Error('lock process exited'); })]);
  assert.equal(ready[0].toString(), 'ready');
  assert.throws(() => operationLock(file), /backup_operation_locked_or_unavailable/);
  const dead = once(child, 'exit'); child.kill('SIGKILL'); await dead;
  operationLock(file)();
  assert.equal(fs.statSync(file).mode & 0o777, 0o600);
});
test('legacy lock directories require review; symlink and public lock files fail closed', t => {
  const { c } = sandbox(t), lock = `${c.directory}.lock`;
  fs.mkdirSync(lock, { mode: 0o700 });
  assert.throws(() => operationLock(lock), /legacy_lock_requires_operator_review/);
  assert.ok(fs.statSync(lock).isDirectory());
  const target = `${c.base}/target`; fs.writeFileSync(target, '', { mode: 0o600 });
  fs.symlinkSync(target, `${c.base}/symlink.lock`);
  assert.throws(() => operationLock(`${c.base}/symlink.lock`));
  fs.chmodSync(target, 0o644);
  assert.throws(() => operationLock(target), /private_lock_required/);
});
