#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Private human review aid. Proposals are never approved manifest decisions.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { open, lstat, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { DATABASE, SERIALIZATION, privateArtifact, streamLines, inspectSandbox, CATALOG_SQL } from './inventory.mjs';

export const LIMITS = { entry_excerpt_chars: 1600, summary_chars: 600, source_previews: 6, source_excerpt_chars: 300, path_chars: 500, title_chars: 180, manifest_bytes: 128 * 1024 * 1024, review_pack_bytes: 128 * 1024 * 1024, lineage_count: 20_000 };
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const sha256 = /^[0-9a-f]{64}$/;
export const HELP = `Usage: node scripts/areas/classify.mjs --descriptor /protected/restore-descriptor.json --inventory /protected/partition-inventory.json --manifest /protected/unresolved-manifest.json --validation /protected/inventory-validation.json --restore-evidence /protected/restore-evidence.json --output /protected/new-review-pack.json

Reads ONLY the exact digest-bound, network-none verified restore descriptor.
Uses no provider, credentials, keychain, network service, live source DB, or importer.
Reads all remaining canonical versions and source links in one read-only snapshot.
Writes a bounded private human review pack; stdout contains aggregate counts only.
Input files must be owner-only regular files; output parent must be mode0700.
Output is mode0600, exclusive, and is NOT an approved migration manifest.
Candidate routing rules are cautious review aids, not evidence of approved ownership.
`;

export function parseArgs(args) {
  const options = {};
  for (let i = 0; i < args.length; i += 2) {
    const name = args[i]?.slice(2);
    if (!['descriptor', 'inventory', 'manifest', 'validation', 'restore-evidence', 'output'].includes(name)
      || args[i] !== `--${name}` || !args[i + 1]?.startsWith('/') || options[name]) throw Error('Explicit unique absolute input/output paths are required');
    options[name] = resolve(args[i + 1]);
  }
  if (Object.keys(options).length !== 6 || new Set(Object.values(options)).size !== 6) throw Error('All six distinct descriptor/input/output paths are required');
  return options;
}

async function privateFile(path, maxBytes = Infinity) {
  const info = await lstat(path);
  if (!info.isFile() || info.uid !== process.getuid() || (info.mode & 0o077) !== 0 || info.size > maxBytes) throw Error('Input must be a bounded owner-only regular file');
  return info;
}

async function jsonFile(path, maxBytes) {
  await privateFile(path, maxBytes);
  try { return JSON.parse(await readFile(path, 'utf8')); } catch { throw Error('Invalid private evidence JSON'); }
}

async function hashFile(path) {
  await privateFile(path);
  const hash = createHash('sha256');
  const file = await open(path, 'r');
  for await (const chunk of file.createReadStream()) hash.update(chunk);
  return hash.digest('hex');
}

export async function loadBindings(options) {
  const [collector, validation, restore, manifest] = await Promise.all([
    jsonFile(`${options.inventory}.collector.json`, 4 * 1024 * 1024),
    jsonFile(options.validation, 16 * 1024 * 1024), jsonFile(options['restore-evidence'], 4 * 1024 * 1024),
    jsonFile(options.manifest, LIMITS.manifest_bytes),
  ]);
  if (manifest.format_version !== 1 || !uuid.test(manifest.source_instance) || !uuid.test(manifest.snapshot_id)
    || !sha256.test(manifest.inventory_sha256) || !Array.isArray(manifest.lineages)
    || manifest.lineages.length > LIMITS.lineage_count || validation.inventory_valid !== true
    || manifest.inventory_sha256 !== validation.inventory_sha256 || restore.all_table_row_hashes_match !== true
    || restore.container_id !== collector.sandbox?.container_id || restore.database_archive_sha256 === undefined
    || manifest.source_instance !== collector.source_instance || manifest.snapshot_id !== collector.snapshot_id) throw Error('Source snapshot, manifest, validation and restore evidence do not agree');
  const ids = new Set(manifest.lineages.map(lineage => lineage.canonical_id));
  if (ids.size !== manifest.lineages.length || [...ids].some(id => !uuid.test(id))) throw Error('Invalid or duplicate manifest canonical identities');
  const inventoryHash = await hashFile(options.inventory);
  if (inventoryHash !== collector.inventory_file_sha256) throw Error('Inventory file digest changed');
  const versions = restore.restored_table_inventory.find(table => table.table === 'public.memory_entries')?.rows;
  if (!Number.isSafeInteger(versions)) throw Error('Restore evidence lacks version-row reconciliation');
  return { canonical_ids: ids, expected_versions: versions, expected_projects: validation.project_count,
    container_id: restore.container_id,
    evidence: { source_instance: manifest.source_instance, snapshot_id: manifest.snapshot_id,
      inventory_file_sha256: inventoryHash, inventory_semantic_sha256: manifest.inventory_sha256,
      reviewed_input_manifest_sha256: await hashFile(options.manifest),
      inventory_validation_sha256: await hashFile(options.validation),
      restore_evidence_sha256: await hashFile(options['restore-evidence']),
      database_archive_sha256: restore.database_archive_sha256, row_serialization: SERIALIZATION } };
}

