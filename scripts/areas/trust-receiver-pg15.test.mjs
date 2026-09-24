// SPDX-License-Identifier: AGPL-3.0-or-later
// Opt-in exact cross-major receiver proof: PostgreSQL 17 source data rendered to
// an allowlisted SQL stream and restored with trusted PostgreSQL 15.18 schema.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash, randomUUID } from 'node:crypto';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { SCHEMA60_SEQUENCES, SCHEMA60_TABLES } from './verify-staging.mjs';
import { validateTrustDataToc } from './trust-transfer-toc.mjs';
import { canonicalTrustDataSql, freshTrustReceiverGuard } from './trust-receiver-data.mjs';
import { validateCaObservedPostgres15 } from './trust-receiver-profile.mjs';
import { TRUST_SCHEMA } from './trust-expectation.mjs';

const PG17 = 'sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180';
const PG15 = 'sha256:e2426fe23642a1e4ddeb6126bb7e4295cfd0b42d35dbb10233d6d31c00e8c9b7';
const clean = { PATH: '/usr/bin:/bin', LC_ALL: 'C', LANG: 'C', TZ: 'UTC' };
const hash = value => createHash('sha256').update(value).digest('hex');
const id = value => `00000000-0000-0000-0000-${String(value).padStart(12, '0')}`;

