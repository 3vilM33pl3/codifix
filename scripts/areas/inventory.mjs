#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Snapshot-only collector. Raw database stdout/stderr must never reach a terminal.
// See HELP and collector evidence for the serialization and completeness boundary.
import { createHash, randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { open, link, unlink, stat, lstat } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { StringDecoder } from 'node:string_decoder';
import { pathToFileURL } from 'node:url';
import { loadRestoreDescriptor } from './restore-descriptor.mjs';

export const DATABASE = 'memory_restore';
export const MAX_ENVELOPE_BYTES = 64 * 1024 * 1024;
export const SERIALIZATION = 'PostgreSQL to_jsonb(complete_heap_row)::text encoded as UTF-8; session timezone UTC, DateStyle ISO/YMD, IntervalStyle postgres, extra_float_digits 3, bytea_output hex. SHA-256 covers those exact text bytes, with no newline. This represents logical row values, not physical heap bytes or original JSON whitespace.';
export const HELP = `Usage: node scripts/areas/inventory.mjs --descriptor /protected/restore-descriptor.json --output /protected/new-inventory.json

Read-only collector restricted to the exact digest-bound verified restore descriptor.
It verifies private snapshot/archive/proof bindings and a Docker network-none sandbox.
Optional --source-instance UUID and --snapshot-id UUID must match the descriptor.
Uses one REPEATABLE READ READ ONLY transaction and UTC complete-row JSON hashing.
Streams all ordinary/partitioned tables in every non-system schema, including empty
tables. Foreign tables fail closed. FK targets and the codifix-area logical-reference
contract are included. Only public lineage/project metadata is interpreted;
per-schema project catalogs are retained in the private evidence sidecar.

Outputs require an existing owner-only mode-0700 directory and are created
exclusively, mode 0600; existing files are never overwritten.
Raw rows and database errors never go to stdout/stderr. A private .collector.json
sidecar records schema identity, exact key encoding and evidence limitations.
Tables without primary keys fail closed pending a reviewed multiset format.
Envelopes exceeding 64 MiB fail collection; rows are never silently truncated.

This does not import, verify restore correctness, classify historical schemas,
discover references embedded in opaque text/JSON, or certify a partition ready.
Unknown schemas remain codifix-area planning blockers.
`;

export function parseArgs(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index];
    if (!['--source-instance', '--snapshot-id', '--descriptor', '--output'].includes(name)
      || !args[index + 1] || Object.hasOwn(options, name.slice(2))) throw Error('Invalid or duplicate collector option; use --help');
    options[name.slice(2)] = args[index + 1];
  }
  const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  for (const field of ['source-instance', 'snapshot-id']) {
    if (options[field] === undefined) continue;
    if (!uuid.test(options[field] ?? '') || options[field].replaceAll('-', '').replaceAll('0', '') === '') throw Error('Explicit nonzero source and snapshot UUIDs are required');
    options[field] = options[field].toLowerCase();
  }
  if (!options.output || !options.output.startsWith('/')) throw Error('An absolute private output path is required');
  if (!options.descriptor?.startsWith('/')) throw Error('An explicit absolute restore descriptor is required');
  options.descriptor = resolve(options.descriptor);
  options.output = resolve(options.output);
  return options;
}

export function identifier(value) { return `"${String(value).replaceAll('"', '""')}"`; }
export function literal(value) { return `'${String(value).replaceAll("'", "''")}'`; }
function qualified(table) { return `${identifier(table.schema)}.${identifier(table.name)}`; }
function tableName(table) { return `${table.schema}.${table.name}`; }
function has(table, name) { return table.columns.some(column => column.name === name); }

// PostgreSQL performs all key serialization, so bigint/numeric values never pass
// through JavaScript Number. Key strings have the same encoding in FK targets.
export function keyExpression(table, alias) {
  if (table.primary_key.length === 1) return `to_jsonb(${alias})->>${literal(table.primary_key[0])}`;
  if (table.primary_key.length > 1) return `jsonb_build_array(${table.primary_key.map(column => `${alias}.${identifier(column)}`).join(',')})::text`;
  throw Error('Tables without primary keys require a reviewed lossless identity format');
}

