// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { historyMetadata, consumePack, validateLedger, revalidateLedger, createManifest, runLedger, parseArgs } from './review-ledger.mjs';
import { FULL_REVIEW_SQL, fullHistoryReview, redactReviewText } from './classify.mjs';

const id = n => `00000000-0000-0000-0000-${String(n).padStart(12, '0')}`;
const hash = value => createHash('sha256').update(value).digest('hex');
const schemaHash = hash('schema');
const review = { classification: 'knowledge', source_evidence: 'Exact original rows and evidence references reviewed.',
  project_evidence: 'Original project boundary retained.', content_evidence: 'Every retained version reviewed.' };
function source(n = 10, project = 1) {
  return { canonical_id: id(n), state: { status: 'active' }, state_row_sha256: hash(`state:${n}`),
    projects: [{ id: id(project), slug: `project-${project}`, name: `Project ${project}`, root_path: `/protected/project-${project}`,
      created_at: '2026-09-19T00:00:00Z', row_sha256: hash(`project:${project}`) }],
    versions: [1, 2].map(version => ({ id: id(n + version), project_id: id(project), version_no: version, is_tombstone: version === 2,
      summary: `Version ${version}`, canonical_text: version === 1 ? 'Complete fictional knowledge. Consult /home/fixture/private/report.md.' : '',
      row_sha256: hash(`version:${n}:${version}`) })),
    sources: [{ id: id(n + 3), memory_entry_id: id(n + 1), row_sha256: hash(`source:${n}`), task_id: id(n + 4), session_id: id(n + 5), target_memory_id: null,
      file_path: '/home/fixture/private/source.md', excerpt: 'SOURCE CONTENT MUST STAY LOCAL', task_title: 'PRIVATE TASK TITLE', source_kind: 'file' }],
    dependencies: [{ table: 'public.tasks', key: id(n + 4), row_sha256: hash(`task:${n}`) },
      { table: 'public.sessions', key: id(n + 5), row_sha256: hash(`session:${n}`) },
      { table: 'public.memory_tags', key: `["${id(n + 1)}", "tag"]`, row_sha256: hash(`tag:${n}`) },
      { table: 'public.raw_captures', key: id(n + 6), row_sha256: hash(`capture:${n}`) }], relations: [] };
}
const catalog = [{ schema: 'public', name: 'memory_sources', kind: 'r', foreign_keys: [{ name: 'memory_sources_task_id_fkey', columns: ['task_id'], target_table: 'public.tasks', target_columns: ['id'] }] }];
function indexOf(inputs = [source()]) {
  return { format: 'memory-area-review-index-v1', hash_version: 'full-history-dependencies-v1', source: { source_instance: id(9000), snapshot_id: id(9001),
    inventory_semantic_sha256: hash('inventory'), reviewed_input_manifest_sha256: hash('draft'), inventory_file_sha256: hash('inventory file') },
    schema_sha256: schemaHash, pack_sha256: hash('pack'), catalog,
    projects: [...new Map(inputs.flatMap(x => x.projects).map(({ row_sha256, ...p }) => [p.id, p])).values()],
    histories: inputs.map(x => historyMetadata(x, schemaHash)) };
}
function decision(h, n = 10000) {
  return { canonical_id: h.canonical_id, decision_id: id(n), dependency_sha256: h.dependency_sha256,
    reviewed_content_sha256: h.reviewed_content_sha256, explicitly_approved: true, reviewed_at: '2026-09-19T12:00:00Z',
    approval_reason: 'Reviewed all versions; coherent project ownership.', review, disposition: { kind: 'area', area: 'general' },
    approved_archive_reference_source_ids: h.sources.map(s => s.id) };
}
function ledgerOf(index) {
  return { format: 'memory-area-review-ledger-v1', source_instance: index.source.source_instance, snapshot_id: index.source.snapshot_id,
    inventory_sha256: index.source.inventory_semantic_sha256, pack_sha256: index.pack_sha256, reviewer: { kind: 'agent', id: 'fixture-reviewer' },
    authority_basis: 'Explicit synthetic test review, not authenticated human consent.',
    supporting_records: { mode: 'protected_archive', reason: 'Raw supporting context retained privately without disclosure.', review },
    relation_policy: { same_area: 'preserve', cross_area: 'protected_archive', reason: 'No inferred cross-area sharing.', review },
    decisions: index.histories.map((h, n) => decision(h, 10000 + n)) };
}
function draftOf(index) {
  return { format_version: 1, source_instance: index.source.source_instance, snapshot_id: index.source.snapshot_id,
    inventory_sha256: index.source.inventory_semantic_sha256, inventory_notices: [], areas: ['trust', 'homelab', 'workshop', 'general'],
    table_policies: [{ table: 'public.memory_entries', strategy: 'owned', reason: 'Exact source strategy.' }],
    lineages: index.histories.map(h => ({ canonical_id: h.canonical_id, source_project_ids: h.source_project_ids, disposition: { kind: 'unresolved' } })),
    records: [...index.histories.flatMap(h => [{ record: { table: 'public.tasks', key: h.sources[0]?.task_id ?? id(777) }, disposition: { kind: 'unresolved' } }]),
      ...[...new Map(index.histories.flatMap(h => h.relations).map(r => [r.id, r])).values()].map(r => ({ record: { table: 'public.memory_relations', key: r.id }, disposition: { kind: 'unresolved' } }))], projects: [] };
}

