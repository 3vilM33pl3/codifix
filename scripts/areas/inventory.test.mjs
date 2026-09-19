// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, symlink, readFile, readdir, stat, rm, chmod } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { BEGIN_SQL, CATALOG_SQL, DATABASE, SERIALIZATION, parseArgs, identifier, literal,
  keyExpression, tableSql, inventoryRow, streamInventory, streamLines, privateArtifact, bindCollectorOptions } from './inventory.mjs';

const options = { 'source-instance': '37e0de32-98b4-4133-81e6-b9d2c2f8f9e9', 'snapshot-id': 'ee7c0a85-a155-4ce8-9aef-b142bd72b36a' };
const table = (name, primary = ['id'], schema = 'public') => ({ schema, name, kind: 'r', is_partition: false,
  columns: ['id', 'slug', 'name', 'root_path', 'created_at'].map(name => ({ name, data_type: 'text', nullable: false })),
  primary_key: primary, foreign_keys: [] });
const raw = (table, key = 'synthetic-id') => ({ event: 'row', table, key, row_json: '{"private": "fixture content", "wide": 9007199254740993}',
  source_project_id: null, canonical_id: null, version: null, references: [] });
function fixture() {
  const tables = [table('projects'), table('_sqlx_migrations'), table('memory_entries', ['id'], 'mab_full_20260516_150014')];
  return [{ event: 'catalog', tables }, { event: 'header', captured_at: '2026-09-17T09:00:00Z', transaction_read_only: 'on', database: DATABASE,
    snapshot: '1:2:', server_version: '17.0', migration_version: '52', failed_migrations: '0',
    project_catalogs: [{ table: 'public.projects', projects: [] }] },
  ...tables.flatMap(entry => [{ event: 'table_start', table: `${entry.schema}.${entry.name}`, row_count: '1' },
    raw(`${entry.schema}.${entry.name}`), { event: 'table_end', table: `${entry.schema}.${entry.name}`, row_count: '1' }]),
  { event: 'complete' }];
}
async function consume(events, code = 0) {
  const sent = [];
  const chunks = [];
  const session = { lines: (async function* () { for (const event of events) yield typeof event === 'string' ? event : JSON.stringify(event); })(),
    send: async sql => sent.push(sql), done: Promise.resolve({ code }) };
  const evidence = await streamInventory(session, options, async text => chunks.push(text));
  return { evidence, inventory: JSON.parse(chunks.join('')), sent };
}

test('requires explicit UUIDs, absolute output and rejects injected options', () => {
  assert.equal(parseArgs(['--descriptor', '/private/restore.json', '--source-instance', options['source-instance'], '--snapshot-id', options['snapshot-id'], '--output', '/tmp/private.json']).output, '/tmp/private.json');
  assert.equal(parseArgs(['--descriptor', '/private/restore.json', '--output', '/tmp/private.json']).descriptor, '/private/restore.json');
  assert.throws(() => parseArgs(['--source-instance', options['source-instance'], '--snapshot-id', options['snapshot-id'], '--output', '/tmp/private.json']), /descriptor/);
  for (const args of [[], ['--output', 'relative.json'], ['--source-instance', 'not-a-uuid'], ['--container', 'live'], ['--output', '/tmp/one', '--output', '/tmp/two']]) assert.throws(() => parseArgs(args));
});

test('optional source selectors must equal the independently verified descriptor binding', () => {
  const sandbox = { source_instance: options['source-instance'], snapshot_id: options['snapshot-id'], database: 'memory_restore_new' };
  assert.equal(bindCollectorOptions({}, sandbox).database, 'memory_restore_new');
  assert.equal(bindCollectorOptions(options, sandbox)['snapshot-id'], options['snapshot-id']);
  assert.throws(() => bindCollectorOptions({ 'snapshot-id': options['source-instance'] }, sandbox), /differs/);
});

test('identifier/literal quoting and exact bigint/composite key construction', () => {
  assert.equal(identifier('odd"table'), '"odd""table"');
  assert.equal(literal("odd'name"), "'odd''name'");
  assert.equal(keyExpression(table('single'), 'r'), "to_jsonb(r)->>'id'");
  assert.equal(keyExpression(table('composite', ['id', 'position']), 'r'), 'jsonb_build_array(r."id",r."position")::text');
  assert.throws(() => keyExpression(table('no_pk', []), 'r'), /without primary keys/);
});

test('full-row hashing preserves raw UTF-8 JSON bytes without JavaScript numeric conversion', () => {
  const input = raw('public.test');
  const result = inventoryRow(input);
  assert.equal(result.row_sha256, createHash('sha256').update(input.row_json).digest('hex'));
  assert.ok(!JSON.stringify(result).includes('fixture content'));
  input.row_json += ' ';
  assert.notEqual(inventoryRow(input).row_sha256, result.row_sha256);
});

test('streaming line decoder retains UTF-8 and final partial lines across arbitrary chunks', async () => {
  const source = Buffer.from('{"x":"é🍀"}\n{"last":true}');
  const chunks = [...source].map(byte => Buffer.from([byte]));
  const lines = [];
  for await (const line of streamLines(Readable.from(chunks))) lines.push(line);
  assert.deepEqual(lines, ['{"x":"é🍀"}', '{"last":true}']);
});

test('oversized complete and partial envelopes fail without truncation, measured as UTF-8 bytes', async () => {
  for (const chunks of [[Buffer.from('123456789\n')], [Buffer.from('1234'), Buffer.from('56789')], [Buffer.from('ééééé\n')]]) {
    await assert.rejects(async () => { for await (const _line of streamLines(Readable.from(chunks), 8)) { /* consume */ } }, /byte limit.*no truncation/);
  }
});

