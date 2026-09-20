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
import { FULL_REVIEW_SQL } from './classify.mjs';
import { consumePack } from './review-ledger.mjs';

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
    // The image's initialization server briefly accepts connections before it
    // shuts down. Wait for PostgreSQL to replace the entrypoint as PID 1 too.
    const main = spawnSync('docker', ['exec', container, 'cat', '/proc/1/comm'], { encoding: 'utf8' });
    if (main.stdout?.trim() === 'postgres'
      && spawnSync('docker', ['exec', container, 'pg_isready', '-U', 'postgres'], { stdio: 'ignore' }).status === 0) { ready = true; break; }
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
  // Full-history SQL/projection rehearsal on synthetic local rows only. These
  // are added to the fixture source, never to the verified restore target.
  const uid = n => `00000000-0000-0000-0000-${String(n).padStart(12, '0')}`;
  const knowledge = `CREATE TABLE memory_entries(id uuid PRIMARY KEY,project_id uuid REFERENCES projects(id),canonical_id uuid,version_no integer,is_tombstone boolean,summary text,canonical_text text,memory_type text,scope text,created_at timestamptz,updated_at timestamptz);
CREATE TABLE memory_canonical_state(canonical_id uuid PRIMARY KEY,project_id uuid REFERENCES projects(id),status text);
CREATE TABLE sessions(id uuid PRIMARY KEY,project_id uuid REFERENCES projects(id),writer_name text,writer_id text,body text);
CREATE TABLE tasks(id uuid PRIMARY KEY,session_id uuid REFERENCES sessions(id),title text,body text);
CREATE TABLE raw_captures(id uuid PRIMARY KEY,task_id uuid REFERENCES tasks(id),payload_json jsonb);
CREATE TABLE memory_sources(id uuid PRIMARY KEY,memory_entry_id uuid REFERENCES memory_entries(id),task_id uuid REFERENCES tasks(id),target_memory_id uuid,file_path text,git_commit text,source_kind text,excerpt text,content_hash text);
CREATE TABLE memory_tags(memory_entry_id uuid REFERENCES memory_entries(id),tag text,PRIMARY KEY(memory_entry_id,tag));
CREATE TABLE memory_source_checks(id bigint PRIMARY KEY,source_id uuid REFERENCES memory_sources(id),reason text);
CREATE TABLE memory_relations(id uuid PRIMARY KEY,src_canonical_id uuid,dst_canonical_id uuid,relation_type text);
INSERT INTO sessions VALUES ('${uid(40)}','${uid(1)}','fixture','fixture','NEVER EXPORT SESSION BODY');
INSERT INTO tasks VALUES ('${uid(41)}','${uid(40)}','PRIVATE TASK TITLE','NEVER EXPORT TASK BODY');
INSERT INTO raw_captures VALUES ('${uid(42)}','${uid(41)}','{"text":"NEVER EXPORT CAPTURE BODY"}');
INSERT INTO memory_entries VALUES ('${uid(11)}','${uid(1)}','${uid(10)}',1,false,'First','First synthetic retained version.','implementation','project',now(),now()),
('${uid(12)}','${uid(1)}','${uid(10)}',2,true,'','', 'implementation','project',now(),now()),
('${uid(21)}','${uid(1)}','${uid(20)}',1,false,'Second','Second synthetic retained history.','implementation','project',now(),now());
INSERT INTO memory_canonical_state VALUES ('${uid(10)}','${uid(1)}','active'),('${uid(20)}','${uid(1)}','active');
INSERT INTO memory_sources VALUES ('${uid(50)}','${uid(11)}','${uid(41)}',null,'/private/source/file.md',null,'file','NEVER EXPORT EVIDENCE BODY',null);
INSERT INTO memory_tags VALUES ('${uid(11)}','tag');
INSERT INTO memory_source_checks VALUES (1,'${uid(50)}','synthetic');
INSERT INTO memory_relations VALUES ('${uid(60)}','${uid(10)}','${uid(20)}','relates_to');`;
  run('docker', ['exec', '-i', container, 'psql', '-X', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'memory_fixture_source'], { input: knowledge });
  const full = run('docker', ['exec', '-i', container, 'psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'memory_fixture_source'], { input: FULL_REVIEW_SQL, encoding: 'utf8' });
  const chunks = [];
  const index = await consumePack((async function* () { for (const line of full.trim().split('\n')) yield line; })(), {
    database: 'memory_fixture_source', canonical_ids: new Set([uid(10), uid(20)]), expected_versions: 3, expected_projects: 1, evidence: {},
  }, async line => chunks.push(line));
  assert.equal(index.histories.length, 2); assert.equal(index.versions, 3);
  assert.ok(index.histories.every(h => h.relation_count === 1));
  assert.equal(index.relations.length, 1);
  assert.ok(index.histories[0].dependency_row_count > index.histories[0].projection_dependencies.length);
  assert.ok(index.histories[0].projection_dependencies.some(d => d.table === 'public.tasks'));
  assert.ok(!chunks.join('').includes('NEVER EXPORT'));
  assert.ok(!chunks.join('').includes('/private/'));
  // Mutating only the restored sequence must fail independent verification, even
  // though the table-row multiset is still identical to the encrypted snapshot.
  run('docker', ['exec', container, 'psql', '-X', '-U', 'postgres', '-d', 'memory_restore_fixture', '-c', "SELECT setval('public.example_id_seq',1,true)"]);
  const mismatch = spawnSync(process.execPath, [...auditArgs, '--verify-existing'], { env, encoding: 'utf8', timeout: 120_000 });
  assert.equal(mismatch.status, 1); assert.match(mismatch.stderr, /sequence state differ/);
});