// Topic markers are evidence for review, never ownership instructions. In
// particular, using Memory Layer does not make a different project Workshop.
const rules = {
  trust: [ ['certificate_authority', /\bcertificate authorit(?:y|ies)\b/i], ['step_ca', /\bstep[- ]ca\b/i], ['ca_signing', /\b(?:root|intermediate) CA\b|\bCA (?:signing|bootstrap|provisioner)\b/], ['pki', /\bPKI\b/] ],
  homelab: [ ['homelab', /\bhome[ -]?lab\b/i], ['monolith_host', /\bMonolith\b/], ['host_operations', /\b(?:Proxmox|TRMNL|PipeWire|NVIDIA|Authentik)\b/i] ],
  workshop: [ ['memory_layer', /\bMemory Layer\b|\bmemory-layer\b/i], ['memory_crates', /\bmem-(?:service|cli|record|store|federation|curate|search|eval|supervisor|area)\b|crates\/mem-/i], ['memory_repo', /\/Projects\/memory(?:\/|\b)/] ],
};
const evaluationProject = /(?:^|[\s/\\_.:-])(?:eval(?:uation)?|benchmark|mab|smoke|fixtures?|tests?)(?=$|[\s/\\_.:-])/i;
// Only a narrow Homelab authority-operator proposal may override the original
// project area. Certificate consumers and negated instructions stay put.
const authorityOperation = /\b(?:configure|operate|bootstrap|restore|back\s?up|rotate|deploy|maintain|manage)\b[^.\n]{0,90}\b(?:step[- ]ca|(?:root|intermediate) CA|certificate authority)\b|\b(?:step[- ]ca|(?:root|intermediate) CA)\b[^.\n]{0,90}\b(?:signing|provisioner|issuance|backup|restoration|rotation)\b/i;
const certificateConsumer = /\b(?:Turris|certificate consumer|(?:admin|web|application) (?:UI|interface|dashboard)|HTTPS (?:UI|interface|dashboard)|(?:client|server|browser|leaf) certificate|certificate (?:issued by|signed by))\b/i;
const negatedAuthorityOperation = /\b(?:never|not|avoid|forbid|without|mustn't|don't|do not)\b[^.\n]{0,100}\b(?:step[- ]ca|(?:root|intermediate) CA|certificate authority|CA (?:signing|bootstrap|provisioner))\b/i;
const syntheticContent = /\b(?:synthetic (?:fixture|record|memory|dataset)|evaluation[- ]only|benchmark fixture|test fixture (?:only|data)|fabricated (?:record|memory))\b/i;
const secretPatterns = [ /-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----[\s\S]*?(?:-----END (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----|$)/g,
  /\bsk-(?:proj-)?[A-Za-z0-9_-]{24,}\b/g,
  /\b(?:password|api[_-]?key|client[_-]?secret|access[_-]?token|refresh[_-]?token)\s*[:=]\s*["']?[A-Za-z0-9_+\/=.-]{12,}["']?/gi,
  /\bBearer\s+[A-Za-z0-9_.~+\/-]{12,}/gi,
  /\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/g,
  /\b(?:postgres(?:ql)?|https?):\/\/[^\s\/@]+:[^\s\/@]+@[^\s]+/gi ];

export function redact(value) {
  let text = String(value ?? '');
  let redacted = false;
  for (const pattern of secretPatterns) text = text.replace(pattern, () => { redacted = true; return '[REDACTED: possible secret material]'; });
  return { text, redacted };
}

function excerpt(value, limit) {
  const safe = redact(value);
  return { text: safe.text.slice(0, limit), original_chars: String(value ?? '').length,
    abbreviated: safe.text.length > limit, secret_pattern_redacted: safe.redacted };
}

function redactFields(value, state) {
  if (typeof value === 'string') {
    const safe = redact(value);
    state.redacted ||= safe.redacted;
    return safe.text;
  }
  if (Array.isArray(value)) return value.map(item => redactFields(item, state));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([name, item]) => [name, redactFields(item, state)]));
  return value;
}