test('full pack includes every version and tombstone but no source context, task title or absolute paths', () => {
  const body = fullHistoryReview(source());
  assert.equal(body.versions.length, 2); assert.equal(body.versions[1].is_tombstone, true);
  assert.equal(body.complete_curated_history, true);
  const encoded = JSON.stringify(body);
  for (const forbidden of ['/home/', 'SOURCE CONTENT MUST STAY LOCAL', 'PRIVATE TASK TITLE', 'session_id']) assert.ok(!encoded.includes(forbidden), forbidden);
  assert.ok(encoded.includes('Complete fictional knowledge'));
  assert.ok(encoded.includes('source.md'));
  for (const path of ['/arbitrary/private/file', '/etc/test', 'C:\\Users\\fixture\\secret', '/home/fixture/path']) assert.ok(!redactReviewText(path).text.includes(path));
});

test('a credential pattern in any version or source withholds the entire history', () => {
  for (const edit of [s => { s.versions[0].canonical_text = 'token sk-proj-' + 's'.repeat(32); },
    s => { s.sources[0].excerpt = 'password=syntheticsecretlong'; }, s => { s.versions[1].canonical_text = '-----BEGIN PRIVATE KEY----- secret -----END PRIVATE KEY-----'; }]) {
    const input = source(); edit(input); const body = fullHistoryReview(input);
    assert.equal(body.review_hold, 'possible_secret_material'); assert.equal(body.complete_curated_history, false);
    assert.ok(body.versions.every(v => v.canonical_text === '[HISTORY WITHHELD]'));
    assert.ok(!JSON.stringify(body).includes('syntheticsecretlong'));
    const index = indexOf([input]); assert.equal(validateLedger(index, ledgerOf(index)).ready, false);
  }
});

test('dependency hashing covers earlier versions, projects, sources, tags, captures and relations', () => {
  const original = source(), before = historyMetadata(original, schemaHash).dependency_sha256;
  for (const edit of [s => { s.versions[0].row_sha256 = hash('changed old version'); },
    s => { s.projects[0].row_sha256 = hash('renamed project'); }, s => { s.sources[0].row_sha256 = hash('new source projection'); },
    s => { s.dependencies[2].row_sha256 = hash('tag'); }, s => { s.dependencies[3].row_sha256 = hash('capture'); },
    s => { s.relations.push({ id: id(500), src_canonical_id: s.canonical_id, dst_canonical_id: id(60), relation_type: 'relates_to', row_sha256: hash('edge') }); }]) {
    const changed = structuredClone(original); edit(changed); assert.notEqual(historyMetadata(changed, schemaHash).dependency_sha256, before);
  }
  const reordered = structuredClone(original); reordered.dependencies.reverse(); reordered.versions.reverse();
  assert.equal(historyMetadata(reordered, schemaHash).dependency_sha256, before);
  assert.notEqual(historyMetadata(original, hash('schema change')).dependency_sha256, before);
});