test('PostgreSQL 17 Trust data restores atomically into the exact CA PostgreSQL 15.18 profile', {
  skip: process.env.MEMORY_AREA_TEST_PG15 !== '1', timeout: 300_000,
}, async t => {
  const root = await mkdtemp(join(tmpdir(), 'memory-trust-pg15-receiver-'));
  const sourceSocket = join(root, 'source-socket'), targetSocket = join(root, 'target-socket');
  await mkdir(sourceSocket, { mode: 0o777 }); await mkdir(targetSocket, { mode: 0o777 });
  const run = (command, args, options = {}) => {
    const result = spawnSync(command, args, { env: clean, timeout: 120_000, maxBuffer: 64 * 1024 * 1024, ...options });
    if (result.status !== 0) {
      if (process.env.MEMORY_AREA_TEST_DEBUG === '1') process.stderr.write(result.stderr ?? '');
      throw new Error(`Trust receiver ${command} operation failed; diagnostics suppressed`);
    }
    return result.stdout;
  };
  assert.equal(run('docker', ['image', 'inspect', PG15, '--format', '{{.Id}}']).toString().trim(), PG15);
  const start = (image, socket, suffix) => run('docker', ['run', '--pull=never', '--detach', '--name',
    `memory-trust-receiver-${suffix}-${randomUUID()}`, '--network', 'none', '--read-only', '--memory', '1g', '--cpus', '2',
    '--tmpfs', '/var/lib/postgresql/data:rw,size=512m', '--tmpfs', '/tmp:rw',
    '--mount', `type=bind,source=${socket},target=/var/run/postgresql`, '--env', 'POSTGRES_HOST_AUTH_METHOD=trust', image,
    '-c', 'unix_socket_directories=/var/run/postgresql']).toString().trim();
  const source = start(PG17, sourceSocket, 'source'), target = start(PG15, targetSocket, 'target');
  t.after(async () => {
    for (const container of [source, target]) {
      spawnSync('docker', ['exec', container, 'chmod', '0777', '/var/run/postgresql'], { env: clean, stdio: 'ignore', timeout: 5000 });
      run('docker', ['rm', '--force', container]);
    }
    await rm(root, { recursive: true, force: true });
  });
  const docker = (container, args, input) => run('docker', ['exec', '-i', container, 'env', '-i', 'PATH=/usr/bin:/bin',
    'LC_ALL=C', 'PGOPTIONS=-c timezone=UTC', ...args], input === undefined ? {} : { input });
  const sql = (container, database, query) => docker(container,
    ['psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', database], query).toString().trim();
  for (const container of [source, target]) {
    let ready = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      const main = spawnSync('docker', ['exec', container, 'cat', '/proc/1/comm'],
        { env: clean, encoding: 'utf8', timeout: 5000 });
      if (main.stdout?.trim() === 'postgres' && spawnSync('docker', ['exec', container, 'pg_isready', '-U', 'postgres'],
        { env: clean, stdio: 'ignore', timeout: 5000 }).status === 0) { ready = true; break; }
      await delay(100);
    }
    assert.equal(ready, true);
  }
  assert.equal(sql(source, 'postgres', 'SHOW server_version;'), '17.11 (Debian 17.11-1.pgdg12+2)');
  const observation = validateCaObservedPostgres15({
    format: 'memory-trust-ca-observed-profile-v1',
    postgres_version: sql(target, 'postgres', 'SHOW server_version;').split(' ')[0],
    client_version: docker(target, ['psql', '--version']).toString().match(/([0-9]+\.[0-9]+)/)?.[1],
    extensions: { ...Object.fromEntries(JSON.parse(sql(target, 'postgres',
      "SELECT json_agg(json_build_object('name',name,'version',default_version) ORDER BY name) FROM pg_available_extensions WHERE name IN ('pgcrypto','pg_trgm');"))
      .map(value => [value.name, value.version])), vector: null },
  });
  assert.equal(observation.postgres_version, '15.18');

  for (const [container, databases] of [[source, ['memory_staging_trust']],
    [target, ['memory_reference_trust', 'memory_trust']]]) {
    for (const database of databases) docker(container, ['createdb', '-U', 'postgres', database]);
  }
  const binary = process.env.MEMORY_AREA_IMPORT_BINARY
    ?? new URL('../../target/debug/memory-area-import', import.meta.url).pathname;
  const initialize = async (database, socket) => {
    const descriptor = join(root, `${database}.json`);
    const bytes = JSON.stringify({ format_version: 1, instance_id: id(database === 'memory_staging_trust' ? 90 : 91),
      area: 'trust', connection: { kind: 'unix', directory: socket, port: 5432 }, database, username: 'postgres', password: null });
    await writeFile(descriptor, bytes, { mode: 0o600, flag: 'wx' });
    const plan = JSON.parse(run(binary, ['schema-plan'])); assert.equal(plan.schema_sha256, TRUST_SCHEMA);
    run(binary, ['initialize-schema', '--target', descriptor, '--target-sha256', hash(bytes), '--schema-sha256', TRUST_SCHEMA,
      '--execute', '--writers-stopped', '--fresh-database']);
  };
  await initialize('memory_staging_trust', sourceSocket);
  await initialize('memory_reference_trust', targetSocket);
  sql(source, 'memory_staging_trust', `INSERT INTO projects(id,slug,name,root_path,created_at)
    VALUES('${id(1)}','trust-fixture','Synthetic Trust','trust-fixture','2026-09-24T07:00:00Z');
    INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,
      importance,confidence,created_at,updated_at,search_document)
    VALUES('${id(3)}','${id(1)}','${id(2)}',1,false,'Synthetic retained Trust version','Synthetic summary','documentation','project',
      3,0.75,'2026-09-24T07:00:00Z','2026-09-24T07:00:00Z',to_tsvector('simple','synthetic trust'));
    CREATE FUNCTION public.source_only_poison() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'POISON'; END $$;
    CREATE TRIGGER source_only_poison AFTER INSERT ON memory_entries FOR EACH ROW EXECUTE FUNCTION public.source_only_poison();`);

  const data = docker(source, ['pg_dump', '-U', 'postgres', '-d', 'memory_staging_trust', '--data-only', '--format=custom',
    '--no-owner', '--no-privileges']);
  const toc = validateTrustDataToc(run('pg_restore', ['--list'], { input: data }).toString());
  assert.equal(toc.table_count, SCHEMA60_TABLES.length); assert.equal(toc.sequence_count, SCHEMA60_SEQUENCES.length);
  const selection = join(root, 'data-selection.list'); await writeFile(selection, toc.selection_list, { mode: 0o600, flag: 'wx' });
  const rendered = run('pg_restore', ['--data-only', '--no-owner', '--no-privileges', '--file=-', `--use-list=${selection}`],
    { input: data }).toString();
  const canonicalData = canonicalTrustDataSql(rendered);
  assert.doesNotMatch(canonicalData, /source_only_poison|transaction_timeout|\\restrict/);

  const schema = docker(target, ['pg_dump', '-U', 'postgres', '-d', 'memory_reference_trust', '--schema-only', '--format=custom',
    '--no-owner', '--no-privileges']);
  const pre = docker(target, ['pg_restore', '--section=pre-data', '--no-owner', '--no-privileges', '--file=-'], schema);
  const post = docker(target, ['pg_restore', '--section=post-data', '--no-owner', '--no-privileges', '--file=-'], schema);
  const restore = Buffer.concat([Buffer.from("SET statement_timeout='120s'; SET lock_timeout='5s';\n"),
    Buffer.from(freshTrustReceiverGuard()), pre, Buffer.from(canonicalData), post]);
  docker(target, ['psql', '-X', '-q', '-1', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'memory_trust'], restore);

  const stateSql = SCHEMA60_TABLES.map(table => `SELECT json_build_object('table','${table}','count',count(*),
    'sha256',encode(public.digest(COALESCE(string_agg(to_jsonb(r)::text,E'\\n' ORDER BY to_jsonb(r)::text),''),'sha256'),'hex')) FROM ONLY ${table} r;`).join('\n')
    + '\n' + SCHEMA60_SEQUENCES.map(sequence => `SELECT json_build_object('sequence','${sequence.name}',
      'last_value',last_value::text,'is_called',is_called) FROM ${sequence.name};`).join('\n');
  assert.equal(sql(target, 'memory_trust', stateSql), sql(source, 'memory_staging_trust', stateSql));
  const installed = Object.fromEntries(JSON.parse(sql(target, 'memory_trust',
    "SELECT json_agg(json_build_object('name',extname,'version',extversion) ORDER BY extname) FROM pg_extension WHERE extname<>'plpgsql';"))
    .map(value => [value.name, value.version]));
  assert.deepEqual(installed, { pg_trgm: '1.6', pgcrypto: '1.3', vector: '0.8.6' });
  assert.equal(sql(target, 'memory_trust', "SELECT count(*) FROM pg_proc WHERE proname='source_only_poison';"), '0');
  assert.equal(sql(target, 'memory_trust', 'SELECT count(*) FROM pg_index WHERE NOT indisvalid;'), '0');
  t.diagnostic(`postgres=${observation.postgres_version}; pgvector=${installed.vector}; tables=${toc.table_count}; sequences=${toc.sequence_count}; source_ddl_executed=false`);
});