function contentSignals(text) {
  return Object.entries(rules).flatMap(([area, markers]) => markers.filter(([, pattern]) => pattern.test(text)).map(([rule]) => ({ area, rule })));
}

function projectHint(project) {
  const slug = project.slug?.toLowerCase() ?? '';
  const root = project.root_path ?? '';
  if (slug === 'homelab' || /\/Projects\/homelab\/?$/.test(root)) return 'homelab';
  if (slug === 'memory' || slug === 'memory-layer' || /\/Projects\/memory\/?$/.test(root)) return 'workshop';
  // Other projects retain General boundaries; ca/ca-2 is not CA evidence.
  return null;
}

export function summarizeLineage(input) {
  if (!uuid.test(input.canonical_id) || !Array.isArray(input.versions) || !Array.isArray(input.sources) || !Array.isArray(input.projects)) throw Error('Invalid lineage envelope');
  const versions = [...input.versions].sort((a, b) => a.version_no - b.version_no);
  const latest = versions.at(-1);
  const readable = [...versions].reverse().find(version => !version.is_tombstone) ?? latest;
  const allContent = versions.map(version => `${version.summary ?? ''}\n${version.canonical_text ?? ''}`).join('\n');
  const signals = contentSignals(allContent);
  const contentAreas = [...new Set(signals.map(signal => signal.area))];
  const projectAreas = [...new Set(input.projects.map(project => projectHint(project) ?? 'general'))];
  const risks = [];
  const reasons = [];
  const sourceText = input.sources.map(source => `${source.file_path ?? ''}\n${source.excerpt ?? ''}`).join('\n');
  const sourceSignals = contentSignals(sourceText);
  const possibleSecret = redact(allContent).redacted || redact(sourceText).redacted;
  const evalProject = input.projects.some(project => evaluationProject.test(`${project.slug ?? ''} ${project.root_path ?? ''}`));
  const synthetic = syntheticContent.test(allContent);
  const projectIds = [...new Set(input.projects.map(project => project.id))];
  const misleadingCaName = input.projects.some(project => /^ca(?:-\d+)?$/i.test(project.slug ?? ''));
  if (misleadingCaName) reasons.push('A ca/ca-N project label is not certificate-authority evidence.');
  if (possibleSecret) risks.push('possible_secret_material');
  if (evalProject) risks.push('evaluation_or_test_project', 'project_marker_requires_false_positive_review');
  if (synthetic) risks.push('synthetic_content_marker');
  if (contentAreas.length > 1) risks.push('multiple_area_content_indicators');
  if (projectIds.length > 1) risks.push('multiple_original_projects');
  if (latest?.is_tombstone) risks.push('latest_version_is_tombstone');
  if (!versions.length) risks.push('canonical_state_without_remaining_versions');
  if (!input.sources.length) risks.push('no_linked_source_records');
  if (input.sources.some(source => source.task_id && !source.session_id)) risks.push('task_without_session_context');
  const originalArea = projectIds.length === 1 && projectAreas.length === 1 ? projectAreas[0] : null;
  const caTopic = contentAreas.includes('trust');
  const consumer = certificateConsumer.test(allContent);
  const negatedOperation = negatedAuthorityOperation.test(allContent);
  const authorityCandidate = originalArea === 'homelab' && caTopic && !contentAreas.includes('workshop')
    && authorityOperation.test(allContent) && !consumer && !negatedOperation;
  const candidateArea = allContent.trim() ? (authorityCandidate ? 'trust' : originalArea) : null;
  if (caTopic) risks.push('possible_certificate_authority_topic');
  if (caTopic && consumer) risks.push('certificate_consumer_not_authority_ownership');
  if (caTopic && negatedOperation) risks.push('negated_authority_operation_requires_review');
  if (signals.some(signal => signal.rule === 'memory_layer') && originalArea !== 'workshop') risks.push('memory_layer_use_does_not_establish_workshop_ownership');
  if (contentAreas.some(area => area !== originalArea) && !authorityCandidate) risks.push('content_topic_differs_from_original_project_area');
  let classification = 'knowledge';
  let disposition = 'area_candidate';
  if (possibleSecret || projectIds.length > 1 || contentAreas.length > 1 || !versions.length || candidateArea === null) {
    classification = contentAreas.length > 1 || projectIds.length > 1 ? 'mixed' : 'ambiguous';
    disposition = 'protected_archive_candidate';
  }
  if (evalProject || synthetic) {
    classification = evalProject ? 'evaluation' : 'ambiguous';
    disposition = 'protected_archive_candidate';
  }
  if (latest?.is_tombstone) disposition = 'protected_archive_candidate';
  if (authorityCandidate) reasons.push('Explicit CA operator markers in an original Homelab project support a possible Trust move; a human must distinguish operating the authority from consuming its certificates.');
  else if (candidateArea) reasons.push(`Original project provenance retains the tentative ${candidateArea} area and original project identity; topic keywords do not authorize relocation.`);
  if (caTopic && !authorityCandidate) reasons.push('CA/PKI mentions alone do not establish authority ownership; certificate consumers and negated operator guidance keep their original project area.');
  if (signals.some(signal => signal.rule === 'memory_layer') && originalArea !== 'workshop') reasons.push('Memory Layer may be used to initialize or remember work in another project; using the tool is not developing it.');
  if (evalProject) reasons.push('A delimited test/eval/smoke/fixture project marker creates a protective hold; a reviewer must explicitly resolve false positives such as ordinary projects named skyrim-test.');
  if (disposition === 'protected_archive_candidate') reasons.push('Retain the entire lineage in protected review/archive until the flagged uncertainty or sensitivity is resolved.');
  const provenanceSignals = sourceSignals.filter(signal => !signals.some(existing => existing.area === signal.area));
  if (candidateArea && provenanceSignals.some(signal => signal.area !== candidateArea)) risks.push('source_content_area_conflict');
  const versionNumbers = versions.map(version => version.version_no);
  const entry = {
    canonical_id: input.canonical_id,
    title: excerpt(readable?.summary?.trim() || readable?.canonical_text?.split('\n').find(line => line.trim()) || '(no remaining content)', LIMITS.title_chars),
    original_projects: input.projects,
    canonical_state: input.state,
    history: { version_count: versions.length, tombstone_count: versions.filter(version => version.is_tombstone).length,
      first_version: versionNumbers[0] ?? null, latest_version: versionNumbers.at(-1) ?? null,
      has_version_gaps: versionNumbers.some((number, index) => index && number !== versionNumbers[index - 1] + 1),
      versions: versions.map(({ id, project_id, version_no, is_tombstone, created_at, updated_at, row_sha256, memory_type, scope }) => ({ id, project_id, version_no, is_tombstone, created_at, updated_at, row_sha256, memory_type, scope })) },
    latest_readable_version_id: readable?.id ?? null,
    summary: excerpt(readable?.summary, LIMITS.summary_chars),
    content_excerpt: excerpt(readable?.canonical_text, LIMITS.entry_excerpt_chars),
    source_count: input.sources.length,
    source_references: input.sources.map(source => ({ id: source.id, memory_entry_id: source.memory_entry_id, row_sha256: source.row_sha256,
      task_id: source.task_id, session_id: source.session_id, target_memory_id: source.target_memory_id,
      capture_ids: source.capture_ids ?? [] })),
    source_previews: input.sources.slice(0, LIMITS.source_previews).map(source => ({ id: source.id, source_kind: source.source_kind,
      file_path: excerpt(source.file_path, LIMITS.path_chars), git_commit: source.git_commit, content_hash: source.content_hash,
      source_excerpt: excerpt(source.excerpt, LIMITS.source_excerpt_chars), task_title: excerpt(source.task_title, LIMITS.title_chars),
      writer_name: source.writer_name, writer_id: source.writer_id })),
    source_previews_omitted: Math.max(0, input.sources.length - LIMITS.source_previews),
    proposal: { suggested_disposition: disposition, candidate_area: candidateArea, source_classification: classification,
      ownership_approved: false, confidence: authorityCandidate ? 'authority_operator_markers_only' : candidateArea ? 'original_project_preserved' : 'unresolved',
      content_markers: signals, provenance_markers: sourceSignals, reasons, risks,
      review_required: ['Review all lineage versions and provenance before assigning ownership.', 'Keep supporting mixed captures/sessions archived unless a reviewed transformation is available.'] },
  };
  // Redact string values independently so malformed credential-like text can
  // never swallow JSON syntax or alter structural identity fields.
  const redactions = { redacted: false };
  const safe = redactFields(entry, redactions);
  safe.metadata_secret_pattern_redacted = redactions.redacted;
  if (redactions.redacted) {
    safe.proposal.suggested_disposition = 'protected_archive_candidate';
    safe.proposal.source_classification = 'ambiguous';
    if (!safe.proposal.risks.includes('possible_secret_material')) safe.proposal.risks.push('possible_secret_material');
  }
  return safe;
}

