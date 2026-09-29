#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Materialize and independently restore a private, inactive PostgreSQL-17 Trust
// receiver bundle. This does not encrypt, transfer, provision or activate it.
import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { chmod, mkdir, open } from 'node:fs/promises';
import { constants } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { privateInput } from './restore-descriptor.mjs';
import { BEGIN_SQL, streamLines } from './inventory.mjs';
import { expandTrust, TRUST_SCHEMA } from './trust-expectation.mjs';
import { validateTrustDataToc } from './trust-transfer-toc.mjs';
import { canonicalTrustDataSql, freshTrustReceiverGuard } from './trust-receiver-data.mjs';
import { splitTrustedPreData } from './trust-receiver-schema.mjs';
import { TRUST_RECEIVER_FIXTURE, TRUST_RECEIVER_PROFILE, validateTrustReceiverObservation } from './trust-receiver-profile.mjs';
import { sourceSchema, TRUST_FROZEN_SOURCE_SHA256, TRUST_SCHEMA_TOOL_SHA256 } from './trust-transfer-rehearsal.mjs';
import { SCHEMA60_SEQUENCES, SCHEMA60_TABLES, verifyDatabase } from './verify-staging.mjs';

const sha = /^[0-9a-f]{64}$/;
const clean = { PATH: '/usr/bin:/bin', LC_ALL: 'C', LANG: 'C', TZ: 'UTC' };
const hash = value => createHash('sha256').update(value).digest('hex');
function fail() { throw new Error('Trust receiver bundle preparation failed; keep private inputs local and inactive'); }
function run(command, args, input, maxBuffer = 64 * 1024 * 1024) {
  const result = spawnSync(command, args, { input, env: clean, encoding: null, timeout: 120_000, maxBuffer });
  if (result.status !== 0) fail(); return result.stdout;
}
const docker = (container, args, input) => run('docker', ['exec', '-i', container, 'env', '-i', 'PATH=/usr/bin:/bin',
  'LC_ALL=C', 'PGOPTIONS=-c timezone=UTC', ...args], input);
const sql = (container, database, query) => docker(container,
  ['psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-p', String(TRUST_RECEIVER_PROFILE.database_port),
    '-U', 'postgres', '-d', database], query).toString().trim();