export const CATALOG_SQL = `SELECT json_build_object('event','catalog','tables',COALESCE(json_agg(t ORDER BY t.schema,t.name),'[]'::json))::jsonb::text
FROM (SELECT n.nspname AS schema,c.relname AS name,c.relkind AS kind,c.relispartition AS is_partition,
  COALESCE((SELECT json_agg(json_build_object('name',a.attname,'data_type',format_type(a.atttypid,a.atttypmod),'nullable',NOT a.attnotnull) ORDER BY a.attnum)
    FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0 AND NOT a.attisdropped),'[]'::json) AS columns,
  COALESCE((SELECT json_agg(a.attname ORDER BY k.ordinality) FROM pg_constraint p CROSS JOIN LATERAL unnest(p.conkey) WITH ORDINALITY k(attnum,ordinality)
    JOIN pg_attribute a ON a.attrelid=c.oid AND a.attnum=k.attnum WHERE p.conrelid=c.oid AND p.contype='p'),'[]'::json) AS primary_key,
  COALESCE((SELECT json_agg(json_build_object('name',f.conname,
    'columns',(SELECT json_agg(a.attname ORDER BY k.ordinality) FROM unnest(f.conkey) WITH ORDINALITY k(attnum,ordinality) JOIN pg_attribute a ON a.attrelid=c.oid AND a.attnum=k.attnum),
    'target_table',tn.nspname||'.'||tc.relname,
    'target_columns',(SELECT json_agg(a.attname ORDER BY k.ordinality) FROM unnest(f.confkey) WITH ORDINALITY k(attnum,ordinality) JOIN pg_attribute a ON a.attrelid=tc.oid AND a.attnum=k.attnum)) ORDER BY f.conname)
    FROM pg_constraint f JOIN pg_class tc ON tc.oid=f.confrelid JOIN pg_namespace tn ON tn.oid=tc.relnamespace WHERE f.conrelid=c.oid AND f.contype='f'),'[]'::json) AS foreign_keys
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE c.relkind IN ('r','p','f') AND n.nspname<>'information_schema' AND n.nspname !~ '^pg_') t;`;

export const BEGIN_SQL = `BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL timezone='UTC'; SET LOCAL datestyle='ISO, YMD'; SET LOCAL intervalstyle='postgres';
SET LOCAL extra_float_digits=3; SET LOCAL bytea_output='hex'; SET LOCAL row_security=off;
SET LOCAL search_path=pg_catalog; SET LOCAL statement_timeout=0; SET LOCAL standard_conforming_strings=on;
${CATALOG_SQL}\n`;

function projectCatalogSql(table) {
  const fields = ['id', 'slug', 'name', 'root_path', 'created_at'];
  if (!fields.every(field => has(table, field))) throw Error('A project catalog has an unsupported schema; no partial output published');
  return `SELECT json_build_object('table',${literal(tableName(table))},'projects',COALESCE(json_agg(json_build_object(${fields.flatMap(field => [literal(field), `p.${identifier(field)}`]).join(',')}) ORDER BY p.id),'[]'::json)) AS catalog FROM ONLY ${qualified(table)} p`;
}

export function headerSql(tables) {
  if (!tables.some(table => tableName(table) === 'public.projects') || !tables.some(table => tableName(table) === 'public._sqlx_migrations')) throw Error('Required public source schema is missing');
  const catalogs = tables.filter(table => table.name === 'projects').map(projectCatalogSql);
  return `SELECT json_build_object('event','header','captured_at',transaction_timestamp(),'transaction_read_only',current_setting('transaction_read_only'),
    'snapshot',txid_current_snapshot()::text,'database',current_database(),'server_version',current_setting('server_version'),
    'migration_version',(SELECT max(version)::text FROM public._sqlx_migrations WHERE success),
    'failed_migrations',(SELECT count(*)::text FROM public._sqlx_migrations WHERE NOT success),
    'project_catalogs',(SELECT json_agg(catalog) FROM (${catalogs.join(' UNION ALL ')}) catalogs))::jsonb::text;\n`;
}

