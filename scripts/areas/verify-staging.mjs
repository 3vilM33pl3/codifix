#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Independent read-only verification of an already imported inactive fleet.
import { constants } from 'node:fs';
import { open } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { resolve, isAbsolute } from 'node:path';
import { pathToFileURL } from 'node:url';
import { privateInput, inspectContainer } from './restore-descriptor.mjs';
import { BEGIN_SQL, identifier, literal, keyExpression, streamLines, privateArtifact } from './inventory.mjs';

export const SCHEMA60_TABLES = [
  "public._sqlx_migrations",
  "public.agent_workspaces",
  "public.approval_requests",
  "public.area_import_provenance",
  "public.area_import_receipts",
  "public.area_import_rows",
  "public.auth_audit_events",
  "public.auth_oidc_flows",
  "public.auth_principals",
  "public.auth_project_memberships",
  "public.auth_service_tokens",
  "public.auth_web_sessions",
  "public.background_daily_budget",
  "public.background_job_attempts",
  "public.background_job_dependencies",
  "public.background_job_outputs",
  "public.background_job_retry_receipts",
  "public.background_jobs",
  "public.background_project_slots",
  "public.background_synthesis_inputs",
  "public.background_work_versions",
  "public.code_references",
  "public.code_symbols",
  "public.curation_runs",
  "public.external_effect_outcomes",
  "public.external_effects",
  "public.federation_accepted",
  "public.federation_action_requests",
  "public.federation_audit",
  "public.federation_expertise_attempts",
  "public.federation_expertise_collection_audit",
  "public.federation_expertise_collection_grants",
  "public.federation_expertise_collections",
  "public.federation_expertise_daily_budget",
  "public.federation_expertise_deliveries",
  "public.federation_expertise_mailbox",
  "public.federation_expertise_origin_results",
  "public.federation_expertise_request_audit",
  "public.federation_expertise_requests",
  "public.federation_expertise_reviews",
  "public.federation_gateway_spaces",
  "public.federation_inbox",
  "public.federation_local_grants",
  "public.federation_publication_reviews",
  "public.federation_publish_outbox",
  "public.federation_replica_receipts",
  "public.federation_replica_trust",
  "public.federation_replicas",
  "public.federation_retention_decisions",
  "public.federation_review_requests",
  "public.federation_security",
  "public.federation_source_changes",
  "public.federation_source_outbox",
  "public.federation_source_subscriptions",
  "public.foreground_executions",
  "public.graph_edges",
  "public.graph_evidence",
  "public.graph_extraction_runs",
  "public.graph_nodes",
  "public.imported_memory_entries",
  "public.learned_skills",
  "public.loop_definitions",
  "public.loop_global_state",
  "public.loop_produced_memory",
  "public.loop_runs",
  "public.loop_setting_audit",
  "public.loop_settings",
  "public.memory_access_events",
  "public.memory_bundle_imports",
  "public.memory_canonical_state",
  "public.memory_chunk_embeddings",
  "public.memory_chunks",
  "public.memory_entries",
  "public.memory_proposals",
  "public.memory_relations",
  "public.memory_replacement_proposals",
  "public.memory_score_audit",
  "public.memory_scores",
  "public.memory_source_checks",
  "public.memory_sources",
  "public.memory_tags",
  "public.memory_validation_evidence",
  "public.memory_validation_runs",
  "public.procedural_utility",
  "public.procedural_utility_audit",
  "public.project_commits",
  "public.project_timeline_events",
  "public.projects",
  "public.proposal_creation_receipts",
  "public.raw_captures",
  "public.run_traces",
  "public.sessions",
  "public.supervisor_action_results",
  "public.supervisor_actions",
  "public.supervisor_attempt_states",
  "public.supervisor_attempts",
  "public.supervisor_authorities",
  "public.supervisor_budget_days",
  "public.supervisor_health_observations",
  "public.supervisor_health_schedule",
  "public.supervisor_instance_binding",
  "public.supervisor_journal",
  "public.supervisor_journal_heads",
  "public.supervisor_model_contexts",
  "public.supervisor_policies",
  "public.supervisor_proposal_reviews",
  "public.supervisor_proposals",
  "public.supervisor_question_denials",
  "public.supervisor_questions",
  "public.supervisor_settlements",
  "public.tasks",
  "public.timeline_position",
  "public.timeline_retention_watermarks",
  "public.trigger_events"
];
const AREAS = ['general', 'homelab', 'trust', 'workshop'];
const COPY_TABLES = ['public.projects', 'public.memory_entries', 'public.memory_canonical_state', 'public.memory_tags', 'public.memory_relations'];
const RECEIPT_TABLES = ['public.area_import_receipts', 'public.area_import_rows', 'public.area_import_provenance'];
const sha = /^[0-9a-f]{64}$/;
const uuid = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const hash = value => createHash('sha256').update(value).digest('hex');
const identity = (table, key) => table + '\0' + key;
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const fail = () => { throw Error('Inactive staging verification failed; private diagnostics suppressed'); };
function exact(value, keys) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
    || !same(Object.keys(value).sort(), [...keys].sort())) fail();
}
function binding(value) {
  exact(value, ['path', 'sha256']);
  if (!isAbsolute(value.path ?? '') || !sha.test(value.sha256)) fail();
}
export function validateContract(c) {
  exact(c, ['schema_version', 'kind', 'prepared', 'manifest', 'archive', 'targets']);
  if (c.schema_version !== 1 || c.kind !== 'four_inactive_staging' || !Array.isArray(c.targets) || c.targets.length !== 4) fail();
  for (const name of ['prepared', 'manifest', 'archive']) binding(c[name]);
  for (const t of c.targets) {
    exact(t, ['area', 'target', 'container_id', 'container_name', 'image_id', 'operation_id']);
    binding(t.target);
    if (!AREAS.includes(t.area) || !sha.test(t.container_id) || !/^sha256:[0-9a-f]{64}$/.test(t.image_id)
      || !uuid.test(t.operation_id) || !new RegExp('^memory-four-area-staging-[a-z0-9-]+-' + t.area + '$').test(t.container_name)) fail();
  }
  for (const field of ['area', 'container_id', 'container_name', 'operation_id']) if (new Set(c.targets.map(t => t[field])).size !== 4) fail();
  if (new Set([...['prepared','manifest','archive'].map(k => c[k].path), ...c.targets.map(t => t.target.path)]).size !== 7) fail();
  return c;
}
export function validateTarget(t, target, metadata, sourceInstance) {
  exact(target, ['format_version', 'instance_id', 'area', 'connection', 'database', 'username', 'password']);
  exact(target.connection, ['kind', 'directory', 'port']);
  if (target.format_version !== 1 || target.area !== t.area || !uuid.test(target.instance_id)
    || target.instance_id === sourceInstance || target.database !== 'memory_staging_' + t.area
    || target.username !== 'postgres' || target.password !== null || target.connection.kind !== 'unix'
    || target.connection.port !== 5432 || !isAbsolute(target.connection.directory ?? '')) fail();
  if (metadata.Id !== t.container_id || metadata.Name !== '/' + t.container_name || metadata.Image !== t.image_id
    || !metadata.State?.Running || metadata.HostConfig?.NetworkMode !== 'none' || !metadata.HostConfig?.ReadonlyRootfs
    || metadata.HostConfig?.Privileged || metadata.HostConfig?.PidMode === 'host' || metadata.HostConfig?.IpcMode === 'host'
    || (metadata.HostConfig?.CapAdd ?? []).length || Object.keys(metadata.HostConfig?.PortBindings ?? {}).length
    || Object.keys(metadata.NetworkSettings?.Networks ?? {}).some(n => n !== 'none')
    || Object.values(metadata.NetworkSettings?.Ports ?? {}).some(p => p !== null)) fail();
  const socket = (metadata.Mounts ?? []).filter(m => m.Destination === '/var/run/postgresql');
  if (socket.length !== 1 || socket[0].Type !== 'bind' || socket[0].Source !== target.connection.directory) fail();
  if ((metadata.Mounts ?? []).some(m => ['/', '/proc', '/sys', '/var/run/docker.sock', '/run/docker.sock'].includes(m.Source))) fail();
}
async function bound(file, limit, json) {
  binding(file);
  const value = await privateInput(file.path, limit, json);
  if (value.sha256 !== file.sha256) fail();
  return value.value;
}
export function makeExpected(prepared, manifest) {
  if (prepared.format_version !== 1 || manifest.format_version !== 2
    || prepared.source_instance !== manifest.source_instance || prepared.snapshot_id !== manifest.snapshot_id
    || prepared.inventory_sha256 !== manifest.inventory_sha256 || !sha.test(prepared.manifest_sha256)
    || !uuid.test(prepared.source_instance) || !uuid.test(prepared.snapshot_id)
    || !same([...manifest.areas].sort(), AREAS) || prepared.partitions.length !== 4) fail();
  const expected = new Map(AREAS.map(area => [area, { rows: new Map(), provenance: new Map(), histories: new Set(), projects: new Set(), versions: 0, tombstones: 0 }]));
  const owners = new Map();
  for (const lineage of manifest.lineages) {
    if (!uuid.test(lineage.canonical_id) || owners.has(lineage.canonical_id)) fail();
    if (!['area', 'protected_archive'].includes(lineage.disposition.kind)) fail();
    const area = lineage.disposition.kind === 'area' ? lineage.disposition.area : null;
    if (area && !expected.has(area)) fail();
    owners.set(lineage.canonical_id, area);
    if (area) expected.get(area).histories.add(lineage.canonical_id);
  }
  for (const p of manifest.projects) {
    if (!expected.has(p.area) || !uuid.test(p.source.id) || expected.get(p.area).projects.has(p.source.id)) fail();
    expected.get(p.area).projects.add(p.source.id);
  }
  const relationOwners = new Map();
  for (const r of manifest.records) {
    if (r.disposition.kind === 'area') {
      if (r.record.table !== 'public.memory_relations' || !expected.has(r.disposition.area)) fail();
      if (relationOwners.has(r.record.key)) fail();
      relationOwners.set(r.record.key, r.disposition.area);
    } else if (r.disposition.kind !== 'protected_archive') fail();
  }
  const projections = new Map();
  for (const p of manifest.provenance_projections) {
    const b = p.binding;
    if (p.format_version !== 1 || p.content.kind !== 'archive_reference_only'
      || b.source.record.table !== 'public.memory_sources' || !expected.has(b.destination_area)
      || owners.get(b.canonical_id) !== b.destination_area || b.source_instance !== prepared.source_instance
      || b.snapshot_id !== prepared.snapshot_id || b.inventory_sha256 !== prepared.inventory_sha256
      || !sha.test(p.operator_attestation.decision_sha256) || projections.has(b.source.record.key)) fail();
    projections.set(b.source.record.key, p);
  }
  return { prepared, expected, owners, relationOwners, projections, seenProjections: new Set(), sourceRows: 0, sourceTables: new Set(), lastRecord: null };
}
// Only the fixed shallow projection schema is encoded here; arbitrary PostgreSQL
// jsonb/numeric canonicalization remains PostgreSQL's responsibility.
export function projectedSource(source) {
  const fields = ['id','memory_entry_id','task_id','file_path','git_commit','source_kind','excerpt','created_at',
    'symbol_name','symbol_kind','line_start','line_end','content_hash','target_memory_id'];
  exact(source, fields);
  if (!uuid.test(source.id) || !uuid.test(source.memory_entry_id) || typeof source.created_at !== 'string') fail();
  const target = Object.fromEntries(fields.map(k => [k, ['id','memory_entry_id','created_at'].includes(k) ? source[k] : k === 'source_kind' ? 'note' : null]));
  const names = fields.sort((a, b) => Buffer.byteLength(a) - Buffer.byteLength(b) || Buffer.compare(Buffer.from(a), Buffer.from(b)));
  return '{' + names.map(k => JSON.stringify(k) + ': ' + JSON.stringify(target[k])).join(', ') + '}';
}
export function consumeOriginal(state, event) {
  if (event.event !== 'original_row' || typeof event.row_json !== 'string' || !sha.test(event.source_sha256)
    || hash(event.row_json) !== event.source_sha256 || typeof event.record?.table !== 'string' || typeof event.record.key !== 'string') fail();
  const key = identity(event.record.table, event.record.key);
  if (state.lastRecord && Buffer.compare(Buffer.from(state.lastRecord), Buffer.from(key)) >= 0) fail();
  state.lastRecord = key; state.sourceRows++; state.sourceTables.add(event.record.table);
  const area = event.destination?.area, projectedArea = event.destination?.archive_and_project_provenance;
  const destinations = new Set();
  if (area) {
    if (!COPY_TABLES.includes(event.record.table) || !state.expected.has(area)) fail();
    if (event.record.table === 'public.memory_relations') {
      if (state.relationOwners.get(event.record.key) !== area) fail();
    } else if (state.owners.get(event.canonical_id) !== area) fail();
    destinations.add(area);
  }
  if (event.record.table === 'public.projects') for (const [a, e] of state.expected) if (e.projects.has(event.record.key)) destinations.add(a);
  for (const a of destinations) {
    const e = state.expected.get(a), value = JSON.parse(event.row_json);
    if (e.rows.has(key)) fail();
    if (event.record.table === 'public.memory_entries') {
      if (value.canonical_id !== event.canonical_id || !e.projects.has(value.project_id)) fail();
      e.versions++; if (value.is_tombstone) e.tombstones++;
    }
    if (event.record.table === 'public.memory_relations'
      && (state.owners.get(value.src_canonical_id) !== a || state.owners.get(value.dst_canonical_id) !== a)) fail();
    e.rows.set(key, { source: event.source_sha256, target: event.source_sha256, transformed: false });
  }
  if (projectedArea) {
    const p = state.projections.get(event.record.key), b = p?.binding;
    if (event.record.table !== 'public.memory_sources' || !p || b.destination_area !== projectedArea || b.source.row_sha256 !== event.source_sha256) fail();
    const source = JSON.parse(event.row_json);
    if (source.memory_entry_id !== b.memory_version.record.key || source.task_id !== b.archived_task.record.key || source.target_memory_id !== null) fail();
    const e = state.expected.get(projectedArea), target = hash(projectedSource(source));
    if (e.rows.has(key) || state.seenProjections.has(event.record.key)) fail();
    state.seenProjections.add(event.record.key);
    e.rows.set(key, { source: event.source_sha256, target, transformed: true });
    e.provenance.set(event.record.key, { source_id: event.record.key, decision_id: p.decision_id,
      decision_sha256: p.operator_attestation.decision_sha256, canonical_id: b.canonical_id,
      memory_version_id: b.memory_version.record.key, source_sha256: event.source_sha256,
      original_version_sha256: b.memory_version.row_sha256, archived_task_id: b.archived_task.record.key,
      archived_task_sha256: b.archived_task.row_sha256, target_sha256: target });
  }
}
export function finishArchive(state) {
  if (state.sourceRows !== state.prepared.source_rows || state.seenProjections.size !== state.projections.size) fail();
  for (const [area, e] of state.expected) {
    const summaries = state.prepared.partitions.filter(p => p.area === area);
    if (summaries.length !== 1 || e.rows.size !== summaries[0].rows + summaries[0].projections || e.provenance.size !== summaries[0].projections) fail();
    for (const canonical of e.histories) if (!e.rows.has(identity('public.memory_canonical_state', canonical))) fail();
    for (const project of e.projects) if (!e.rows.has(identity('public.projects', project))) fail();
  }
}
export async function readArchive(file, state) {
  await bound(file, Infinity, false);
  const input = await open(file.path, constants.O_RDONLY | constants.O_NOFOLLOW);
  const bytes = createHash('sha256'); let size = 0, header = false, complete = false;
  try {
    const raw = input.createReadStream({ autoClose: false });
    async function* hashed() { for await (const chunk of raw) { bytes.update(chunk); size += chunk.length; yield chunk; } }
    for await (const line of streamLines(hashed())) {
      if (!line) fail();
      const e = JSON.parse(line);
      if (!header) {
        if (e.event !== 'header' || e.format_version !== 1 || e.serialization_id !== 'postgresql-to-jsonb-utf8-utc-v1') fail();
        for (const k of ['source_instance','snapshot_id','inventory_sha256','manifest_sha256']) if (e[k] !== state.prepared[k]) fail();
        state.sourceMigration = e.source_migration_version; header = true;
      } else if (complete) fail();
      else if (e.event === 'complete') {
        if (e.row_count !== state.prepared.source_rows || e.table_count !== state.prepared.source_tables) fail();
        complete = true;
      } else consumeOriginal(state, e);
    }
    if (!complete || size !== state.prepared.archive_bytes || bytes.digest('hex') !== file.sha256) fail();
    finishArchive(state);
  } finally { await input.close(); }
}
export function stagingSession(container, database) {
  if (!sha.test(container) || !/^memory_staging_(?:general|homelab|trust|workshop)$/.test(database)) fail();
  const child = spawn('docker', ['exec','-i',container,'env','-i','PATH=/usr/bin:/bin','psql','-X','-qAt','-v','ON_ERROR_STOP=1',
    '-h','/var/run/postgresql','-U','postgres','-d',database], { stdio: ['pipe','pipe','pipe'] });
  child.stderr.resume(); child.stdin.on('error', () => {});
  const done = new Promise(r => { child.once('error', () => r(-1)); child.once('close', r); });
  return { lines: streamLines(child.stdout), done, send(sql) { if (child.stdin.destroyed) fail(); child.stdin.write(sql); },
    abort() { child.stdin.destroy(); child.kill('SIGTERM'); child.stdout.destroy(); } };
}
const seedCounts = { 'public.supervisor_authorities': 4, 'public.supervisor_journal_heads': 4, 'public.timeline_position': 1, 'public.loop_global_state': 1 };
export function targetSql(tables) {
  if (!same(tables.map(t => t.schema + '.' + t.name).sort(), SCHEMA60_TABLES) || tables.some(t => t.kind !== 'r' || t.is_partition)) fail();
  const sql = ["SET LOCAL statement_timeout='120s';"];
  sql.push(`SELECT json_build_object('event','safety','database',current_database(),'read_only',current_setting('transaction_read_only'),
    'clients',(SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND backend_type='client backend'),
    'migrations',(SELECT count(*) FROM public._sqlx_migrations),'migration_head',(SELECT max(version) FROM public._sqlx_migrations),
    'failed_migrations',(SELECT count(*) FROM public._sqlx_migrations WHERE NOT success OR version NOT BETWEEN 1 AND 60),
    'invalid_indexes',(SELECT count(*) FROM pg_index WHERE NOT indisvalid),
    'invalid_constraints',(SELECT count(*) FROM pg_constraint WHERE NOT convalidated),
    'unsafe_seeds',(SELECT count(*) FROM public.supervisor_authorities WHERE current_revision IS NOT NULL)
      +(SELECT count(*) FROM public.timeline_position WHERE position<>0)
      +(SELECT count(*) FROM public.loop_global_state WHERE NOT kill_switch_enabled OR updated_by IS NOT NULL OR reason IS NOT NULL),
    'broken_links',(SELECT count(*) FROM public.memory_relations r LEFT JOIN public.memory_canonical_state a ON a.canonical_id=r.src_canonical_id
      LEFT JOIN public.memory_canonical_state b ON b.canonical_id=r.dst_canonical_id WHERE a.canonical_id IS NULL OR b.canonical_id IS NULL)
      +(SELECT count(*) FROM public.memory_entries e LEFT JOIN public.memory_canonical_state s ON s.canonical_id=e.canonical_id WHERE s.canonical_id IS NULL)
    )::text;`);
  for (const t of tables) {
    const name = t.schema + '.' + t.name, q = identifier(t.schema) + '.' + identifier(t.name);
    sql.push("SELECT json_build_object('event','count','table'," + literal(name) + ",'count',(SELECT count(*) FROM ONLY " + q + "))::text;");
    if (COPY_TABLES.includes(name) || name === 'public.memory_sources') sql.push("SELECT json_build_object('event','row','table'," + literal(name)
      + ",'key'," + keyExpression(t,'r') + ",'sha256',encode(public.digest(to_jsonb(r)::text,'sha256'),'hex'))::text FROM ONLY " + q + " r;");
    if (RECEIPT_TABLES.includes(name)) sql.push("SELECT json_build_object('event'," + literal(name.slice(7))
      + ",'row',to_jsonb(r))::text FROM ONLY " + q + " r;");
  }
  sql.push("COMMIT; SELECT json_build_object('event','complete')::text;\n\\q\n");
  return sql.join('\n');
}
export async function verifyDatabase(io, spec, target, state) {
  const expected = state.expected.get(spec.area), partition = state.prepared.partitions.find(p => p.area === spec.area);
  const counts = new Map(), seen = new Set(), receipts = new Set(), provenance = new Set();
  let catalog = false, safety = false, receipt = false, complete = false;
  io.send(BEGIN_SQL);
  try {
    for await (const line of io.lines) {
      if (!line.trim()) continue;
      const e = JSON.parse(line);
      if (complete) fail();
      if (e.event === 'catalog' && !catalog) { catalog = true; io.send(targetSql(e.tables)); }
      else if (e.event === 'safety' && catalog && !safety) {
        if (e.database !== target.database || e.read_only !== 'on' || e.clients !== 0 || e.migrations !== 60 || e.migration_head !== 60
          || ['failed_migrations','invalid_indexes','invalid_constraints','unsafe_seeds','broken_links'].some(k => e[k] !== 0)) fail();
        safety = true;
      } else if (e.event === 'count' && safety) {
        if (!SCHEMA60_TABLES.includes(e.table) || counts.has(e.table) || !Number.isSafeInteger(e.count) || e.count < 0) fail();
        counts.set(e.table, e.count);
      } else if (e.event === 'row' && safety) {
        const key = identity(e.table,e.key), row = expected.rows.get(key);
        if (!row || row.target !== e.sha256 || seen.has(key)) fail(); seen.add(key);
      } else if (e.event === 'area_import_receipts' && safety && !receipt) {
        const r = e.row;
        const pairs = { id:spec.operation_id, target_instance:target.instance_id, area:spec.area, source_instance:state.prepared.source_instance,
          snapshot_id:state.prepared.snapshot_id, inventory_sha256:state.prepared.inventory_sha256, manifest_sha256:state.prepared.manifest_sha256,
          partition_sha256:partition.partition_sha256, row_count:expected.rows.size, projection_count:expected.provenance.size,
          source_migration:state.sourceMigration, target_migration:60, singleton:true };
        if (Object.entries(pairs).some(([k,v]) => r[k] !== v)) fail(); receipt = true;
      } else if (e.event === 'area_import_rows' && safety) {
        const r = e.row, key = identity(r.table_name,r.record_key), row = expected.rows.get(key);
        if (!row || receipts.has(key) || r.import_id !== spec.operation_id || r.source_sha256 !== row.source
          || r.target_sha256 !== row.target || r.transformed !== row.transformed) fail(); receipts.add(key);
      } else if (e.event === 'area_import_provenance' && safety) {
        const r = e.row, p = expected.provenance.get(r.source_id);
        if (!p || provenance.has(r.source_id) || r.import_id !== spec.operation_id || Object.entries(p).some(([k,v])=>r[k]!==v)) fail();
        provenance.add(r.source_id);
      } else if (e.event === 'complete' && safety) complete = true;
      else fail();
    }
    if (await io.done !== 0 || !complete || !receipt || seen.size !== expected.rows.size || receipts.size !== expected.rows.size
      || provenance.size !== expected.provenance.size || counts.size !== SCHEMA60_TABLES.length) fail();
    const expectedCounts = new Map(SCHEMA60_TABLES.map(t=>[t,seedCounts[t]??0]));
    expectedCounts.set('public._sqlx_migrations',60);
    expectedCounts.set('public.area_import_receipts',1);
    expectedCounts.set('public.area_import_rows',expected.rows.size);
    expectedCounts.set('public.area_import_provenance',expected.provenance.size);
    for (const key of expected.rows.keys()) { const t=key.split('\0')[0]; expectedCounts.set(t,expectedCounts.get(t)+1); }
    if ([...counts].some(([table,count])=>count!==expectedCounts.get(table))) fail();
    return { area:spec.area, target_instance:target.instance_id, container_id:spec.container_id,
      histories:expected.histories.size, versions:expected.versions, tombstones:expected.tombstones, projects:expected.projects.size,
      relationships:counts.get('public.memory_relations'), projected_sources:expected.provenance.size, verified_rows:seen.size,
      row_and_receipt_hashes_match:true, authority_and_runtime_inactive:true, table_counts:Object.fromEntries([...counts].filter(([,n])=>n)) };
  } catch { io.abort(); fail(); }
}
export async function verifyFleet(contractFile, contractSha, output, inspect = inspectContainer) {
  if (!sha.test(contractSha) || !isAbsolute(output)) fail();
  const c = validateContract(await bound({path:contractFile,sha256:contractSha},64*1024,true));
  const prepared = await bound(c.prepared,1024*1024,true);
  const manifest = await bound(c.manifest,256*1024*1024,true);
  if (prepared.manifest_file_sha256 !== c.manifest.sha256 || prepared.archive_sha256 !== c.archive.sha256) fail();
  const state = makeExpected(prepared,manifest);
  const targets=[];
  for (const spec of c.targets) {
    const target=await bound(spec.target,64*1024,true); validateTarget(spec,target,await inspect(spec.container_id),prepared.source_instance);
    targets.push({spec,target});
  }
  if(new Set(targets.map(t=>t.target.instance_id)).size!==4)fail();
  await readArchive(c.archive,state);
  const results=[];
  for(const {spec,target} of targets) {
    validateTarget(spec,target,await inspect(spec.container_id),prepared.source_instance);
    results.push(await verifyDatabase(stagingSession(spec.container_id,target.database),spec,target,state));
    validateTarget(spec,target,await inspect(spec.container_id),prepared.source_instance);
  }
  for(const file of [c.prepared,c.manifest,...c.targets.map(t=>t.target),{path:contractFile,sha256:contractSha}]) await bound(file,Infinity,false);
  const report={schema_version:1,status:'four_inactive_partitions_verified_not_activated',observed_at:new Date().toISOString(),
    contract_sha256:contractSha,prepared_sha256:c.prepared.sha256,manifest_file_sha256:c.manifest.sha256,
    manifest_sha256:prepared.manifest_sha256,archive_sha256:c.archive.sha256,source_instance:prepared.source_instance,snapshot_id:prepared.snapshot_id,
    targets:results,application_checks_passed:false,cutover_ready:false,
    limitations:['Read-only point-in-time checks require continued writer exclusion.','No service, retrieval or UI is started on an import target.',
      'Source row integrity and reviewed ownership are bound to the prepared archive, not independently re-approved.',
      'Projected provenance is reference-only; raw context stays archived.','Use separately approved disposable clones for application-level checks.']};
  return privateArtifact(output,async write=>{await write(JSON.stringify(report,null,2)+'\n');return {status:report.status,areas:results.map(({area,histories,versions,relationships})=>({area,histories,versions,relationships})),cutover_ready:false};});
}
export function parseArgs(args) {
  const result={};for(let i=0;i<args.length;i+=2) {
    if(!['--contract','--contract-sha256','--output'].includes(args[i])||!args[i+1]||result[args[i]])fail();
    result[args[i]]=args[i+1];
  }
  if(Object.keys(result).length!==3||!isAbsolute(result['--contract'])||!isAbsolute(result['--output'])||!sha.test(result['--contract-sha256']))fail();
  return result;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try { const a=parseArgs(process.argv.slice(2));console.log(JSON.stringify(await verifyFleet(a['--contract'],a['--contract-sha256'],a['--output']))); }
  catch { console.error('Inactive staging verification failed. No private rows or database diagnostics disclosed; keep targets inactive and inspect exact bindings.');process.exitCode=1; }
}
