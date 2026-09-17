#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Deliberately restores only into an explicitly created network-none sandbox.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createInterface } from 'node:readline';
import { pipeline } from 'node:stream/promises';
import { pathToFileURL } from 'node:url';
import { hashFile } from './snapshot.mjs';

export async function copyInventory(input) {
  const tables = new Map(); let current;
  for await (const line of createInterface({ input, crlfDelay: Infinity })) {
    if (current) {
      if (line === '\\.') { current = undefined; continue; }
      current.rows.push(createHash('sha256').update(line).digest('hex'));
    } else if (line.startsWith('COPY ')) {
      // Back up every schema, including historical evaluation namespaces. Keep
      // identifiers as inert strings; this parser never interpolates them into SQL.
      const match = line.match(/^COPY ((?:[a-z_][a-z_0-9]*|"(?:[^"]|"")+")\.(?:[a-z_][a-z_0-9]*|"(?:[^"]|"")+")) (\(.*\)) FROM stdin;$/);
      if (!match || tables.has(match[1])) throw Error('Unsupported or duplicate COPY table');
      current = { columns: match[2], rows: [] }; tables.set(match[1], current);
    }
  }
  if (current || tables.size === 0) throw Error('Truncated or empty COPY inventory');
  return [...tables].sort(([a], [b]) => a.localeCompare(b)).map(([table, value]) => ({
    table, columns: value.columns, rows: value.rows.length,
    row_set_sha256: createHash('sha256').update(value.rows.sort().join('\n')).digest('hex'),
  }));
}

export function inventoryDifferences(expected, actual) {
  const before = new Map(expected.map(table => [table.table, table]));
  const after = new Map(actual.map(table => [table.table, table]));
  return [...new Set([...before.keys(), ...after.keys()])].sort().flatMap(table => {
    const left = before.get(table), right = after.get(table);
    return JSON.stringify(left) === JSON.stringify(right) ? [] : [{ table,
      expected: left ?? null, actual: right ?? null }];
  });
}

export function archiveTimezone(manifest, override) {
  const value = override ?? manifest.database_session_timezone;
  if (!value || !/^[A-Za-z0-9_+\/-]+$/.test(value))
    throw Error('Archive timezone is required for byte-exact timestamp verification');
  try { new Intl.DateTimeFormat('en', { timeZone: value }); }
  catch { throw Error('Invalid archive timezone'); }
  if (override && manifest.database_session_timezone && override !== manifest.database_session_timezone)
    throw Error('Archive timezone override conflicts with manifest');
  return value;
}

function start(command, args) {
  const child = spawn(command, args, { stdio: ['pipe', 'pipe', 'pipe'] });
  child.stderr.resume();
  const done = new Promise((resolve, reject) => {
    child.once('error', () => reject(Error(`${command} could not start`)));
    child.once('close', code => code === 0 ? resolve() : reject(Error(`${command} failed (${code})`)));
  });
  return { child, done };
}

function decrypt(file) {
  const source = start('gpg', ['--batch', '--no-tty', '--pinentry-mode', 'error', '--decrypt', file]);
  source.child.stdin.end(); return source;
}

async function pipeRun(source, target, inspect) {
  const pipe = pipeline(source.child.stdout, target.child.stdin);
  const observed = inspect ? inspect(target.child.stdout) : (target.child.stdout.resume(), Promise.resolve());
  try { const [, , , result] = await Promise.all([pipe, source.done, target.done, observed]); return result; }
  catch (error) {
    source.child.kill('SIGTERM'); target.child.kill('SIGTERM');
    await Promise.allSettled([pipe, source.done, target.done, observed]); throw error;
  }
}

export function validateSandbox(metadata, container) {
  if (!/^memory-four-area-restore-[a-z0-9-]+$/.test(container)
      || metadata.Name !== `/${container}` || !metadata.State?.Running
      || metadata.HostConfig?.NetworkMode !== 'none' || !metadata.HostConfig?.ReadonlyRootfs
      || Object.keys(metadata.NetworkSettings?.Ports || {}).length
        && Object.values(metadata.NetworkSettings.Ports).some(value => value !== null))
    throw Error('Restore target must be an explicitly named network-none, read-only-root sandbox without published ports');
}