function projectExpression(table) {
  if (has(table, 'project_id')) return 'r.project_id::text';
  if (table.schema !== 'public') return 'NULL';
  switch (table.name) {
    case 'tasks': return '(SELECT s.project_id::text FROM public.sessions s WHERE s.id=r.session_id)';
    case 'raw_captures': return '(SELECT s.project_id::text FROM public.tasks t JOIN public.sessions s ON s.id=t.session_id WHERE t.id=r.task_id)';
    case 'memory_sources': case 'memory_tags': case 'memory_chunks':
      return '(SELECT m.project_id::text FROM public.memory_entries m WHERE m.id=r.memory_entry_id)';
    case 'memory_source_checks':
      return '(SELECT m.project_id::text FROM public.memory_sources s JOIN public.memory_entries m ON m.id=s.memory_entry_id WHERE s.id=r.source_id)';
    default: return 'NULL';
  }
}

function canonicalExpression(table) {
  if (table.schema !== 'public') return 'NULL';
  if (has(table, 'canonical_id')) return 'r.canonical_id::text';
  switch (table.name) {
    case 'memory_sources': case 'memory_tags':
      return '(SELECT m.canonical_id::text FROM public.memory_entries m WHERE m.id=r.memory_entry_id)';
    case 'memory_source_checks':
      return '(SELECT m.canonical_id::text FROM public.memory_sources s JOIN public.memory_entries m ON m.id=s.memory_entry_id WHERE s.id=r.source_id)';
    default: return 'NULL';
  }
}

function logicalReference(name, table, value) {
  return `json_build_object('constraint',${literal(`logical:${name}`)},'target',CASE WHEN ${value} IS NULL THEN NULL ELSE json_build_object('table',${literal(table)},'key',${value}::text) END,'required_target',false)`;
}

export function tableSql(table, catalog) {
  const name = tableName(table);
  const references = table.foreign_keys.map((foreignKey, index) => {
    const target = catalog.find(candidate => tableName(candidate) === foreignKey.target_table);
    if (!target) throw Error('Foreign key references a table outside the complete source catalog');
    const alias = `fk${index}`;
    const hasValues = foreignKey.columns.map(column => `r.${identifier(column)} IS NOT NULL`).join(' AND ');
    const predicate = foreignKey.columns.map((column, position) => `r.${identifier(column)}=${alias}.${identifier(foreignKey.target_columns[position])}`).join(' AND ');
    // Query an inherited target's children when resolving FK equality. Its
    // catalog parent identity remains explicit; unsupported partition routing
    // therefore becomes a planner blocker, never a silent table copy.
    return `json_build_object('constraint',${literal(foreignKey.name)},'required_target',(${hasValues}),
      'target',CASE WHEN ${hasValues} THEN (SELECT json_build_object('table',${literal(foreignKey.target_table)},'key',${keyExpression(target, alias)}) FROM ${qualified(target)} ${alias} WHERE ${predicate}) ELSE NULL END)`;
  });
  if (name === 'public.memory_relations') {
    references.push(logicalReference('src_canonical_id', 'public.memory_canonical_state', 'r.src_canonical_id'));
    references.push(logicalReference('dst_canonical_id', 'public.memory_canonical_state', 'r.dst_canonical_id'));
  }
  if (name === 'public.memory_sources') references.push(logicalReference('target_memory_id', 'public.memory_entries', 'r.target_memory_id'));
  const version = name === 'public.memory_entries' ? "json_build_object('version_no',r.version_no,'is_tombstone',r.is_tombstone)" : 'NULL';
  return `SELECT json_build_object('event','table_start','table',${literal(name)},'row_count',(SELECT count(*)::text FROM ONLY ${qualified(table)}))::jsonb::text;
SELECT json_build_object('event','row','table',${literal(name)},'key',${keyExpression(table, 'r')},'row_json',to_jsonb(r)::text,
  'source_project_id',${projectExpression(table)},'canonical_id',${canonicalExpression(table)},'version',${version},
  'references',json_build_array(${references.join(',')}))::jsonb::text FROM ONLY ${qualified(table)} r;
SELECT json_build_object('event','table_end','table',${literal(name)},'row_count',(SELECT count(*)::text FROM ONLY ${qualified(table)}))::jsonb::text;\n`;
}

function count(value) {
  if (!/^\d+$/.test(String(value))) throw Error('Invalid count from snapshot collector');
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw Error('Snapshot count exceeds exact collector integer support');
  return parsed;
}

