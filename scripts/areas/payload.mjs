#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Exact private payload export from the already isolated restoration only.
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { BEGIN_SQL, DATABASE, CONTAINER, MAX_ENVELOPE_BYTES, headerSql, keyExpression,
  identifier, literal, inspectSandbox, psqlSession, privateArtifact, parseArgs as inventoryArgs } from './inventory.mjs';

export const SERIALIZATION_ID = 'postgresql-to-jsonb-utf8-utc-v1';
export const HELP = `Usage: node scripts/areas/payload.mjs --source-instance UUID --snapshot-id UUID --inventory-sha256 SHA256 --output /protected/new-payload.ndjson

Reads only ${CONTAINER}/${DATABASE}, never a live/configured database.
Uses one read-only repeatable-read transaction, complete row JSON, C-locale key
ordering and exact SHA-256 bytes. Output is exclusive mode 0600 beneath an
existing owner-only 0700 parent. No row or database error is printed.

--inventory-sha256 is the normalized digest from memory-area inventory, not the
inventory file digest. This argument is a claimed binding: memory-area-import
must independently check every exported row against the reviewed inventory.
This command does not approve ownership, import records or activate workers.
`;

export function parseArgs(args) {
  let digest;
  const remaining = [];
  for (let i = 0; i < args.length; i += 2) {
    if (args[i] === '--inventory-sha256') {
      if (digest !== undefined || !/^[0-9a-f]{64}$/.test(args[i + 1] ?? '')) throw Error('Exact inventory digest required');
      digest = args[i + 1];
    } else remaining.push(args[i], args[i + 1]);
  }
  if (!digest) throw Error('Exact inventory digest required');
  return { ...inventoryArgs(remaining), inventory_sha256: digest };
}
const name = table => `${table.schema}.${table.name}`;
const qualified = table => `${identifier(table.schema)}.${identifier(table.name)}`;
const compare = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b));
function count(value) {
  if (!/^\d+$/.test(String(value)) || !Number.isSafeInteger(Number(value))) throw Error('Invalid exact payload count');
  return Number(value);
}
export function payloadSql(table) {
  const key = keyExpression(table, 'r');
  return `SELECT json_build_object('event','table_start','table',${literal(name(table))},'row_count',(SELECT count(*)::text FROM ONLY ${qualified(table)}))::jsonb::text;
SELECT json_build_object('event','row','table',${literal(name(table))},'key',${key},'row_json',to_jsonb(r)::text)::jsonb::text FROM ONLY ${qualified(table)} r ORDER BY (${key}) COLLATE "C";
SELECT json_build_object('event','table_end','table',${literal(name(table))},'row_count',(SELECT count(*)::text FROM ONLY ${qualified(table)}))::jsonb::text;\n`;
}

export async function streamPayload(session, options, write) {
  let tables, header, current, previous;
  let index = 0, seen = 0, expected = 0, total = 0, complete = false;
  const emit = async value => {
    const encoded = `${JSON.stringify(value)}\n`;
    if (Buffer.byteLength(encoded) > MAX_ENVELOPE_BYTES) throw Error('Payload envelope exceeds byte limit');
    await write(encoded);
  };
  await session.send(BEGIN_SQL);
  for await (const line of session.lines) {
    if (!line.trim()) continue;
    let event;
    try { event = JSON.parse(line); } catch { throw Error('Invalid private source envelope; suppressed'); }
    if (complete) throw Error('Unexpected output after source completion');
    if (event.event === 'catalog' && !tables) {
      if (!Array.isArray(event.tables) || !event.tables.length || event.tables.some(t => !['r', 'p'].includes(t.kind) || !Array.isArray(t.primary_key) || !t.primary_key.length)) throw Error('Unsupported source catalog');
      tables = [...event.tables].sort((a, b) => compare(name(a), name(b)));
      if (new Set(tables.map(name)).size !== tables.length) throw Error('Duplicate source tables');
      await session.send(headerSql(tables));
    } else if (event.event === 'header' && tables && !header) {
      if (event.database !== DATABASE || event.transaction_read_only !== 'on' || count(event.failed_migrations) !== 0) throw Error('Source safety check failed');
      header = event;
      await emit({ event: 'header', format_version: 1, source_instance: options['source-instance'], snapshot_id: options['snapshot-id'], inventory_sha256: options.inventory_sha256, serialization_id: SERIALIZATION_ID });
      await session.send(payloadSql(tables[0]));
    } else if (event.event === 'table_start' && header && !current && index < tables.length && event.table === name(tables[index])) {
      current = tables[index]; previous = undefined; seen = 0; expected = count(event.row_count);
      await emit({ event: 'table_start', table: name(current), row_count: expected });
    } else if (event.event === 'row' && current && event.table === name(current)) {
      if (typeof event.key !== 'string' || !event.key || typeof event.row_json !== 'string' || (previous !== undefined && compare(previous, event.key) >= 0) || seen >= expected) throw Error('Payload identity/order/count mismatch');
      previous = event.key; seen++;
      await emit({ event: 'row', table: name(current), key: event.key,
        row_sha256: createHash('sha256').update(event.row_json, 'utf8').digest('hex'), row_json: event.row_json });
    } else if (event.event === 'table_end' && current && event.table === name(current)) {
      if (seen !== expected || count(event.row_count) !== expected) throw Error('Payload count mismatch');
      total += seen;
      if (!Number.isSafeInteger(total)) throw Error('Payload total exceeds exact integer range');
      await emit({ event: 'table_end', table: name(current), row_count: seen });
      current = undefined; index++;
      await session.send(index < tables.length ? payloadSql(tables[index]) : "COMMIT; SELECT json_build_object('event','complete')::text;\n\\q\n");
    } else if (event.event === 'complete' && header && !current && index === tables.length) {
      complete = true;
    } else throw Error('Unexpected private source event; suppressed');
  }
  if ((await session.done).code !== 0 || !complete) throw Error('Source export interrupted; suppressed');
  // Completion is emitted only after PostgreSQL exits successfully.
  await emit({ event: 'complete', table_count: tables.length, row_count: total });
  return { table_count: tables.length, row_count: total, serialization_id: SERIALIZATION_ID,
    payload_written: true, inventory_verified: false, import_performed: false };
}

export async function collect(options) {
  const sandbox = await inspectSandbox();
  let session;
  try {
    return await privateArtifact(options.output, async write => {
      session = psqlSession(sandbox.container_id);
      return streamPayload(session, options, write);
    });
  } catch (error) { session?.abort(); throw error; }
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (process.argv.slice(2).includes('--help')) process.stdout.write(HELP);
  else try { process.stdout.write(`${JSON.stringify(await collect(parseArgs(process.argv.slice(2))))}\n`); }
  catch { process.stderr.write('Private payload export failed. No source rows or database diagnostics were disclosed; check the isolated restore and protected output.\n'); process.exitCode = 1; }
}