test('one read-only UTC snapshot streams all schemas without public-only filtering', async () => {
  const result = await consume(fixture());
  assert.equal(result.inventory.tables.length, 3);
  assert.equal(result.evidence.row_count, 3);
  assert.deepEqual(result.evidence.schemas, ['public', 'mab_full_20260516_150014']);
  assert.equal(result.evidence.row_serialization, SERIALIZATION);
  assert.equal(result.sent[0], BEGIN_SQL);
  assert.match(BEGIN_SQL, /REPEATABLE READ READ ONLY/);
  assert.match(BEGIN_SQL, /timezone='UTC'/);
  assert.ok(!CATALOG_SQL.includes("n.nspname='public'"));
  assert.match(CATALOG_SQL, /::jsonb::text/); // json_agg(record) may otherwise emit literal newlines.
  assert.ok(result.sent.some(sql => sql.startsWith('COMMIT;')));
  assert.ok(!JSON.stringify(result.inventory).includes('fixture content'));
});

test('truncation, mismatched counts, reordered events and nonzero process exit fail closed', async () => {
  await assert.rejects(consume(fixture().slice(0, -2)), /failed|interrupted/);
  const mismatched = fixture();
  mismatched[2].row_count = '2';
  await assert.rejects(consume(mismatched), /reconciliation/);
  const tooMany = fixture();
  tooMany[2].row_count = '0';
  await assert.rejects(consume(tooMany), /exceeded/);
  const reorder = fixture();
  [reorder[2], reorder[3]] = [reorder[3], reorder[2]];
  await assert.rejects(consume(reorder), /out-of-order/);
  await assert.rejects(consume(fixture(), 1), /failed|interrupted/);
});

test('database error bodies and invalid private stream content are never surfaced', async () => {
  await assert.rejects(consume(['ERROR: secret password fixture']), error => !error.message.includes('password') && /suppressed/.test(error.message));
  const events = fixture();
  events[1].transaction_read_only = 'off';
  await assert.rejects(consume(events), /safety check/);
});

test('unsupported foreign tables and broken foreign-key targets block collection', async () => {
  const events = fixture();
  events[0].tables[0].kind = 'f';
  await assert.rejects(consume(events), /Foreign or unsupported/);
  const input = raw('public.test');
  input.references = [{ constraint: 'parent_fkey', required_target: true, target: null }];
  assert.throws(() => inventoryRow(input), /no target/);
  input.references[0].required_target = false;
  assert.deepEqual(inventoryRow(input).references, [{ constraint: 'parent_fkey', target: null }]);
});

test('no-PK tables fail before any raw row body can be embedded in metadata keys', async () => {
  const events = fixture();
  events[0].tables[2].primary_key = [];
  await assert.rejects(consume(events), /without primary keys/);
});

test('all FK components resolve exact target keys and mandatory logical refs are emitted', () => {
  const parent = table('parent', ['first', 'second']);
  const child = table('child');
  child.foreign_keys.push({ name: 'parent_fkey', columns: ['parent_one', 'parent_two'], target_table: 'public.parent', target_columns: ['first', 'second'] });
  const sql = tableSql(child, [parent, child]);
  assert.match(sql, /r\."parent_one"=fk0\."first" AND r\."parent_two"=fk0\."second"/);
  assert.match(sql, /jsonb_build_array\(fk0\."first",fk0\."second"\)::text/);
  assert.match(sql, /FROM ONLY "public"\."child" r/);
  assert.match(tableSql(table('memory_relations'), []), /logical:src_canonical_id/);
  assert.match(tableSql(table('memory_relations'), []), /logical:dst_canonical_id/);
  assert.match(tableSql(table('memory_sources'), []), /logical:target_memory_id/);
  assert.throws(() => tableSql(table('no_pk', []), []), /without primary keys/);
});

test('private output is mode 0600, exclusive and no incomplete file is published', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'memory-area-inventory-test-'));
  try {
    const output = join(directory, 'inventory.json');
    await privateArtifact(output, async write => { await write('{"fixture":'); await write('true}\n'); });
    assert.equal((await stat(output)).mode & 0o777, 0o600);
    assert.deepEqual(JSON.parse(await readFile(output, 'utf8')), { fixture: true });
    await assert.rejects(privateArtifact(output, async write => write('overwrite')), /already exists/);
    await assert.rejects(privateArtifact(join(directory, 'failed.json'), async write => { await write('partial'); throw Error('synthetic interruption'); }), /synthetic interruption/);
    assert.deepEqual(await readdir(directory), ['inventory.json']);
    await chmod(directory, 0o750);
    await assert.rejects(privateArtifact(join(directory, 'unsafe.json'), async write => write('private')), /mode 0700/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('private output refuses symlinked or shared-writable non-sticky ancestors', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'memory-area-ancestor-test-'));
  try {
    const real = join(directory, 'real');
    const link = join(directory, 'alias');
    const child = join(real, 'child');
    await mkdir(real, { mode: 0o700 });
    await mkdir(child, { mode: 0o700 });
    await symlink(real, link);
    await assert.rejects(privateArtifact(join(link, 'child', 'unsafe.json'), async write => write('private')), /ancestry/);
    await chmod(real, 0o777);
    await assert.rejects(privateArtifact(join(child, 'unsafe.json'), async write => write('private')), /ancestry/);
    assert.deepEqual(await readdir(child), []);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
