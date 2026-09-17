// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { DATABASE } from './inventory.mjs';
import { streamPayload, payloadSql, parseArgs, SERIALIZATION_ID } from './payload.mjs';
const options = { 'source-instance': '00000000-0000-0000-0000-000000000001', 'snapshot-id': '00000000-0000-0000-0000-000000000002', inventory_sha256: 'a'.repeat(64) };
const table = name => ({ schema: 'public', name, kind: 'r', primary_key: ['id'], columns: ['id', 'slug', 'name', 'root_path', 'created_at'].map(name => ({ name })) });
const raw = '{"big": 9007199254740993123456789, "private": "synthetic-only"}';
function events() {
  return [{ event: 'catalog', tables: [table('projects'), table('_sqlx_migrations')] },
    { event: 'header', database: DATABASE, transaction_read_only: 'on', failed_migrations: '0' },
    { event: 'table_start', table: 'public._sqlx_migrations', row_count: '0' },
    { event: 'table_end', table: 'public._sqlx_migrations', row_count: '0' },
    { event: 'table_start', table: 'public.projects', row_count: '2' },
    { event: 'row', table: 'public.projects', key: '10', row_json: raw },
    { event: 'row', table: 'public.projects', key: '2', row_json: raw },
    { event: 'table_end', table: 'public.projects', row_count: '2' }, { event: 'complete' }];
}
async function consume(input = events(), code = 0) {
  const output = [], sql = [];
  const result = await streamPayload({ lines: (async function* () { for (const event of input) yield typeof event === 'string' ? event : JSON.stringify(event); })(), send: async s => sql.push(s), done: Promise.resolve({ code }) }, options, async s => output.push(s));
  return { result, sql, records: output.map(s => JSON.parse(s)) };
}
test('exact private row bytes, lexical keys, empty tables and protocol completion survive', async () => {
  const { result, sql, records } = await consume();
  assert.equal(result.inventory_verified, false);
  assert.equal(records[0].serialization_id, SERIALIZATION_ID);
  assert.equal(records.at(-1).row_count, 2);
  assert.equal(records.at(-1).table_count, 2);
  assert.equal(records[4].row_json, raw);
  assert.equal(records[4].row_sha256, createHash('sha256').update(raw).digest('hex'));
  assert.match(sql[0], /REPEATABLE READ READ ONLY/);
  assert.match(sql[2], /COLLATE "C"/);
  assert.match(payloadSql(table('projects')), /FROM ONLY/);
});
test('missing digest, duplicate options and any live-target selector fail closed', () => {
  const args = ['--source-instance', options['source-instance'], '--snapshot-id', options['snapshot-id'], '--output', '/tmp/private/out'];
  assert.throws(() => parseArgs(args));
  assert.equal(parseArgs([...args, '--inventory-sha256', options.inventory_sha256]).inventory_sha256, options.inventory_sha256);
  for (const extra of [['--container', 'live'], ['--inventory-sha256', 'bad'], ['--inventory-sha256', options.inventory_sha256]]) assert.throws(() => parseArgs([...args, '--inventory-sha256', options.inventory_sha256, ...extra]));
});
test('reordered, duplicate, truncated, inconsistent or failed output never completes', async () => {
  for (const mutate of [
    e => e.slice(0, -1), e => [...e, e.at(-1)],
    e => { e[6].key = '10'; return e; },
    e => { [e[5], e[6]] = [e[6], e[5]]; return e; },
    e => { e[7].row_count = '1'; return e; },
    e => { e[1].transaction_read_only = 'off'; return e; },
    e => { e[0].tables[0].kind = 'f'; return e; },
    e => { e[0].tables[0].primary_key = []; return e; },
  ]) await assert.rejects(consume(mutate(events())));
  await assert.rejects(consume(events(), 1));
  await assert.rejects(consume(['ERROR synthetic password']), error => !error.message.includes('password'));
});