export function inventoryRow(event) {
  if (typeof event.row_json !== 'string' || typeof event.key !== 'string' || !Array.isArray(event.references)) throw Error('Incomplete source row envelope');
  const references = event.references.map(reference => {
    if (reference.required_target && reference.target === null) throw Error('An existing source foreign key has no target; no partial output published');
    return { constraint: reference.constraint, target: reference.target };
  });
  return { key: event.key, row_sha256: createHash('sha256').update(event.row_json, 'utf8').digest('hex'),
    source_project_id: event.source_project_id, canonical_id: event.canonical_id,
    version: event.version, references };
}

// Exported protocol consumer allows synthetic tests without Docker/credentials.
export async function streamInventory(session, options, write) {
  let tables;
  let header;
  let current;
  let tableIndex = 0;
  let total = 0;
  let completed = false;
  let seen = 0;
  let expected = 0;
  await session.send(BEGIN_SQL);
  for await (const line of session.lines) {
    if (!line.trim()) continue;
    let event;
    try { event = JSON.parse(line); } catch { throw Error('Invalid database stream envelope; raw output suppressed'); }
    if (completed) throw Error('Unexpected source output after transaction completion');
    if (event.event === 'catalog' && !tables) {
      tables = event.tables;
      if (!Array.isArray(tables) || tables.length === 0) throw Error('Source catalog is empty or malformed');
      if (tables.some(table => !['r', 'p'].includes(table.kind))) throw Error('Foreign or unsupported tables require separate reviewed collection');
      if (tables.some(table => !Array.isArray(table.primary_key) || !table.primary_key.length)) throw Error('Tables without primary keys require a reviewed lossless identity format');
      if (new Set(tables.map(tableName)).size !== tables.length) throw Error('Duplicate source table identities');
      await session.send(headerSql(tables));
    } else if (event.event === 'header' && tables && !header) {
      header = event;
      if (header.transaction_read_only !== 'on' || header.database !== (options.database ?? DATABASE) || count(header.failed_migrations) !== 0) throw Error('Source transaction/database/migration safety check failed');
      const projects = header.project_catalogs.find(catalog => catalog.table === 'public.projects')?.projects;
      if (!Array.isArray(projects)) throw Error('Public project provenance is missing');
      const start = { format_version: 1, source_instance: options['source-instance'], snapshot_id: options['snapshot-id'],
        captured_at: header.captured_at, migration_version: count(header.migration_version), projects };
      await write(`${JSON.stringify(start).slice(0, -1)},"tables":[`);
      await session.send(tableSql(tables[0], tables));
    } else if (event.event === 'table_start' && header && !current && event.table === tableName(tables[tableIndex])) {
      current = tables[tableIndex];
      seen = 0;
      expected = count(event.row_count);
      const entry = { table: tableName(current), columns: current.columns, foreign_keys: current.foreign_keys, row_count: expected };
      await write(`${tableIndex ? ',' : ''}${JSON.stringify(entry).slice(0, -1)},"rows":[`);
    } else if (event.event === 'row' && current && event.table === tableName(current)) {
      await write(`${seen ? ',' : ''}${JSON.stringify(inventoryRow(event))}`);
      seen += 1;
      if (seen > expected) throw Error('Source stream exceeded independently counted table rows');
    } else if (event.event === 'table_end' && current && event.table === tableName(current)) {
      if (seen !== expected || count(event.row_count) !== expected) throw Error('Source row stream/count reconciliation failed');
      total += seen;
      if (!Number.isSafeInteger(total)) throw Error('Total rows exceed exact collector integer support');
      await write(']}');
      current = null;
      tableIndex += 1;
      if (tableIndex < tables.length) await session.send(tableSql(tables[tableIndex], tables));
      else await session.send("COMMIT; SELECT json_build_object('event','complete')::text;\n\\q\n");
    } else if (event.event === 'complete' && header && !current && tableIndex === tables.length) {
      completed = true;
      await write(']}\n');
    } else throw Error('Unexpected or out-of-order source stream event; raw output suppressed');
  }
  const result = await session.done;
  if (result.code !== 0 || !completed) throw Error('Database collection failed or was interrupted; raw stderr suppressed');
  return {
    collector_format_version: 1, source_instance: options['source-instance'], snapshot_id: options['snapshot-id'],
    captured_at: header.captured_at, database: options.database ?? DATABASE, transaction_snapshot: header.snapshot,
    transaction_read_only: true, server_version: header.server_version, timezone: 'UTC', row_serialization: SERIALIZATION,
    table_count: tables.length, row_count: total, schemas: [...new Set(tables.map(table => table.schema))],
    table_identities: tables.map(table => ({ table: tableName(table), kind: table.kind, is_partition: table.is_partition,
      primary_key: table.primary_key, key_encoding: table.primary_key.length === 1 ? 'single-PK PostgreSQL JSON scalar text' : 'ordered-PK PostgreSQL JSONB array text' })),
    project_catalogs: header.project_catalogs,
    limitations: ['Restored snapshot inventory is not independent proof of restore correctness.',
      'Public project/canonical metadata and explicit nonpublic project_id fields are recorded; source classification is separate.',
      'All physical heap tables are read with ONLY to avoid inheritance duplication; partitioned FK routing requires planner review.',
      'Tables without primary keys fail closed pending a reviewed lossless identity format.',
      'Catalog foreign keys plus explicit codifix-area logical references are inventoried; links embedded in text/JSON are not inferred.',
      'Views, sequences, functions, role/privilege state and physical byte identity remain in the protected backup, not this row inventory.',
      'No classification, import, credential transfer, worker activation or cutover is performed.'],
  };
}