export function annotateMixedSources(entries) {
  const sessions = new Map();
  const tasks = new Map();
  for (const entry of entries) for (const source of entry.source_references) {
    for (const [index, id] of [[sessions, source.session_id], [tasks, source.task_id]]) {
      if (!id) continue;
      if (!index.has(id)) index.set(id, new Set());
      index.get(id).add(entry.canonical_id);
    }
  }
  const byId = new Map(entries.map(entry => [entry.canonical_id, entry]));
  const archiveCandidates = [];
  for (const [kind, index] of [['session', sessions], ['task', tasks]]) for (const [id, ids] of index) {
    const related = [...ids].map(id => byId.get(id));
    const areas = [...new Set(related.map(entry => entry.proposal.candidate_area).filter(Boolean))];
    const archiveAndArea = related.some(entry => entry.proposal.suggested_disposition === 'protected_archive_candidate')
      && related.some(entry => entry.proposal.suggested_disposition === 'area_candidate');
    if (areas.length <= 1 && !archiveAndArea) continue;
    archiveCandidates.push({ record_type: kind, id, canonical_ids: [...ids].sort(), candidate_areas: areas.sort(),
      suggested_disposition: 'protected_archive_candidate', approved: false,
      reason: 'Shared source links span area or retention candidates; raw source content was not inspected.' });
    for (const entry of related) {
      if (!entry.proposal.risks.includes('shared_source_may_mix_areas')) entry.proposal.risks.push('shared_source_may_mix_areas');
      entry.proposal.review_required.push(`Review ${kind} ${id}; preserve its raw captures in protected archive pending separation.`);
    }
  }
  return archiveCandidates;
}

