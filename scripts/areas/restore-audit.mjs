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
import { validateSandbox, inspectContainer, privateInput, createRestoreDescriptor } from './restore-descriptor.mjs';
export { validateSandbox } from './restore-descriptor.mjs';

export async function archiveInventory(input) {
  const tables = new Map(), sequences = new Map(); let current;
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
    } else if (/\bsetval\s*\(/.test(line) && !line.startsWith('--')) {
      // Parse the exact pg_dump sequence-set protocol, never execute dump SQL.
      // Keep bigint values as strings and compare is_called, including false.
      const match = line.match(/^SELECT pg_catalog\.setval\('((?:[^']|'')+)', (-?\d+), (true|false)\);$/);
      if (!match) throw Error('Unsupported sequence state in archive');
      const sequence = match[1].replaceAll("''", "'");
      if (sequences.has(sequence)) throw Error('Duplicate sequence state in archive');
      sequences.set(sequence, { sequence, last_value: BigInt(match[2]).toString(), is_called: match[3] === 'true' });
    }
  }
  if (current || tables.size === 0) throw Error('Truncated or empty COPY inventory');
  return { tables: [...tables].sort(([a], [b]) => a.localeCompare(b)).map(([table, value]) => ({
    table, columns: value.columns, rows: value.rows.length,
    row_set_sha256: createHash('sha256').update(value.rows.sort().join('\n')).digest('hex'),
  })), sequences: [...sequences.values()].sort((a, b) => a.sequence.localeCompare(b.sequence)) };
}

export async function copyInventory(input) { return (await archiveInventory(input)).tables; }