async function artifact(path, bytes, executable = false) {
  const file = await open(path, 'wx', executable ? 0o700 : 0o600);
  try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
  if (executable) await chmod(path, 0o700);
  return { name: path.split('/').at(-1), sha256: hash(bytes), bytes: Buffer.byteLength(bytes) };
}
async function bound(path, digest, limit, json = false) {
  if (!isAbsolute(path) || !sha.test(digest)) fail();
  const input = await privateInput(path, limit, json); if (input.sha256 !== digest) fail(); return input.value;
}
async function readBound(source, digest, limit) {
  await bound(source, digest, limit);
  const file = await open(source, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const stat = await file.stat(); if (!stat.isFile() || stat.uid !== process.getuid() || (stat.mode & 0o077) || stat.nlink !== 1 || stat.size > limit) fail();
    const bytes = await file.readFile(); if (hash(bytes) !== digest) fail(); return bytes;
  } finally { await file.close(); }
}
async function copyBound(source, destination, digest, limit, executable = false) {
  return artifact(destination, await readBound(source, digest, limit), executable);
}
function containerSession(container, database) {
  const child = spawn('docker', ['exec', '-i', container, 'env', '-i', 'PATH=/usr/bin:/bin', 'psql', '-X', '-qAt',
    '-v', 'ON_ERROR_STOP=1', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres', '-d', database],
  { stdio: ['pipe', 'pipe', 'pipe'], env: clean });
  child.stderr.resume(); child.stdin.on('error', () => {});
  const done = new Promise(resolveDone => { child.once('error', () => resolveDone(-1)); child.once('close', resolveDone); });
  return { lines: streamLines(child.stdout), done, send(value) { child.stdin.write(value); },
    abort() { child.stdin.destroy(); child.kill('SIGTERM'); child.stdout.destroy(); } };
}

export async function prepareReceiverBundle(options) {
  for (const name of ['data', 'selection', 'expectation', 'schema-tool', 'source-archive', 'output'])
    if (!isAbsolute(options[name] ?? '')) fail();
  for (const name of ['data-sha256', 'selection-sha256', 'expectation-sha256']) if (!sha.test(options[name] ?? '')) fail();
  await mkdir(options.output, { mode: 0o700 });
  const bundle = join(options.output, 'bundle'); await mkdir(bundle, { mode: 0o700 });
  const expectation = await bound(options.expectation, options['expectation-sha256'], 32 * 1024 * 1024, true);
  // Historical receiver v4 remains pinned to the exact schema60 tool/source.
  // Existing Trust upgrades use reconciliation, never a replacement restore.
  if (expectation.format !== 'memory-trust-expectation-v1') fail();
  const state = expandTrust(expectation);
  const data = await readBound(options.data, options['data-sha256'], 32 * 1024 * 1024);
  const selection = await readBound(options.selection, options['selection-sha256'], 256 * 1024);
  const toc = validateTrustDataToc(run('pg_restore', ['--list'], data).toString());
  if (selection.toString() !== toc.selection_list) fail();
  const rendered = run('pg_restore', ['--data-only', '--no-owner', '--no-privileges', '--file=-',
    `--use-list=${options.selection}`], data).toString();
  const canonicalData = canonicalTrustDataSql(rendered);
  const schemaTool = join(options.output, 'schema-tool');
  await copyBound(options['schema-tool'], schemaTool, TRUST_SCHEMA_TOOL_SHA256, 32 * 1024 * 1024, true);
  await copyBound(options['source-archive'], join(options.output, 'frozen-source.tar'), TRUST_FROZEN_SOURCE_SHA256, 512 * 1024 * 1024);
  const plan = JSON.parse(run(schemaTool, ['schema-plan']));
  if (plan.schema_sha256 !== TRUST_SCHEMA || plan.migration_head !== 60) fail();
  const migrationChecksums = await sourceSchema(join(options.output, 'frozen-source.tar'), options.output);

  const token = randomUUID(), socket = join(options.output, 'socket'); await mkdir(socket, { mode: 0o700 });
  const name = `memory-trust-pg17-receiver-${token}`;
  const container = run('docker', ['run', '--pull=never', '--detach', '--name', name, '--network', 'none', '--read-only',
    '--memory', '1g', '--cpus', '2', '--tmpfs', '/var/lib/postgresql/data:rw,size=512m', '--tmpfs', '/tmp:rw',
    '--mount', `type=bind,source=${socket},target=/var/run/postgresql`, '--env', 'POSTGRES_HOST_AUTH_METHOD=trust',
    TRUST_RECEIVER_FIXTURE.image_id, '-c', 'unix_socket_directories=/var/run/postgresql',
    '-c', `port=${TRUST_RECEIVER_PROFILE.database_port}`]).toString().trim();
  let passed = false, phase = 'startup';
  try {
    let ready = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      const main = spawnSync('docker', ['exec', container, 'cat', '/proc/1/comm'], { env: clean, encoding: 'utf8', timeout: 5000 });
      if (main.stdout?.trim() === 'postgres' && spawnSync('docker', ['exec', container, 'pg_isready', '-p',
        String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres'],
        { env: clean, stdio: 'ignore', timeout: 5000 }).status === 0) { ready = true; break; }
      await delay(100);
    }
    if (!ready) fail();
    phase = 'role-and-database-preparation';
    docker(container, ['createuser', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres', '--login', TRUST_RECEIVER_PROFILE.role]);
    docker(container, ['createdb', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres', 'memory_reference_trust']);
    docker(container, ['createdb', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres',
      '--owner', TRUST_RECEIVER_PROFILE.role, 'memory_trust']);
    phase = 'trusted-reference-schema';
    const target = { format_version: 1, instance_id: expectation.target_instance, area: 'trust',
      connection: { kind: 'unix', directory: socket, port: TRUST_RECEIVER_PROFILE.database_port },
      database: 'memory_reference_trust', username: 'postgres', password: null };
    const targetBytes = Buffer.from(JSON.stringify(target));
    const targetPath = join(options.output, 'reference-target.json'); await artifact(targetPath, targetBytes);
    const initialized = JSON.parse(run(schemaTool, ['initialize-schema', '--target', targetPath, '--target-sha256', hash(targetBytes),
      '--schema-sha256', TRUST_SCHEMA, '--execute', '--writers-stopped', '--fresh-database']));
    if (initialized.status !== 'fresh_schema_initialized_only_not_imported_or_activated') fail();
    phase = 'receiver-observation';
    const observation = validateTrustReceiverObservation({
      format: 'memory-trust-receiver-observation-v1',
      postgres_version: sql(container, 'postgres', 'SHOW server_version;').split(' ')[0],
      client_version: docker(container, ['psql', '--version']).toString().match(/([0-9]+\.[0-9]+)/)?.[1],
      extensions: Object.fromEntries(JSON.parse(sql(container, 'postgres',
        "SELECT json_agg(json_build_object('name',name,'version',default_version) ORDER BY name) FROM pg_available_extensions WHERE name IN ('vector','pgcrypto','pg_trgm');"))
        .map(value => [value.name, value.version])),
    });
    phase = 'trusted-schema-render';
    const schema = docker(container, ['pg_dump', '-p', String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres',
      '-d', 'memory_reference_trust', '--schema-only',
      '--format=custom', '--no-owner', '--no-privileges']);
    const pre = splitTrustedPreData(docker(container,
      ['pg_restore', '--section=pre-data', '--no-owner', '--no-privileges', '--file=-'], schema));
    const post = docker(container, ['pg_restore', '--section=post-data', '--no-owner', '--no-privileges', '--file=-'], schema);
    phase = 'bundle-artifacts';
    const adminPreArtifact = await artifact(join(bundle, 'trusted-admin-pre-data.sql'), pre.admin);
    const ownedPreArtifact = await artifact(join(bundle, 'trusted-owned-pre-data.sql'), pre.owned);
    const dataArtifact = await artifact(join(bundle, 'trust-data.sql'), Buffer.from(canonicalData));
    const postArtifact = await artifact(join(bundle, 'trusted-post-data.sql'), post);
    const expectationArtifact = await copyBound(options.expectation, join(bundle, 'trust-expectation.json'),
      options['expectation-sha256'], 32 * 1024 * 1024);
    phase = 'atomic-restore';
    const restore = Buffer.concat([Buffer.from("SET statement_timeout='120s'; SET lock_timeout='5s';\n"),
      Buffer.from(freshTrustReceiverGuard()), pre.admin, Buffer.from(`SET ROLE ${TRUST_RECEIVER_PROFILE.role};\n`),
      pre.owned, Buffer.from(canonicalData), post]);
    docker(container, ['psql', '-X', '-q', '-1', '-v', 'ON_ERROR_STOP=1', '-p',
      String(TRUST_RECEIVER_PROFILE.database_port), '-U', 'postgres', '-d', 'memory_trust'], restore);
    phase = 'migration-verification';
    const migrated = sql(container, 'memory_trust', "SELECT encode(checksum,'hex') FROM public._sqlx_migrations ORDER BY version;").split('\n');
    if (JSON.stringify(migrated) !== JSON.stringify(migrationChecksums)) fail();
    phase = 'content-verification';
    const spec = { area: 'trust', operation_id: expectation.operation_id, container_id: container };
    const verifiedTarget = { database: 'memory_trust', instance_id: expectation.target_instance };
    const result = await verifyDatabase(containerSession(container, 'memory_trust'), spec, verifiedTarget, state);
    phase = 'ownership-verification';
    const nonOwned = Number(sql(container, 'memory_trust', `SELECT count(*) FROM (
      SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
        WHERE n.nspname='public' AND c.relkind IN ('r','S','v','m','p','f') AND pg_get_userbyid(c.relowner)<>'${TRUST_RECEIVER_PROFILE.role}'
        AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e')
      UNION ALL SELECT p.oid FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
        WHERE n.nspname='public' AND pg_get_userbyid(p.proowner)<>'${TRUST_RECEIVER_PROFILE.role}'
        AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.deptype='e')
    ) unowned;`));
    if (nonOwned !== 0) fail();
    const catalog = { tables: SCHEMA60_TABLES, sequences: SCHEMA60_SEQUENCES };
    const catalogSha256 = hash(JSON.stringify(catalog));
    phase = 'manifest';
    const manifest = {
      format: 'memory-trust-receiver-bundle-v1', status: 'verified_inactive_unencrypted_not_transferred',
      receiver_profile: TRUST_RECEIVER_PROFILE, receiver_observation: observation,
      source: { instance_id: expectation.source_instance, snapshot_id: expectation.snapshot_id, migration: expectation.source_migration },
      target: { instance_id: expectation.target_instance, operation_id: expectation.operation_id,
        database: TRUST_RECEIVER_PROFILE.database, owner_role: TRUST_RECEIVER_PROFILE.role },
      schema: { sha256: TRUST_SCHEMA, migration_head: 60, migration_checksums: migrationChecksums,
        catalog, catalog_sha256: catalogSha256, object_owner: TRUST_RECEIVER_PROFILE.role },
      artifacts: { admin_pre: adminPreArtifact, owned_pre: ownedPreArtifact, data: dataArtifact,
        post: postArtifact, expectation: expectationArtifact },
      result, source_ddl_executed: false, encrypted: false, transfer_authorized: false, activation_authorized: false,
    };
    const manifestArtifact = await artifact(join(bundle, 'manifest.json'), Buffer.from(JSON.stringify(manifest, null, 2) + '\n'));
    passed = true;
    return { status: manifest.status, manifest: manifestArtifact, histories: result.histories, versions: result.versions,
      rows: result.verified_rows, projected_sources: result.projected_sources, receiver: observation, encrypted: false,
      transfer_authorized: false, activation_authorized: false };
  } catch (error) {
    await artifact(join(options.output, 'failure.json'), Buffer.from(JSON.stringify({
      status: 'failed_not_deliverable', phase, private_diagnostics_included: false,
    }, null, 2) + '\n')).catch(() => {});
    throw error;
  } finally {
    spawnSync('docker', ['exec', container, 'chmod', '0777', '/var/run/postgresql'], { env: clean, stdio: 'ignore', timeout: 5000 });
    const removed = spawnSync('docker', ['rm', '--force', container], { env: clean, stdio: 'ignore', timeout: 30_000 }).status === 0;
    await artifact(join(options.output, 'cleanup.json'), Buffer.from(JSON.stringify({ container, removed, verification_passed: passed }, null, 2) + '\n'));
    if (!removed) fail();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const allowed = new Set(['data', 'data-sha256', 'selection', 'selection-sha256', 'expectation', 'expectation-sha256',
      'schema-tool', 'source-archive', 'output']);
    const options = {};
    for (let index = 2; index < process.argv.length; index += 2) {
      const name = process.argv[index]?.slice(2), value = process.argv[index + 1];
      if (!allowed.has(name) || process.argv[index] !== `--${name}` || !value || options[name]) fail(); options[name] = value;
    }
    if (Object.keys(options).length !== allowed.size) fail();
    console.log(JSON.stringify(await prepareReceiverBundle(options)));
  } catch { console.error('Trust receiver bundle preparation failed. Keep inputs private and inactive.'); process.exitCode = 1; }
}