test('no heuristics, incomplete review, stale labels or missing source approval can mark a ledger ready', () => {
  const index = indexOf();
  assert.equal(validateLedger(index, ledgerOf(index)).ready, true);
  for (const edit of [l => { l.decisions = []; }, l => { l.decisions[0].explicitly_approved = false; },
    l => { l.decisions[0].approved_archive_reference_source_ids = []; }, l => { l.decisions[0].dependency_sha256 = hash('old'); },
    l => { l.decisions.push(l.decisions[0]); }, l => { l.decisions[0].review = {}; }, l => { l.relation_policy = {}; }]) {
    const ledger = ledgerOf(index); edit(ledger); assert.equal(validateLedger(index, ledger).ready, false);
  }
  const forged = ledgerOf(index); forged.reviewer = { kind: 'human', id: 'Olivier' };
  assert.throws(() => validateLedger(index, forged), /explicitly bound/);
});

test('unsupported sources require explicit protected archive rather than lossy import', () => {
  const input = source(); input.sources[0].task_id = null;
  const index = indexOf([input]), ledger = ledgerOf(index);
  assert.ok(validateLedger(index, ledger).problems.some(p => p.code === 'unsupported_source_projection_requires_archive'));
  ledger.decisions[0].disposition = { kind: 'protected_archive', reason: 'Source projection cannot preserve this edge.' };
  ledger.decisions[0].approved_archive_reference_source_ids = [];
  assert.equal(validateLedger(index, ledger).ready, true);
});

test('delta retains only exact reviewed dependencies and exposes new, changed and missing histories', () => {
  const before = indexOf([source(10), source(30), source(50)]), ledger = ledgerOf(before);
  const changed = source(30); changed.dependencies[0].row_sha256 = hash('changed task');
  const after = indexOf([source(10), changed, source(70)]); after.source.snapshot_id = id(9999); after.pack_sha256 = hash('next pack');
  const delta = revalidateLedger(before, after, ledger);
  assert.equal(delta.report.carried, 1); assert.equal(delta.report.ready, false);
  assert.deepEqual(delta.report.changes.map(c => c.change).sort(), ['absent_from_new_snapshot', 'changed_requires_review', 'new_unreviewed']);
  assert.equal(delta.ledger.decisions[0].decision_id, ledger.decisions[0].decision_id);
  assert.equal(delta.ledger.decisions[0].revalidated_from.snapshot_id, before.source.snapshot_id);
  assert.throws(() => createManifest(after, delta.ledger, draftOf(after)), /Every history/);
});

test('full manifests retain more than 64 histories, exact projects and reviewed source projections', () => {
  const index = indexOf(Array.from({ length: 70 }, (_, n) => source(10 + n * 10, 1 + (n % 2))));
  const ledger = ledgerOf(index), manifest = createManifest(index, ledger, draftOf(index));
  assert.equal(manifest.lineages.length, 70); assert.equal(manifest.projects.length, 2);
  assert.equal(manifest.provenance_projections.length, 70); assert.equal(manifest.format_version, 2);
  assert.ok(manifest.projects.every(p => index.projects.some(original => JSON.stringify(original) === JSON.stringify(p.source))));
  const p = manifest.provenance_projections[0];
  assert.equal(p.content.kind, 'archive_reference_only'); assert.equal(p.binding.version.is_tombstone, false);
  assert.equal(p.operator_attestation.operator, 'agent:fixture-reviewer');
  const { operator_attestation, ...body } = p;
  assert.equal(operator_attestation.decision_sha256, hash('mem-area-provenance-projection-v1\n' + JSON.stringify(body)));
  assert.ok(manifest.records.every(r => r.disposition.kind === 'protected_archive'));
});

test('same-area relations preserve both original projects while cross-area relations are archived', () => {
  const a = source(10, 1), b = source(30, 2);
  const edge = { id: id(700), src_canonical_id: a.canonical_id, dst_canonical_id: b.canonical_id, relation_type: 'relates_to', row_sha256: hash('relation') };
  a.relations = [edge]; b.relations = [edge];
  const index = indexOf([a, b]), ledger = ledgerOf(index);
  assert.equal(createManifest(index, ledger, draftOf(index)).records.at(-1).disposition.kind, 'area');
  ledger.decisions[1].disposition.area = 'workshop';
  const cross = createManifest(index, ledger, draftOf(index));
  assert.equal(cross.records.at(-1).disposition.kind, 'protected_archive'); assert.equal(cross.projects.length, 2);
  const unknown = draftOf(index); unknown.table_policies.push({ table: 'public.unknown', strategy: null });
  assert.throws(() => createManifest(index, ledger, unknown), /schema/);
});