export const REVIEW_SQL = `BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL timezone='UTC'; SET LOCAL datestyle='ISO, YMD'; SET LOCAL intervalstyle='postgres';
SET LOCAL extra_float_digits=3; SET LOCAL bytea_output='hex'; SET LOCAL row_security=off;
SET LOCAL search_path=pg_catalog; SET LOCAL standard_conforming_strings=on;
SELECT jsonb_build_object('event','header','captured_at',transaction_timestamp(),'read_only',current_setting('transaction_read_only'),
 'database',current_database(),'snapshot',txid_current_snapshot()::text,
 'canonical_count',(SELECT count(*) FROM (SELECT canonical_id FROM public.memory_entries UNION SELECT canonical_id FROM public.memory_canonical_state) ids),
 'version_count',(SELECT count(*) FROM public.memory_entries),'project_count',(SELECT count(*) FROM public.projects))::text;
WITH ids AS (SELECT canonical_id FROM public.memory_entries UNION SELECT canonical_id FROM public.memory_canonical_state)
SELECT jsonb_build_object('event','lineage','canonical_id',ids.canonical_id,
 'state',(SELECT to_jsonb(cs) FROM public.memory_canonical_state cs WHERE cs.canonical_id=ids.canonical_id),
 'projects',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM public.projects p WHERE p.id IN (
   SELECT project_id FROM public.memory_entries v WHERE v.canonical_id=ids.canonical_id UNION SELECT project_id FROM public.memory_canonical_state cs WHERE cs.canonical_id=ids.canonical_id)), '[]'::jsonb),
 'versions',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',v.id,'project_id',v.project_id,'version_no',v.version_no,'is_tombstone',v.is_tombstone,
   'summary',v.summary,'canonical_text',v.canonical_text,'memory_type',v.memory_type,'scope',v.scope,'created_at',v.created_at,'updated_at',v.updated_at,
   'row_sha256',encode(sha256(convert_to(to_jsonb(v)::text,'UTF8')),'hex')) ORDER BY v.version_no) FROM public.memory_entries v WHERE v.canonical_id=ids.canonical_id),'[]'::jsonb),
 'sources',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',s.id,'memory_entry_id',s.memory_entry_id,'task_id',s.task_id,'target_memory_id',s.target_memory_id,
   'file_path',s.file_path,'git_commit',s.git_commit,'source_kind',s.source_kind,'excerpt',s.excerpt,'content_hash',s.content_hash,
   'row_sha256',encode(sha256(convert_to(to_jsonb(s)::text,'UTF8')),'hex'),'task_title',t.title,'session_id',sess.id,'writer_name',sess.writer_name,'writer_id',sess.writer_id,
   'capture_ids',COALESCE((SELECT jsonb_agg(rc.id ORDER BY rc.id) FROM public.raw_captures rc WHERE rc.task_id=t.id),'[]'::jsonb)) ORDER BY s.id)
   FROM public.memory_sources s JOIN public.memory_entries v ON v.id=s.memory_entry_id LEFT JOIN public.tasks t ON t.id=s.task_id
   LEFT JOIN public.sessions sess ON sess.id=t.session_id WHERE v.canonical_id=ids.canonical_id),'[]'::jsonb))::text FROM ids ORDER BY ids.canonical_id;
COMMIT;
SELECT jsonb_build_object('event','complete')::text;
`;