export async function inspectSandbox(descriptorPath) {
  const { descriptor, descriptor_sha256 } = await loadRestoreDescriptor(descriptorPath);
  return { container: descriptor.container_name, container_id: descriptor.container_id, image: descriptor.image_id,
    database: descriptor.database, source_instance: descriptor.source_instance, snapshot_id: descriptor.snapshot_id,
    restore_descriptor_sha256: descriptor_sha256, restore_evidence_sha256: descriptor.restore_evidence.sha256,
    network: 'none', read_only_root: true, published_ports: false };
}

export function bindCollectorOptions(options, sandbox) {
  for (const [option, field] of [['source-instance', 'source_instance'], ['snapshot-id', 'snapshot_id']]) {
    if (options[option] !== undefined && options[option] !== sandbox[field]) throw Error('Collector identity differs from verified restore descriptor');
  }
  return { ...options, 'source-instance': sandbox.source_instance, 'snapshot-id': sandbox.snapshot_id, database: sandbox.database };
}

export function psqlSession(containerId, database = DATABASE) {
  if (!/^[0-9a-f]{64}$/.test(containerId) || !/^memory_restore(?:_[a-z0-9_]+)?$/.test(database)) throw Error('Exact isolated database identity required');
  const child = spawn('docker', ['exec', '-i', containerId, 'psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-h', '/var/run/postgresql', '-U', 'postgres', '-d', database], { stdio: ['pipe', 'pipe', 'pipe'] });
  child.stderr.resume();
  child.stdin.on('error', () => {}); // Failure is reported by the exit event, without SQL/body text.
  const lines = streamLines(child.stdout);
  const done = new Promise(resolveResult => {
    child.once('error', () => resolveResult({ code: -1 }));
    child.once('close', code => resolveResult({ code }));
  });
  return { lines, done, async send(sql) {
    if (child.stdin.destroyed) throw Error('Database collector stream closed unexpectedly');
    if (!child.stdin.write(sql)) {
      const drained = await Promise.race([once(child.stdin, 'drain').then(() => true).catch(() => false), done.then(() => false)]);
      if (!drained) throw Error('Database collector stream interrupted');
    }
  }, abort() { child.stdin.destroy(); child.kill('SIGTERM'); child.stdout.destroy(); } };
}