test('pack streaming checks full coverage and privately binds exact index to reviewed content', async t => {
  const root = await mkdtemp(join(tmpdir(), 'memory-review-ledger-test-')); t.after(() => rm(root, { recursive: true, force: true }));
  const inputs = [source(10), source(30)], sourceIndex = indexOf(inputs);
  const bindings = { database: 'memory_restore', canonical_ids: new Set(inputs.map(s => s.canonical_id)), expected_versions: 4,
    expected_projects: 1, evidence: sourceIndex.source };
  const events = [{ event: 'header', read_only: 'on', database: 'memory_restore', canonical_count: 2, version_count: 4, project_count: 1 },
    { event: 'catalog', tables: catalog }, ...inputs.map(s => ({ event: 'lineage', ...s })), { event: 'complete' }];
  const lines = list => (async function* () { for (const value of list) yield JSON.stringify(value); })();
  const output = []; const index = await consumePack(lines(events), bindings, async line => output.push(line));
  const pack = output.join(''); index.pack_sha256 = hash(pack);
  const ledger = ledgerOf(index), packPath = join(root, 'pack.ndjson'), indexPath = join(root, 'index.json'), ledgerPath = join(root, 'ledger.json');
  for (const [file, bytes] of [[packPath, pack], [indexPath, JSON.stringify(index)], [ledgerPath, JSON.stringify(ledger)]]) await writeFile(file, bytes, { mode: 0o600 });
  assert.equal((await runLedger('validate-ledger', { index: indexPath, pack: packPath, ledger: ledgerPath })).ready, true);
  index.histories[0].dependencies[0].row_sha256 = hash('changed dependency');
  await writeFile(indexPath, JSON.stringify(index));
  await assert.rejects(runLedger('validate-ledger', { index: indexPath, pack: packPath, ledger: ledgerPath }), /index changed/);
  await assert.rejects(consumePack(lines(events.slice(0, -1)), bindings, async () => {}), /Incomplete/);
  assert.ok(!(await readFile(packPath, 'utf8')).includes('/home/fixture'));
});

