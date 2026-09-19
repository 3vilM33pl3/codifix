// SPDX-License-Identifier: AGPL-3.0-or-later
// Opt-in synthetic, network-none end-to-end rehearsal. No live source or keys.
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const scripts = fileURLToPath(new URL('.', import.meta.url));
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { maxBuffer: 16 * 1024 * 1024, timeout: 120_000, ...options });
  if (result.status !== 0) throw Error(`Synthetic ${command} command failed; output suppressed`);
  return result.stdout;
}
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

test('real encrypted archive, sequence restore and descriptor-bound collectors stay isolated', {
  skip: process.env.MEMORY_AREA_TEST_DOCKER !== '1', timeout: 180_000,
}, async t => {
  const root = await mkdtemp(join(tmpdir(), 'memory-restore-rehearsal-'));
  const gnupg = join(root, 'gnupg'); await mkdir(gnupg, { mode: 0o700 });
  const env = { ...process.env, GNUPGHOME: gnupg };
  t.after(async () => {
    spawnSync('gpgconf', ['--homedir', gnupg, '--kill', 'gpg-agent'], { stdio: 'ignore' });
    await rm(root, { recursive: true, force: true });
  });
  // Resolve an already installed image to an immutable ID; never pull an image.
  const image = run('docker', ['image', 'inspect', 'pgvector/pgvector:pg17', '--format', '{{.Id}}'], { encoding: 'utf8' }).trim();
  assert.match(image, /^sha256:[0-9a-f]{64}$/);
  const name = `memory-four-area-restore-test-${randomUUID()}`;
  const container = run('docker', ['run', '--pull=never', '--detach', '--rm', '--name', name, '--network', 'none', '--read-only',
    '--tmpfs', '/var/lib/postgresql/data:rw,size=256m', '--tmpfs', '/var/run/postgresql:rw', '--tmpfs', '/tmp:rw',
    '--env', 'POSTGRES_HOST_AUTH_METHOD=trust', image], { encoding: 'utf8' }).trim();
  assert.match(container, /^[0-9a-f]{64}$/);
  t.after(() => run('docker', ['rm', '--force', container]));
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (spawnSync('docker', ['exec', container, 'pg_isready', '-U', 'postgres'], { stdio: 'ignore' }).status === 0) { ready = true; break; }
    await delay(100);
  }
  assert.equal(ready, true);
  for (const database of ['memory_fixture_source', 'memory_restore_fixture']) run('docker', ['exec', container, 'createdb', '-U', 'postgres', database]);
  const sql = `CREATE TABLE projects(id uuid PRIMARY KEY,slug text,name text,root_path text,created_at timestamptz);
CREATE TABLE _sqlx_migrations(version bigint PRIMARY KEY,success boolean);
INSERT INTO _sqlx_migrations VALUES (52,true);
INSERT INTO projects VALUES ('00000000-0000-0000-0000-000000000001','synthetic','Synthetic fixture','/synthetic','2026-09-19T12:00:00Z');
CREATE TABLE example(id bigserial PRIMARY KEY,value text);
INSERT INTO example(value) VALUES ('synthetic only');
SELECT setval('public.example_id_seq',9007199254740993,false);
CREATE SCHEMA history;
CREATE TABLE history.example(id bigserial PRIMARY KEY,value text);
INSERT INTO history.example(value) VALUES ('historical synthetic only');
SELECT setval('history.example_id_seq',19,true);`;
  run('docker', ['exec', '-i', container, 'psql', '-X', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'memory_fixture_source'], { input: sql });
  const dump = run('docker', ['exec', '--env', 'PGOPTIONS=-c timezone=UTC', container, 'pg_dump', '-U', 'postgres', '-d', 'memory_fixture_source', '--format=custom', '--no-owner', '--no-privileges']);
  run('gpg', ['--batch', '--pinentry-mode', 'loopback', '--passphrase', '', '--quick-generate-key', 'Memory Restore Fixture <synthetic@invalid.test>', 'default', 'default', '0'], { env });
  const listing = run('gpg', ['--batch', '--with-colons', '--list-secret-keys'], { env, encoding: 'utf8' });
  const fingerprint = listing.split('\n').find(line => line.startsWith('fpr:')).split(':')[9];
  const encrypted = run('gpg', ['--batch', '--trust-model', 'always', '--encrypt', '--recipient', fingerprint, '--output', '-'], { env, input: dump });
  await writeFile(join(root, 'database.pgdump.gpg'), encrypted, { mode: 0o600, flag: 'wx' });
  await writeFile(join(root, 'snapshot.json'), JSON.stringify({ schema_version: 1, database: 'memory_fixture_source', database_session_timezone: 'UTC',
    artifacts: [{ name: 'database.pgdump.gpg', sha256: hash(encrypted) }] }), { mode: 0o600, flag: 'wx' });
  const source = randomUUID(), snapshot = randomUUID();
  const auditArgs = [join(scripts, 'restore-audit.mjs'), root, name, '--database', 'memory_restore_fixture'];
  const result = JSON.parse(run(process.execPath, [...auditArgs, '--source-instance', source, '--snapshot-id', snapshot], { env, encoding: 'utf8' }));
  assert.equal(result.status, 'database_restore_verified'); assert.equal(result.sequences, 2); assert.equal(result.descriptor_written, true);
  const evidence = JSON.parse(await readFile(join(root, 'restore-evidence.json'), 'utf8'));
  assert.deepEqual(evidence.restored_sequence_inventory, [
    { sequence: 'history.example_id_seq', last_value: '19', is_called: true },
    { sequence: 'public.example_id_seq', last_value: '9007199254740993', is_called: false },
  ]);
  const descriptor = join(root, 'restore-descriptor.json'), inventory = join(root, 'inventory.json');
  const collected = JSON.parse(run(process.execPath, [join(scripts, 'inventory.mjs'), '--descriptor', descriptor, '--output', inventory], { encoding: 'utf8' }));
  assert.equal(collected.table_count, 4); assert.equal(collected.row_count, 4);
  const content = JSON.parse(await readFile(inventory, 'utf8'));
  assert.equal(content.source_instance, source); assert.equal(content.snapshot_id, snapshot);
  const payload = JSON.parse(run(process.execPath, [join(scripts, 'payload.mjs'), '--descriptor', descriptor,
    '--inventory-sha256', 'a'.repeat(64), '--output', join(root, 'payload.ndjson')], { encoding: 'utf8' }));
  assert.equal(payload.row_count, 4); assert.equal(payload.inventory_verified, false);
  // Mutating only the restored sequence must fail independent verification, even
  // though the table-row multiset is still identical to the encrypted snapshot.
  run('docker', ['exec', container, 'psql', '-X', '-U', 'postgres', '-d', 'memory_restore_fixture', '-c', "SELECT setval('public.example_id_seq',1,true)"]);
  const mismatch = spawnSync(process.execPath, [...auditArgs, '--verify-existing'], { env, encoding: 'utf8', timeout: 120_000 });
  assert.equal(mismatch.status, 1); assert.match(mismatch.stderr, /sequence state differ/);
});