// Extra metadata is computed locally in PostgreSQL. No capture/session/task
// body enters the review stream: only complete-row SHA-256 dependencies do.
const fullDependencies = `
 'state_row_sha256',(SELECT encode(sha256(convert_to(to_jsonb(cs)::text,'UTF8')),'hex') FROM public.memory_canonical_state cs WHERE cs.canonical_id=ids.canonical_id),
 'dependencies',COALESCE((SELECT jsonb_agg(jsonb_build_object('table',d.tbl,'key',d.key,'row_sha256',d.sha) ORDER BY d.tbl,d.key) FROM (
  SELECT 'public.memory_tags' tbl,jsonb_build_array(t.memory_entry_id,t.tag)::text key,encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') sha FROM public.memory_tags t JOIN public.memory_entries v ON v.id=t.memory_entry_id WHERE v.canonical_id=ids.canonical_id
  UNION ALL SELECT 'public.memory_source_checks',c.id::text,encode(sha256(convert_to(to_jsonb(c)::text,'UTF8')),'hex') FROM public.memory_source_checks c JOIN public.memory_sources s ON s.id=c.source_id JOIN public.memory_entries v ON v.id=s.memory_entry_id WHERE v.canonical_id=ids.canonical_id
  UNION ALL SELECT 'public.tasks',t.id::text,encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') FROM public.tasks t WHERE t.id IN (SELECT s.task_id FROM public.memory_sources s JOIN public.memory_entries v ON v.id=s.memory_entry_id WHERE v.canonical_id=ids.canonical_id)
  UNION ALL SELECT 'public.sessions',s.id::text,encode(sha256(convert_to(to_jsonb(s)::text,'UTF8')),'hex') FROM public.sessions s WHERE s.id IN (SELECT t.session_id FROM public.tasks t JOIN public.memory_sources ms ON ms.task_id=t.id JOIN public.memory_entries v ON v.id=ms.memory_entry_id WHERE v.canonical_id=ids.canonical_id)
  UNION ALL SELECT 'public.raw_captures',r.id::text,encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') FROM public.raw_captures r WHERE r.task_id IN (SELECT s.task_id FROM public.memory_sources s JOIN public.memory_entries v ON v.id=s.memory_entry_id WHERE v.canonical_id=ids.canonical_id)
  UNION ALL SELECT 'public.memory_relations',r.id::text,encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') FROM public.memory_relations r WHERE ids.canonical_id IN (r.src_canonical_id,r.dst_canonical_id)
  UNION ALL SELECT 'public.memory_canonical_state',c.canonical_id::text,encode(sha256(convert_to(to_jsonb(c)::text,'UTF8')),'hex') FROM public.memory_canonical_state c WHERE c.canonical_id IN (SELECT r.src_canonical_id FROM public.memory_relations r WHERE r.dst_canonical_id=ids.canonical_id UNION SELECT r.dst_canonical_id FROM public.memory_relations r WHERE r.src_canonical_id=ids.canonical_id)
  UNION ALL SELECT 'public.memory_entries',v.id::text,encode(sha256(convert_to(to_jsonb(v)::text,'UTF8')),'hex') FROM public.memory_entries v WHERE v.canonical_id IN (SELECT r.src_canonical_id FROM public.memory_relations r WHERE r.dst_canonical_id=ids.canonical_id UNION SELECT r.dst_canonical_id FROM public.memory_relations r WHERE r.src_canonical_id=ids.canonical_id)
  UNION ALL SELECT 'public.memory_entries',v.id::text,encode(sha256(convert_to(to_jsonb(v)::text,'UTF8')),'hex') FROM public.memory_entries v WHERE v.id IN (SELECT s.target_memory_id FROM public.memory_sources s JOIN public.memory_entries own ON own.id=s.memory_entry_id WHERE own.canonical_id=ids.canonical_id)
 ) d),'[]'::jsonb),
 'relations',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'src_canonical_id',r.src_canonical_id,'dst_canonical_id',r.dst_canonical_id,'relation_type',r.relation_type,'row_sha256',encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex')) ORDER BY r.id) FROM public.memory_relations r WHERE ids.canonical_id IN (r.src_canonical_id,r.dst_canonical_id)),'[]'::jsonb),
`;

export const FULL_REVIEW_SQL = REVIEW_SQL
  .replace('WITH ids AS', `${CATALOG_SQL}\nWITH ids AS`)
  .replace(" 'state',", `${fullDependencies} 'state',`)
  .replace('jsonb_agg(to_jsonb(p) ORDER BY p.id)', "jsonb_agg(to_jsonb(p)||jsonb_build_object('row_sha256',encode(sha256(convert_to(to_jsonb(p)::text,'UTF8')),'hex')) ORDER BY p.id)");