test('SQL exports complete hashes but not raw support bodies; selectors cannot accept live connections', () => {
  assert.match(FULL_REVIEW_SQL, /REPEATABLE READ READ ONLY/);
  assert.match(FULL_REVIEW_SQL, /'dependencies'/);
  assert.match(FULL_REVIEW_SQL, /sha256\(convert_to\(to_jsonb\(r\)::text/);
  assert.ok(!/payload_json|['"]session_body['"]|UPDATE public|INSERT INTO/.test(FULL_REVIEW_SQL));
  assert.throws(() => parseArgs('validate-ledger', ['--database', 'postgresql://live']), /Invalid/);
  assert.throws(() => parseArgs('create-manifest', []), /bindings/);
});

test('generated v2 projections and cross-project relations pass the independent Rust verifier', {
  skip: process.env.MEMORY_AREA_TEST_RUST !== '1', timeout: 120_000,
}, async t => {
  const root = await mkdtemp(join(tmpdir(), 'memory-ledger-rust-test-')); t.after(() => rm(root, { recursive: true, force: true }));
  const repo = fileURLToPath(new URL('../../', import.meta.url));
  const policy = await readFile(join(repo, 'crates/mem-area/src/policy.rs'), 'utf8');
  const names = [...new Set([...policy.split('pub const SUPERVISOR_TABLES_0053')[0].matchAll(/"(public\.[a-z_0-9]+)"/g)].map(m => m[1]))];
  const tables = names.map(table => ({ table, columns: [{ name: 'id', data_type: 'uuid', nullable: false }], foreign_keys: [], row_count: 0, rows: [] }));
  const table = name => tables.find(t => t.table === name);
  const push = (name, row) => { table(name).rows.push(row); table(name).row_count++; };
  const row = (key, row_sha256, project = null, canonical = null, version = null, references = []) => ({ key, row_sha256, source_project_id: project, canonical_id: canonical, version, references });
  const fk = (name, column, target) => ({ name, columns: [column], target_table: target, target_columns: ['id'] });
  const reference = (constraint, target, key) => ({ constraint, target: key ? { table: target, key } : null });
  table('public.memory_sources').columns.push(...['memory_entry_id', 'task_id', 'target_memory_id'].map(name => ({ name, data_type: 'uuid', nullable: name !== 'memory_entry_id' })));
  table('public.memory_sources').foreign_keys = [fk('source_version', 'memory_entry_id', 'public.memory_entries'), fk('memory_sources_task_id_fkey', 'task_id', 'public.tasks')];
  table('public.tasks').columns.push({ name: 'session_id', data_type: 'uuid', nullable: false });
  table('public.tasks').foreign_keys = [fk('task_session', 'session_id', 'public.sessions')];
  const inputs = [source(10, 1), source(30, 2)];
  const relation = { id: id(700), src_canonical_id: id(10), dst_canonical_id: id(30), relation_type: 'relates_to', row_sha256: hash('edge') };
  for (const input of inputs) {
    input.relations = [relation];
    const p = input.projects[0], s = input.sources[0];
    push('public.projects', row(p.id, p.row_sha256));
    push('public.memory_canonical_state', row(input.canonical_id, input.state_row_sha256, p.id, input.canonical_id));
    for (const v of input.versions) push('public.memory_entries', row(v.id, v.row_sha256, p.id, input.canonical_id, { version_no: v.version_no, is_tombstone: v.is_tombstone }));
    const deps = new Map(input.dependencies.map(d => [d.key, d.row_sha256]));
    push('public.sessions', row(s.session_id, deps.get(s.session_id), p.id));
    push('public.tasks', row(s.task_id, deps.get(s.task_id), p.id, null, null, [reference('task_session', 'public.sessions', s.session_id)]));
    push('public.memory_sources', row(s.id, s.row_sha256, p.id, input.canonical_id, null,
      [reference('source_version', 'public.memory_entries', s.memory_entry_id), reference('memory_sources_task_id_fkey', 'public.tasks', s.task_id), reference('logical:target_memory_id', 'public.memory_entries', null)]));
  }
  push('public.memory_relations', row(relation.id, relation.row_sha256, null, null, null,
    [reference('logical:src_canonical_id', 'public.memory_canonical_state', relation.src_canonical_id), reference('logical:dst_canonical_id', 'public.memory_canonical_state', relation.dst_canonical_id)]));
  const index = indexOf(inputs);
  const inventory = { format_version: 1, source_instance: index.source.source_instance, snapshot_id: index.source.snapshot_id,
    captured_at: '2026-09-19T12:00:00Z', migration_version: 52, projects: index.projects, tables };
  const inventoryFile = join(root, 'inventory.json'), manifestFile = join(root, 'manifest.json');
  await writeFile(inventoryFile, JSON.stringify(inventory), { mode: 0o600 });
  const plan = spawnSync(join(repo, 'target/debug/memory-area'), ['plan', '--inventory', inventoryFile], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  assert.equal(plan.status, 0, 'Synthetic inventory must pass the compiled Rust schema allowlist');
  const draft = JSON.parse(plan.stdout); index.source.inventory_semantic_sha256 = draft.inventory_sha256;
  const ledger = ledgerOf(index), manifest = createManifest(index, ledger, draft);
  await writeFile(manifestFile, JSON.stringify(manifest), { mode: 0o600 });
  const inspect = spawnSync(join(repo, 'target/debug/memory-area-import'), ['inspect', '--inventory', inventoryFile, '--manifest', manifestFile], { encoding: 'utf8', maxBuffer: 1024 * 1024 });
  assert.equal(inspect.status, 0);
  assert.equal(JSON.parse(inspect.stdout).plan_valid, true, 'Independent Rust must accept projection field order, digests and relation/project ownership');
  manifest.provenance_projections[0].binding.version.version_no++;
  await writeFile(manifestFile, JSON.stringify(manifest));
  const rejected = spawnSync(join(repo, 'target/debug/memory-area-import'), ['inspect', '--inventory', inventoryFile, '--manifest', manifestFile], { encoding: 'utf8' });
  assert.equal(JSON.parse(rejected.stdout).plan_valid, false);
});