// Readable async iteration propagates backpressure to psql. Do not replace this
// with an unbounded queue of readline events while millions of rows are hashed.
export async function* streamLines(input, maxBytes = MAX_ENVELOPE_BYTES) {
  const decoder = new StringDecoder('utf8');
  let pending = '';
  for await (const chunk of input) {
    pending += decoder.write(chunk);
    let start = 0;
    for (let newline = pending.indexOf('\n', start); newline !== -1; newline = pending.indexOf('\n', start)) {
      const line = pending.slice(start, newline);
      if (Buffer.byteLength(line, 'utf8') > maxBytes) throw Error('Source envelope exceeds collector byte limit; no truncation performed');
      yield line;
      start = newline + 1;
    }
    pending = pending.slice(start);
    if (Buffer.byteLength(pending, 'utf8') > maxBytes) throw Error('Source envelope exceeds collector byte limit; no truncation performed');
  }
  pending += decoder.end();
  if (Buffer.byteLength(pending, 'utf8') > maxBytes) throw Error('Source envelope exceeds collector byte limit; no truncation performed');
  if (pending.length) yield pending;
}

export async function privateArtifact(output, body) {
  // A private leaf alone is insufficient under a renameable shared ancestor.
  // Same-user/root mutation remains outside this operator-file boundary.
  let ancestor = dirname(output);
  for (;;) {
    const entry = await lstat(ancestor);
    const trusted = entry.uid === 0 || entry.uid === process.getuid();
    if (!entry.isDirectory() || !trusted || ((entry.mode & 0o022) !== 0 && (entry.mode & 0o1000) === 0)) throw Error('Output ancestry must be trusted and non-symlinked');
    const parent = dirname(ancestor);
    if (parent === ancestor) break;
    ancestor = parent;
  }
  const directory = await lstat(dirname(output));
  if (!directory.isDirectory() || directory.uid !== process.getuid() || (directory.mode & 0o777) !== 0o700) throw Error('Output parent must be owned by the current user with mode 0700');
  try { await stat(output); throw Error('Output already exists; refusing overwrite'); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const temporary = `${output}.partial-${randomUUID()}`;
  const file = await open(temporary, 'wx', 0o600);
  try {
    const write = async text => {
      const buffer = Buffer.from(text, 'utf8');
      let offset = 0;
      while (offset < buffer.length) {
        const { bytesWritten } = await file.write(buffer, offset, buffer.length - offset);
        if (bytesWritten <= 0) throw Error('Private inventory write failed');
        offset += bytesWritten;
      }
    };
    const result = await body(write);
    await file.sync();
    await file.close();
    await link(temporary, output); // Exclusive publish: link cannot replace an existing destination.
    await unlink(temporary);
    const parent = await open(dirname(output), 'r');
    try { await parent.sync(); } finally { await parent.close(); }
    return result;
  } catch (error) {
    await file.close().catch(() => {});
    await unlink(temporary).catch(() => {});
    throw error;
  }
}

export async function collect(options) {
  const sandbox = await inspectSandbox(options.descriptor);
  options = bindCollectorOptions(options, sandbox);
  const sidecar = `${options.output}.collector.json`;
  try { await stat(sidecar); throw Error('Collector evidence path already exists; refusing overwrite'); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  let session;
  let evidence;
  try {
    evidence = await privateArtifact(options.output, async write => {
      session = psqlSession(sandbox.container_id, sandbox.database);
      return streamInventory(session, options, write);
    });
    const digest = createHash('sha256');
    // Streaming file digest binds the artifact without loading millions of rows.
    const inventory = await open(options.output, 'r');
    for await (const data of inventory.createReadStream()) digest.update(data);
    await privateArtifact(sidecar, async write => write(`${JSON.stringify({ ...evidence, sandbox, inventory_file_sha256: digest.digest('hex') }, null, 2)}\n`));
  } catch (error) {
    session?.abort();
    throw error;
  }
  return { table_count: evidence.table_count, row_count: evidence.row_count, schema_count: evidence.schemas.length,
    inventory_written: true, collector_evidence_written: true, classification_or_import_performed: false };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (process.argv.slice(2).includes('--help')) process.stdout.write(HELP);
  else {
    try { process.stdout.write(`${JSON.stringify(await collect(parseArgs(process.argv.slice(2))))}\n`); }
    catch { process.stderr.write('Inventory collection failed; no database output was disclosed. Check sandbox state, explicit UUIDs, output permissions and collector contract. An inventory without its collector evidence sidecar is incomplete.\n'); process.exitCode = 1; }
  }
}
