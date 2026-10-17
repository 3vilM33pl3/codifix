#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Explicit preview-only coordinated backup. Never discovers additional writers.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { pipeline } from 'node:stream/promises';
import { pathToFileURL } from 'node:url';

const check = (v, code) => { if (!v) throw Error(code); };
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const exactKeys = (v, keys) => check(v && JSON.stringify(Object.keys(v).sort()) === JSON.stringify(keys.sort()), 'invalid_contract');
const uid = () => process.getuid();
function privateDir(file) {
  const s = fs.lstatSync(file);
  check(s.isDirectory() && s.uid === uid() && !(s.mode & 0o077) && fs.realpathSync(file) === file, 'private_directory_required');
}
function read(file) {
  privateDir(path.dirname(file));
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  try { const s = fs.fstatSync(fd); check(s.isFile() && s.uid === uid() && s.nlink === 1 && !(s.mode & 0o077) && s.size <= 1048576, 'private_file_required'); return fs.readFileSync(fd); }
  finally { fs.closeSync(fd); }
}
function syncDirectory(file) {
  const directory = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_DIRECTORY);
  try { fs.fsyncSync(directory); } finally { fs.closeSync(directory); }
}
function write(file, value) {
  privateDir(path.dirname(file));
  const fd = fs.openSync(file, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_NOFOLLOW, 0o600);
  try { fs.writeFileSync(fd, JSON.stringify(value, null, 2) + '\n'); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  syncDirectory(path.dirname(file));
}
function run(command, args, options = {}) {
  const r = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 1048576, ...options });
  check(r.status === 0, `${path.basename(command)}_failed_private_diagnostics_suppressed`); return r.stdout;
}
const inspect = c => { const x = JSON.parse(run('docker', ['inspect', c.id]))[0]; check(x.Id === c.id && x.Name === `/${c.name}`, 'container_binding_changed'); return x; };
const sql = (db, text) => run('docker', ['exec', db.id, 'psql', '-XAt', '-v', 'ON_ERROR_STOP=1', '-U', db.user, '-d', db.database, '-c', text]).trim();
export function validate(c) {
  exactKeys(c, ['schema_version', 'base', 'directory', 'nas_directory', 'nas_mount', 'recipient', 'writers', 'databases', 'worker', 'archive_paths', ...(Object.hasOwn(c, 'maintenance_worker') ? ['maintenance_worker'] : [])]);
  check(c.schema_version === 1 && /^[A-F0-9]{40}$/.test(c.recipient), 'invalid_contract');
  for (const p of [c.base, c.directory, c.nas_directory]) check(path.isAbsolute(p) && path.normalize(p) === p && p !== '/', 'absolute_paths_required');
  check(path.dirname(c.directory) === c.base && /^dependable-backup-[a-z0-9-]+$/.test(path.basename(c.directory)), 'dedicated_backup_required');
  check(c.worker === 'codifix-workshop-question-worker.service', 'unexpected_worker');
  check(c.maintenance_worker === undefined || c.maintenance_worker === 'codifix-pds-refresh-only.service', 'unexpected_maintenance_worker');
  check(Array.isArray(c.writers) && c.writers.length === 4 && Array.isArray(c.databases) && c.databases.length === 3, 'exact_preview_topology_required');
  const ids = new Set();
  for (const [index, container] of [...c.writers, ...c.databases].entries()) {
    exactKeys(container, index < 4 ? ['id', 'name'] : ['id', 'name', 'user', 'database', 'label']);
    check(/^[a-f0-9]{64}$/.test(container.id) && !ids.has(container.id) && /^codifix-(?:workshop-preview-|homelab-preview-|company-)/.test(container.name), 'invalid_preview_container'); ids.add(container.id);
    if (index >= 4) check(['workshop', 'homelab', 'gateway'].includes(container.label) && /^[a-z_][a-z_0-9]*$/.test(container.user) && /^[a-z_][a-z_0-9]*$/.test(container.database), 'invalid_database');
  }
  check(new Set(c.databases.map(d => d.label)).size === 3, 'duplicate_database_label');
  check(Array.isArray(c.archive_paths) && c.archive_paths.length > 0 && c.archive_paths.every(p => typeof p === 'string' && /^(codifix-company-20260919|codifix-workshop-preview-20260918|codifix-homelab-preview-20260919)\//.test(p) && !p.split('/').some(s => ['', '.', '..'].includes(s))), 'invalid_archive_path');
  return c;
}
function nas(c) {
  check(run('findmnt', ['-rn', '-t', 'cifs', '-o', 'TARGET,SOURCE,FSTYPE', '-T', c.nas_directory]).trim() === c.nas_mount, 'nas_mount_changed');
  const s = fs.lstatSync(c.nas_directory); check(s.isDirectory() && s.uid === uid() && !(s.mode & 0o002) && fs.realpathSync(c.nas_directory) === c.nas_directory, 'unsafe_nas_directory');
}
async function digestFile(file) { const h = createHash('sha256'); for await (const chunk of fs.createReadStream(file)) h.update(chunk); return h.digest('hex'); }
async function pipe(command, args, destination) {
  const child = spawn(command, args, { stdio: ['ignore', 'pipe', 'ignore'] });
  const done = new Promise((resolve, reject) => { child.once('error', () => reject(Error('backup_process_start_failed'))); child.once('close', n => n === 0 ? resolve() : reject(Error('backup_process_failed'))); });
  try { await Promise.all([pipeline(child.stdout, destination), done]); } catch (e) { child.kill('SIGTERM'); await done.catch(() => {}); throw e; }
}
// Linux flock is attached to the open file description shared with the child.
// Closing the parent's descriptor (including process death) releases the lock;
// no timestamp/PID guesswork or automatic stale-directory deletion is needed.
export function operationLock(file) {
  privateDir(path.dirname(file));
  check(!fs.existsSync(file) || !fs.lstatSync(file).isDirectory(), 'legacy_lock_requires_operator_review');
  const fd = fs.openSync(file, fs.constants.O_RDWR | fs.constants.O_CREAT | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK, 0o600);
  try {
    const s = fs.fstatSync(fd);
    check(s.isFile() && s.uid === uid() && s.nlink === 1 && !(s.mode & 0o077), 'private_lock_required');
    const result = spawnSync('flock', ['--exclusive', '--nonblock', '3'], { stdio: ['ignore', 'ignore', 'ignore', fd], timeout: 5000 });
    check(result.status === 0, 'backup_operation_locked_or_unavailable');
    return () => fs.closeSync(fd);
  } catch (error) { fs.closeSync(fd); throw error; }
}
function boundIntent(c, review) {
  const intent = JSON.parse(read(`${c.directory}/intent.json`));
  check(intent.review === review && JSON.stringify(intent.writers) === JSON.stringify(c.writers)
    && (intent.maintenance_worker ?? null) === (c.maintenance_worker ?? null)
    && typeof intent.worker_was_active === 'boolean' && Number.isFinite(Date.parse(intent.created_at)), 'shutdown_intent_binding_changed');
  return intent;
}
export async function execute(c, phase, approval, effects = {}) {
  const io = { run, inspect, sql, nas, pipe, digestFile, ...effects }; // trusted test injection, never configuration data
  validate(c); privateDir(c.base);
  if (phase !== 'recover-shutdown') io.nas(c);
  const review = hash(JSON.stringify(c));
  if (phase === 'plan') return { review, writers: c.writers, directory: c.directory, paid_calls: 0, original_writers_affected: false };
  check(['preserve', 'resume', 'recover-shutdown'].includes(phase) && approval === review, 'exact_review_required');
  const release = operationLock(`${c.directory}.lock`);
  try {
    if (phase === 'preserve') {
      check(!fs.existsSync(c.directory) && !fs.existsSync(`${c.directory}.tar.gpg`), 'backup_exists_reconcile');
      for (const container of [...c.writers, ...c.databases]) check(io.inspect(container).State.Running, 'preview_not_running');
      const worker = io.run('systemctl', ['--user', 'show', '-p', 'ActiveState', '--value', c.worker]).trim();
      check(['active', 'inactive', 'failed'].includes(worker), 'worker_transition_in_progress');
      const maintenance = io.run('systemctl', ['--user', 'show', '-p', 'ActiveState', '--value', 'codifix-pds-refresh-only.service']).trim();
      check(['active', 'inactive', 'failed'].includes(maintenance), 'maintenance_state_unavailable');
      check(maintenance !== 'active' || c.maintenance_worker, 'active_maintenance_must_be_explicitly_quiesced');
      fs.mkdirSync(c.directory, { mode: 0o700 });
      syncDirectory(c.base);
      write(`${c.directory}/intent.json`, { review, created_at: new Date().toISOString(), worker_was_active: worker === 'active', writers: c.writers,
        maintenance_worker: c.maintenance_worker ?? null, maintenance_was_active: maintenance === 'active' });
      io.run('systemctl', ['--user', 'stop', c.worker]);
      if (c.maintenance_worker) io.run('systemctl', ['--user', 'stop', c.maintenance_worker]);
      for (const container of c.writers) io.run('docker', ['stop', '--time', '30', container.id]);
      for (const db of c.databases) {
        check(io.sql(db, 'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()') === '0', 'unexpected_database_writer');
        if (db.label !== 'gateway') {
          check(io.sql(db, "SELECT count(*) FROM supervisor_attempt_states WHERE state IN ('reserved','in_flight','uncertain')") === '0', 'unsettled_model_attempt');
        }
        for (const [suffix, cmd, args] of [['dump', 'pg_dump', ['-d', db.database, '-Fc']], ['roles.sql', 'pg_dumpall', ['-l', db.database, '--globals-only']]]) {
          await io.pipe('docker', ['exec', db.id, cmd, '-U', db.user, ...args], fs.createWriteStream(`${c.directory}/${db.label}.${suffix}`, { flags: 'wx', mode: 0o600 }));
        }
      }
      const includes = [...c.archive_paths, ...['intent.json', 'workshop.dump', 'homelab.dump', 'gateway.dump', 'workshop.roles.sql', 'homelab.roles.sql', 'gateway.roles.sql'].map(n => `${path.basename(c.directory)}/${n}`)];
      const gpg = spawn('gpg', ['--batch', '--no-tty', '--encrypt', '--recipient', c.recipient], { stdio: ['pipe', 'pipe', 'ignore'] });
      const complete = new Promise((resolve, reject) => { gpg.once('error', () => reject(Error('encryption_start_failed'))); gpg.once('close', n => n === 0 ? resolve() : reject(Error('encryption_failed'))); });
      try { await Promise.all([pipe('tar', ['-C', c.base, '-cf', '-', ...includes], gpg.stdin), pipeline(gpg.stdout, fs.createWriteStream(`${c.directory}.tar.gpg`, { flags: 'wx', mode: 0o600 })), complete]); }
      catch(e) { gpg.kill('SIGTERM'); await complete.catch(() => {}); throw e; }
      const sha256 = await digestFile(`${c.directory}.tar.gpg`), copy = path.join(c.nas_directory, `${path.basename(c.directory)}.tar.gpg`);
      io.nas(c); fs.copyFileSync(`${c.directory}.tar.gpg`, copy, fs.constants.COPYFILE_EXCL);
      check(await digestFile(copy) === sha256, 'nas_copy_mismatch');
      write(`${c.directory}/receipt.json`, { review, sha256, copy, completed_at: new Date().toISOString(), restored: false, original_writers_affected: false, paid_calls: 0 });
      return { review, phase, sha256, nas_copy_verified: true, writers_stopped: true, restored: false };
    }
    boundIntent(c, review);
    if (phase === 'recover-shutdown') {
      // Restoring availability is not recovery of a database or proof of backup.
      // Exact intent/IDs only; preserve partial artifacts and never start either
      // the paid worker or credential maintenance after an interrupted backup.
      const receipt = `${c.directory}/shutdown-recovered.json`;
      if (fs.existsSync(receipt)) check(JSON.parse(read(receipt)).review === review, 'recovery_receipt_binding_changed');
      for (const db of c.databases) check(io.inspect(db).State.Running, 'database_recovery_requires_operator_review');
      for (const container of c.writers) io.inspect(container);
      for (const container of [...c.writers].reverse()) io.run('docker', ['start', container.id]);
      if (!fs.existsSync(receipt)) write(receipt, { review, completed_at: new Date().toISOString(), backup_verified: false, paid_worker_resumed: false, maintenance_worker_resumed: false, health_verified: false });
      return { phase, review, backup_verified: false, paid_worker_resumed: false, maintenance_worker_resumed: false, health_verified: false };
    }
    const receipt = JSON.parse(read(`${c.directory}/receipt.json`));
    check(receipt.copy === path.join(c.nas_directory, `${path.basename(c.directory)}.tar.gpg`) && receipt.review === review && receipt.sha256 === await io.digestFile(`${c.directory}.tar.gpg`) && receipt.sha256 === await io.digestFile(receipt.copy), 'backup_verification_required');
    // Reverse shutdown order: PDS, gateway, then independent apps. Never invokes
    // the model worker; its explicit resume requires a separate empty-work check.
    for (const container of [...c.writers].reverse()) { io.inspect(container); io.run('docker', ['start', container.id]); }
    write(`${c.directory}/resumed.json`, { review, completed_at: new Date().toISOString(), paid_worker_resumed: false, maintenance_worker_resumed: false, health_verified: false });
    return { phase, review, paid_worker_resumed: false, maintenance_worker_resumed: false, health_verified: false };
  } finally { release(); }
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  process.umask(0o077);
  const [file, phase = 'plan', approval] = process.argv.slice(2);
  Promise.resolve().then(() => execute(JSON.parse(read(file)), phase, approval)).then(r => console.log(JSON.stringify(r))).catch(() => {
    console.error('Preview backup incomplete: inspect protected intent/receipt and exact container states. Explicit recover-shutdown restores only intent-bound containers, not backup validity. No automatic rollback, cleanup or worker resume.'); process.exitCode = 1;
  });
}