export function inventoryDifferences(expected, actual, identity = 'table') {
  const before = new Map(expected.map(table => [table[identity], table]));
  const after = new Map(actual.map(table => [table[identity], table]));
  return [...new Set([...before.keys(), ...after.keys()])].sort().flatMap(table => {
    const left = before.get(table), right = after.get(table);
    return JSON.stringify(left) === JSON.stringify(right) ? [] : [{ [identity]: table,
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

export async function restoreAudit(directory, container, verifyExisting = false, timezoneOverride, database = 'memory_restore') {
  if (!/^memory_restore(?:_[a-z0-9_]+)?$/.test(database)) throw Error('Explicit restore database name is invalid');
  const root = path.resolve(directory);
  const snapshot = await privateInput(path.join(root, 'snapshot.json'), 1024 * 1024, true);
  const manifest = snapshot.value;
  const timezone = archiveTimezone(manifest, timezoneOverride);
  const dump = manifest.artifacts?.find(item => item.name === 'database.pgdump.gpg');
  const encrypted = path.join(root, 'database.pgdump.gpg');
  if (manifest.schema_version !== 1 || !dump || dump.sha256 !== (await privateInput(encrypted)).sha256)
    throw Error('Snapshot checksum mismatch');
  const metadata = inspectContainer(container); validateSandbox(metadata, container);
  if (!/^[0-9a-f]{64}$/.test(metadata.Id) || !/^sha256:[0-9a-f]{64}$/.test(metadata.Image)) throw Error('Restore sandbox lacks exact identities');
  const containerId = metadata.Id;
  const empty = spawnSync('docker', ['exec', containerId, 'psql', '-X', '-U', 'postgres', '-d', database,
    '-Atc', "SELECT count(*) FROM pg_tables WHERE schemaname NOT IN ('pg_catalog','information_schema') AND schemaname !~ '^pg_toast'"], { encoding: 'utf8' });
  if (empty.status !== 0) throw Error('Sandbox database inspection failed');
  if (!verifyExisting) {
    if (empty.stdout.trim() !== '0') throw Error('Restore requires a fresh empty memory_restore database');
    const restored = start('docker', ['exec', '-i', containerId, 'pg_restore', '-U', 'postgres',
      `--dbname=${database}`, '--exit-on-error', '--no-owner', '--no-privileges']);
    await pipeRun(decrypt(encrypted), restored);
  }
  const archive = start('pg_restore', ['--data-only', '--no-owner', '--no-privileges', '--file=-']);
  const expected = await pipeRun(decrypt(encrypted), archive, archiveInventory);
  const actualDump = start('docker', ['exec', '--env', `PGOPTIONS=-c timezone=${timezone}`, containerId, 'pg_dump', '-U', 'postgres', '-d', database,
    '--data-only', '--no-owner', '--no-privileges']);
  actualDump.child.stdin.end();
  const [actual] = await Promise.all([archiveInventory(actualDump.child.stdout), actualDump.done]);
  const differences = [...inventoryDifferences(expected.tables, actual.tables), ...inventoryDifferences(expected.sequences, actual.sequences, 'sequence')];
  if (differences.length) {
    // Metadata and hashes only; never persist differing record contents here.
    await fs.writeFile(path.join(root, `restore-mismatch-${Date.now()}.json`),
      JSON.stringify(differences, null, 2) + '\n', { mode: 0o600, flag: 'wx' });
    throw Error('Restored table rows or sequence state differ from archive; protected diagnostic saved');
  }
  const validation = spawnSync('docker', ['exec', containerId, 'psql', '-X', '-U', 'postgres', '-d', database,
    '-Atc', "SELECT count(*) FROM pg_index WHERE NOT indisvalid; SELECT count(*) FROM pg_constraint WHERE NOT convalidated;"],
  { encoding: 'utf8' });
  if (validation.status !== 0 || validation.stdout.trim() !== '0\n0') throw Error('Restored schema has invalid indexes or unvalidated constraints');
  const after = inspectContainer(containerId); validateSandbox(after, container);
  if (after.Id !== metadata.Id || after.Image !== metadata.Image) throw Error('Restore sandbox changed during audit');
  const report = { schema_version: 2, completed_at: new Date().toISOString(),
    snapshot_manifest_sha256: snapshot.sha256, database_archive_sha256: dump.sha256,
    container_name: container, container_id: metadata.Id, image_id: metadata.Image, database,
    comparison_timezone: timezone, timezone_from_operator: !manifest.database_session_timezone,
    network: 'none', published_ports: false, restored_table_inventory: actual.tables,
    restored_sequence_inventory: actual.sequences, all_sequence_states_match: true,
    all_table_row_hashes_match: true, invalid_indexes: 0, unvalidated_constraints: 0,
    limitations: ['Does not verify application startup, operational identities or final writer-stop capture.',
      'Role ownership and grants require separate explicitly reviewed provisioning.'], cutover_ready: false };
  await fs.writeFile(path.join(root, 'restore-evidence.json'), JSON.stringify(report, null, 2) + '\n',
    { mode: 0o600, flag: 'wx' });
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const [directory, container, ...options] = process.argv.slice(2);
  let verifyExisting = false, timezone, sourceInstance, snapshotId, database;
  for (let i = 0; i < options.length; i++) {
    if (options[i] === '--verify-existing' && !verifyExisting) verifyExisting = true;
    else if (options[i] === '--archive-timezone' && !timezone) timezone = options[++i];
    else if (options[i] === '--source-instance' && !sourceInstance) sourceInstance = options[++i];
    else if (options[i] === '--snapshot-id' && !snapshotId) snapshotId = options[++i];
    else if (options[i] === '--database' && !database) database = options[++i];
    else throw Error('Unknown or repeated restore option');
  }
  if (!directory || !container || Boolean(sourceInstance) !== Boolean(snapshotId))
    throw Error('Usage: restore-audit.mjs SNAPSHOT_DIRECTORY SANDBOX_CONTAINER [--verify-existing] [--archive-timezone ZONE] [--database memory_restore_NAME] [--source-instance UUID --snapshot-id UUID]');
  restoreAudit(directory, container, verifyExisting, timezone, database).then(async report => {
    const descriptor = sourceInstance ? await createRestoreDescriptor(directory, sourceInstance, snapshotId, path.join(path.resolve(directory), 'restore-descriptor.json')) : {};
    console.log(JSON.stringify({
    status: 'database_restore_verified', tables: report.restored_table_inventory.length,
    rows: report.restored_table_inventory.reduce((n, table) => n + table.rows, 0), cutover_ready: false,
    sequences: report.restored_sequence_inventory.length, ...descriptor,
  })); }).catch(error => {
    // Messages are fixed local diagnostics; child/provider output is never surfaced.
    console.error(error.message); process.exitCode = 1;
  });
}