export async function restoreAudit(directory, container, verifyExisting = false, timezoneOverride) {
  const root = path.resolve(directory);
  const manifest = JSON.parse(await fs.readFile(path.join(root, 'snapshot.json'), 'utf8'));
  const timezone = archiveTimezone(manifest, timezoneOverride);
  const dump = manifest.artifacts?.find(item => item.name === 'database.pgdump.gpg');
  const encrypted = path.join(root, 'database.pgdump.gpg');
  if (manifest.schema_version !== 1 || !dump || dump.sha256 !== await hashFile(encrypted))
    throw Error('Snapshot checksum mismatch');
  const inspected = spawnSync('docker', ['inspect', container], { encoding: 'utf8' });
  if (inspected.status !== 0) throw Error('Restore sandbox unavailable');
  const metadata = JSON.parse(inspected.stdout)[0]; validateSandbox(metadata, container);
  const empty = spawnSync('docker', ['exec', container, 'psql', '-X', '-U', 'postgres', '-d', 'memory_restore',
    '-Atc', "SELECT count(*) FROM pg_tables WHERE schemaname NOT IN ('pg_catalog','information_schema') AND schemaname !~ '^pg_toast'"], { encoding: 'utf8' });
  if (empty.status !== 0) throw Error('Sandbox database inspection failed');
  if (!verifyExisting) {
    if (empty.stdout.trim() !== '0') throw Error('Restore requires a fresh empty memory_restore database');
    const restored = start('docker', ['exec', '-i', container, 'pg_restore', '-U', 'postgres',
      '--dbname=memory_restore', '--exit-on-error', '--no-owner', '--no-privileges']);
    await pipeRun(decrypt(encrypted), restored);
  }
  const archive = start('pg_restore', ['--data-only', '--no-owner', '--no-privileges', '--file=-']);
  const expected = await pipeRun(decrypt(encrypted), archive, copyInventory);
  const actualDump = start('docker', ['exec', '--env', `PGOPTIONS=-c timezone=${timezone}`, container, 'pg_dump', '-U', 'postgres', '-d', 'memory_restore',
    '--data-only', '--no-owner', '--no-privileges']);
  actualDump.child.stdin.end();
  const [actual] = await Promise.all([copyInventory(actualDump.child.stdout), actualDump.done]);
  const differences = inventoryDifferences(expected, actual);
  if (differences.length) {
    // Metadata and hashes only; never persist differing record contents here.
    await fs.writeFile(path.join(root, `restore-mismatch-${Date.now()}.json`),
      JSON.stringify(differences, null, 2) + '\n', { mode: 0o600, flag: 'wx' });
    throw Error('Restored table row/hash inventory differs from archive; protected diagnostic saved');
  }
  const validation = spawnSync('docker', ['exec', container, 'psql', '-X', '-U', 'postgres', '-d', 'memory_restore',
    '-Atc', "SELECT count(*) FROM pg_index WHERE NOT indisvalid; SELECT count(*) FROM pg_constraint WHERE NOT convalidated;"],
  { encoding: 'utf8' });
  if (validation.status !== 0 || validation.stdout.trim() !== '0\n0') throw Error('Restored schema has invalid indexes or unvalidated constraints');
  const report = { schema_version: 1, completed_at: new Date().toISOString(),
    database_archive_sha256: dump.sha256, container_id: metadata.Id, image_id: metadata.Image,
    comparison_timezone: timezone, timezone_from_operator: !manifest.database_session_timezone,
    network: 'none', published_ports: false, restored_table_inventory: actual,
    all_table_row_hashes_match: true, invalid_indexes: 0, unvalidated_constraints: 0,
    limitations: ['Does not verify application startup, operational identities or final writer-stop capture.',
      'Role ownership and grants require separate explicitly reviewed provisioning.'], cutover_ready: false };
  await fs.writeFile(path.join(root, 'restore-evidence.json'), JSON.stringify(report, null, 2) + '\n',
    { mode: 0o600, flag: 'wx' });
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const [directory, container, ...options] = process.argv.slice(2);
  let verifyExisting = false, timezone;
  for (let i = 0; i < options.length; i++) {
    if (options[i] === '--verify-existing' && !verifyExisting) verifyExisting = true;
    else if (options[i] === '--archive-timezone' && !timezone) timezone = options[++i];
    else throw Error('Unknown or repeated restore option');
  }
  if (!directory || !container)
    throw Error('Usage: restore-audit.mjs SNAPSHOT_DIRECTORY SANDBOX_CONTAINER [--verify-existing] [--archive-timezone ZONE]');
  restoreAudit(directory, container, verifyExisting, timezone).then(report => console.log(JSON.stringify({
    status: 'database_restore_verified', tables: report.restored_table_inventory.length,
    rows: report.restored_table_inventory.reduce((n, table) => n + table.rows, 0), cutover_ready: false,
  }))).catch(error => {
    // Messages are fixed local diagnostics; child/provider output is never surfaced.
    console.error(error.message); process.exitCode = 1;
  });
}