export function redactReviewText(value) {
  const safe = redact(value);
  return { text: safe.text.replace(/(?<![A-Za-z0-9:/])(?:[A-Za-z]:\\|\/(?!\/))[^\s<>"'`),;]+/g, '[LOCAL PATH OMITTED]'), secret: safe.redacted };
}

export function compactRelation(value) { return [value.id, value.src_canonical_id, value.dst_canonical_id, value.row_sha256]; }
export function relationSetDigest(values) {
  const digest = createHash('sha256'); digest.update('memory-area-relations-v1\n');
  for (const value of [...values].sort((a, b) => a[0].localeCompare(b[0]))) digest.update(JSON.stringify(value) + '\n');
  return digest.digest('hex');
}

export function fullHistoryReview(input) {
  const summary = summarizeLineage(input);
  const allStrings = JSON.stringify({ versions: input.versions, sources: input.sources, projects: input.projects, state: input.state });
  const secret = redact(allStrings).redacted;
  const oversized = Buffer.byteLength(allStrings) > 2 * 1024 * 1024;
  const hold = secret || oversized;
  return {
    event: 'history', canonical_id: input.canonical_id,
    review_hold: hold ? (secret ? 'possible_secret_material' : 'history_exceeds_review_limit') : null,
    projects: input.projects.map(p => ({ id: p.id, slug: hold ? '[WITHHELD]' : redactReviewText(p.slug).text, name: hold ? '[WITHHELD]' : redactReviewText(p.name).text })),
    versions: input.versions.map(v => ({ id: v.id, project_id: v.project_id, version_no: v.version_no,
      is_tombstone: v.is_tombstone, row_sha256: v.row_sha256,
      summary: hold ? '[HISTORY WITHHELD]' : redactReviewText(v.summary).text,
      canonical_text: hold ? '[HISTORY WITHHELD]' : redactReviewText(v.canonical_text).text })),
    sources: input.sources.map(s => ({ id: s.id, memory_entry_id: s.memory_entry_id, row_sha256: s.row_sha256,
      source_kind: hold ? '[WITHHELD]' : redactReviewText(s.source_kind).text,
      evidence_label: hold ? '[WITHHELD]' : redactReviewText(String(s.file_path ?? '').split(/[\\/]/).at(-1)).text,
      // Original evidence bytes, prompts and task titles are not disclosed.
      evidence_content: 'protected archive; reference-only projection available after explicit review' })),
    relations: { count: (input.relations ?? []).length, sha256: relationSetDigest((input.relations ?? []).map(compactRelation)),
      representation: 'Exact original edges retained privately; ownership policy uses reviewed endpoint areas. Individual relation contents are not disclosed here.' },
    heuristic: { candidate_area: summary.proposal.candidate_area, risks: summary.proposal.risks, ownership_approved: false },
    complete_curated_history: !hold, original_evidence_disclosed: false,
    secret_assurance: 'Pattern screening only; not proof that all secrets have been found.',
  };
}

export async function consumeReview(lines, bindings) {
  const entries = [];
  const seen = new Set();
  let header;
  let complete = false;
  let versions = 0;
  for await (const line of lines) {
    let event;
    try { event = JSON.parse(line); } catch { throw Error('Invalid review stream; raw output suppressed'); }
    if (complete) throw Error('Unexpected data after review completion');
    if (event.event === 'header' && !header) {
      header = event;
      if (header.read_only !== 'on' || header.database !== (bindings.database ?? DATABASE) || header.canonical_count !== bindings.canonical_ids.size
        || header.version_count !== bindings.expected_versions || header.project_count !== bindings.expected_projects) throw Error('Restored source counts or transaction safety changed');
    } else if (event.event === 'lineage' && header) {
      if (!bindings.canonical_ids.has(event.canonical_id) || seen.has(event.canonical_id)) throw Error('Unknown or duplicate canonical history');
      seen.add(event.canonical_id);
      versions += event.versions.length;
      entries.push(summarizeLineage(event));
    } else if (event.event === 'complete' && header) complete = true;
    else throw Error('Unexpected review stream ordering');
  }
  if (!complete || seen.size !== bindings.canonical_ids.size || versions !== bindings.expected_versions) throw Error('Canonical history stream was incomplete');
  const sourceArchiveCandidates = annotateMixedSources(entries);
  const dispositionCounts = {};
  const areaCounts = {};
  for (const entry of entries) {
    dispositionCounts[entry.proposal.suggested_disposition] = (dispositionCounts[entry.proposal.suggested_disposition] ?? 0) + 1;
    const area = entry.proposal.candidate_area ?? 'unresolved';
    areaCounts[area] = (areaCounts[area] ?? 0) + 1;
  }
  entries.sort((a, b) => b.proposal.risks.length - a.proposal.risks.length || a.canonical_id.localeCompare(b.canonical_id));
  return { format: 'memory-area-human-review-v1', rules_version: 2, ownership_approved: false, import_supported: false,
    source: { ...bindings.evidence, restore_container_id: bindings.container_id, read_transaction_snapshot: header.snapshot, reviewed_at: header.captured_at },
    coverage: { canonical_histories: seen.size, memory_versions: versions, original_projects: header.project_count,
      all_remaining_version_text_scanned_for_rule_markers: true, raw_capture_payloads_inspected: false, complete_row_hashes_recorded: true,
      source_rows_recompared_with_inventory: false, excerpt_limits: LIMITS },
    limitations: ['Heuristics produce proposals only; no source/project/content review approval is manufactured.',
      'Historical versions are scanned but only latest readable content is excerpted; review all version hashes/IDs before approval.',
      'Complete-row hashes and fixed restored-container binding identify reviewed bytes; this pack does not independently recompare them with inventory row hashes.',
      'Source paths/excerpts are provenance hints. Shared-source risk derives from links; raw capture payloads are not inspected.',
      'Generic project names such as ca-2 do not imply Certificate Authority. General project IDs remain separate.',
      'Candidate defaults retain original project areas. Memory Layer/PKI topic mentions can describe tooling use or certificate consumers, not ownership.',
      'Delimited test/eval/smoke/fixture project markers deliberately create false-positive review holds; these must be explicitly resolved before an ownership decision.',
      'Narrow CA operator markers can propose Trust only from Homelab, excluding detected consumers and negated operations; this lexical test does not understand every context.',
      'Possible credential patterns are redacted from previews and force protected review; this is not a complete secret scanner.',
      'No approved manifest, database write, provider call, project merge, importer or cutover is produced.'],
    counts: { dispositions: dispositionCounts, area_candidates: areaCounts, supporting_archive_candidates: sourceArchiveCandidates.length },
    supporting_archive_candidates: sourceArchiveCandidates, entries };
}

export async function collectReview(options) {
  const bindings = await loadBindings(options);
  const sandbox = await inspectSandbox(options.descriptor);
  if (sandbox.container_id !== bindings.container_id || sandbox.restore_evidence_sha256 !== bindings.evidence.restore_evidence_sha256
    || sandbox.source_instance !== bindings.evidence.source_instance || sandbox.snapshot_id !== bindings.evidence.snapshot_id)
    throw Error('Restore descriptor, review evidence or source identities changed');
  bindings.database = sandbox.database;
  bindings.evidence.restore_descriptor_sha256 = sandbox.restore_descriptor_sha256;
  const child = spawn('docker', ['exec', '-i', sandbox.container_id, 'psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-h', '/var/run/postgresql', '-U', 'postgres', '-d', sandbox.database], { stdio: ['pipe', 'pipe', 'pipe'] });
  child.stderr.resume();
  child.stdin.on('error', () => {});
  const done = new Promise(resolve => { child.once('error', () => resolve(-1)); child.once('close', resolve); });
  child.stdin.end(REVIEW_SQL);
  try {
    const pack = await consumeReview(streamLines(child.stdout), bindings);
    if (await done !== 0) throw Error('Database review query failed; raw stderr suppressed');
    const serialized = `${JSON.stringify(pack, null, 2)}\n`;
    if (Buffer.byteLength(serialized, 'utf8') > LIMITS.review_pack_bytes) throw Error('Review pack exceeds its bounded output limit; no partial pack published');
    await privateArtifact(options.output, async write => write(serialized));
    return { review_pack_written: true, canonical_histories: pack.coverage.canonical_histories,
      memory_versions: pack.coverage.memory_versions, ownership_approved: false, import_supported: false, counts: pack.counts };
  } catch (error) { child.stdin.destroy(); child.kill('SIGTERM'); child.stdout.destroy(); throw error; }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (process.argv.slice(2).includes('--help')) process.stdout.write(HELP);
  else {
    try { process.stdout.write(`${JSON.stringify(await collectReview(parseArgs(process.argv.slice(2))))}\n`); }
    catch { process.stderr.write('Private classification review failed; no content was disclosed or approved. Check protected input bindings, restore sandbox, source counts and output permissions.\n'); process.exitCode = 1; }
  }
}
