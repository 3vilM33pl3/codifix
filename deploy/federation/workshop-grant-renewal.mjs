#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Prepare only: no service reload, database mutation, API key read or paid call.
import fs from 'node:fs/promises';
import { constants } from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { privateInput } from '../../scripts/areas/restore-descriptor.mjs';
import { canonical } from './pds-enrollment/files.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const check = (value, reason) => { if (!value) throw Error(reason); };
const ROOT = '/home/olivier/.local/state/memory-workshop-preview-20260918';
const UUID = /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/u;
const APPROVED_SOURCES = ['44e45bda-d0e3-4b92-b699-0835fc297d2d', '4fa476fd-6b78-48de-a196-c0a76160bcc2', '5ba41d65-3d5d-4de2-af67-500ffd8dfd79'];
export function renewal(oldManifest, now) {
  const value = JSON.parse(oldManifest.toString('utf8'));
  check(value.schema_version === 1 && value.grant?.instance_id === '72d1a39c-29fe-4f8d-94ee-b3a8dba51f85'
    && value.grant.project_id === 'fb83249e-c78c-43eb-a4ba-928513a9023a'
    && value.grant.principal_id === 'ba0b1627-a3ac-47a1-bc62-08ccd50d8cc1'
    && value.grant.policy_revision === '79c928c4-59db-4dc6-aade-a055156e8cf3'
    && value.grant.destination === 'openai' && value.grant.sources.length === 3, 'existing_reviewed_workshop_profile_required');
  for (const source of value.grant.sources) check(UUID.test(source.canonical_id) && UUID.test(source.version_id)
    && source.version_id === source.canonical_id && source.version === 1 && Array.isArray(source.evidence_digest) && source.evidence_digest.length === 32
    && source.evidence_digest.every(byte => Number.isInteger(byte) && byte >= 0 && byte <= 255), 'source_binding_invalid');
  check(canonical(value.grant.sources.map(source => source.canonical_id).sort()) === canonical(APPROVED_SOURCES), 'approved_sources_changed');
  const before = new Date(now); check(Number.isFinite(before.getTime()), 'explicit_preparation_time_required');
  check(Date.parse(value.grant.expires_at) <= before.getTime(), 'existing_grant_has_not_expired');
  const next = structuredClone(value);
  next.grant.expires_at = new Date(before.getTime() + 7 * 86400000).toISOString();
  const { expires_at: oldExpiry, ...oldGrant } = value.grant;
  const { expires_at: newExpiry, ...newGrant } = next.grant;
  check(canonical(oldGrant) === canonical(newGrant) && newExpiry !== oldExpiry, 'grant_scope_changed');
  const bytes = Buffer.from(`${JSON.stringify(next)}\n`);
  return { manifest: next, bytes, sha256: hash(bytes), old_sha256: hash(oldManifest), prepared_at: before.toISOString(), expires_at: newExpiry };
}
export function serviceConfig(bytes, oldHash, newHash) {
  const text = bytes.toString('utf8');
  check(!/^\[supervisor\.model_work\]/mu.test(text), 'autonomous_model_work_must_remain_disabled');
  const section = /^\[supervisor\.interaction\]\n([^[]*)/mu.exec(text);
  check(section && section[1].includes('manifest = "/interaction/work.json"'), 'expected_explicit_interaction_required');
  const old = `sha256 = "${oldHash}"`;
  check(section[1].split(old).length === 2, 'old_profile_pin_changed');
  return Buffer.from(text.replace(section[0], section[0].replace(old, `sha256 = "${newHash}"`)));
}
function sourceDigest(row, project) {
  // Match Chrono AutoSi UTC (0/3/6 fractional digits), retaining microseconds.
  let authored = row.authored_at;
  if (authored.endsWith('.000000Z')) authored = authored.replace('.000000Z', 'Z');
  else if (/\.\d{3}000Z$/u.test(authored)) authored = authored.replace(/000Z$/u, 'Z');
  return hash(JSON.stringify({ project_id: project, canonical_id: row.canonical_id, version_id: row.id,
    version: row.version_no, authored_at: authored, summary: row.summary, text: row.canonical_text }));
}
function query(sql) {
  const result = spawnSync('docker', ['exec', '--env', 'PGOPTIONS=-c default_transaction_read_only=on -c timezone=UTC',
    '461db39b48b6688b4af5cc47566d9b8b7b7c81951d6703641d9e8acaba786c55',
    'psql', '-XAt', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'memory_workshop_preview_20260918', '-c', sql],
  { encoding: 'utf8', maxBuffer: 1024 * 1024 });
  check(result.status === 0, 'read_only_workshop_validation_failed');
  return JSON.parse(result.stdout.trim());
}
export async function prepare(stageName, now = new Date().toISOString()) {
  check(/^questions-renewed-[a-z0-9-]+$/u.test(stageName), 'dedicated_new_stage_required');
  const oldStage = path.join(ROOT, 'questions-60'), nextStage = path.join(ROOT, stageName);
  const artifacts = (await privateInput(path.join(oldStage, 'artifact.json'), 4096, true)).value;
  const oldProfile = await privateInput(path.join(oldStage, 'work.json'), 16384);
  const oldConfig = await privateInput(path.join(ROOT, 'app.toml'), 65536);
  const worker = await privateInput(path.join(oldStage, 'worker'), 100 * 1024 * 1024);
  const oldQueue = await privateInput(path.join(oldStage, 'queue.mjs'), 65536);
  check(oldProfile.sha256 === artifacts.manifest_sha256 && oldConfig.sha256 === artifacts.service_config_sha256
    && worker.sha256 === artifacts.worker_sha256, 'old_profile_configuration_or_worker_changed');
  const result = renewal(await fs.readFile(path.join(oldStage, 'work.json')), now), grant = result.manifest.grant;
  const ids = grant.sources.map(source => `'${source.canonical_id}'`).join(',');
  // All statements are reads. Sensitive source text is hashed locally and never
  // printed, written to this new stage, or sent to a model.
  const observed = query(`SELECT jsonb_build_object(
    'policy',(SELECT to_jsonb(p)-'created_at' FROM supervisor_policies p WHERE area='workshop'),
    'principal',(SELECT jsonb_build_object('kind',kind,'disabled',disabled_at IS NOT NULL,'global_role',global_role,'groups',groups_json,'global_permissions',global_permissions_json) FROM auth_principals WHERE id='${grant.principal_id}'),
    'memberships',(SELECT jsonb_agg(jsonb_build_object('project_id',project_id,'permissions',permissions_json)) FROM auth_project_memberships WHERE principal_id='${grant.principal_id}'),
    'sources',(SELECT jsonb_agg(jsonb_build_object('id',m.id,'canonical_id',m.canonical_id,'version_no',m.version_no,'is_tombstone',m.is_tombstone,'status',s.status,'authored_at',to_char(m.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),'summary',m.summary,'canonical_text',m.canonical_text)) FROM memory_entries m JOIN memory_canonical_state s ON s.project_id=m.project_id AND s.canonical_id=m.canonical_id WHERE m.project_id='${grant.project_id}' AND m.canonical_id IN (${ids}) AND m.version_no=(SELECT max(x.version_no) FROM memory_entries x WHERE x.project_id=m.project_id AND x.canonical_id=m.canonical_id)),
    'unsettled',(SELECT count(*) FROM supervisor_attempt_states WHERE state IN ('reserved','in_flight','uncertain')),
    'unexpired_questions',(SELECT count(*) FROM supervisor_questions WHERE expires_at>clock_timestamp()),
    'budget',(SELECT jsonb_build_object('day',(clock_timestamp() AT TIME ZONE 'UTC')::date,'calls',count(*) FILTER (WHERE s.state<>'released_before_send'),'charged_usd_micros',coalesce(sum(CASE WHEN s.state='settled' THEN u.actual_usd_micros ELSE 0 END),0),'reserved_usd_micros',coalesce(sum(CASE WHEN s.state IN ('reserved','in_flight','uncertain') THEN a.worst_usd_micros ELSE 0 END),0)) FROM supervisor_attempts a JOIN supervisor_attempt_states s ON s.attempt_id=a.id LEFT JOIN supervisor_settlements u ON u.attempt_id=a.id WHERE a.area='workshop' AND a.day=(clock_timestamp() AT TIME ZONE 'UTC')::date));`);
  check(observed.policy?.revision === grant.policy_revision && observed.policy.enabled && !observed.policy.revoked
    && observed.policy.model === 'gpt-5.6-sol' && observed.policy.daily_calls === 20 && observed.policy.daily_usd_micros === 2500000, 'unchanged_policy_required');
  check(observed.principal?.kind === 'internal' && !observed.principal.disabled && observed.principal.global_role === null
    && canonical(observed.principal.groups) === '[]' && [null, '[]'].includes(observed.principal.global_permissions === null ? null : canonical(observed.principal.global_permissions))
    && observed.memberships?.length === 1 && observed.memberships[0].project_id === grant.project_id
    && canonical([...observed.memberships[0].permissions].sort()) === canonical(['bundle_export', 'loops_run', 'memory_read']), 'current_exact_project_authority_required');
  check(observed.sources?.length === 3 && observed.unsettled === 0 && observed.unexpired_questions === 0, 'sources_or_pending_work_require_review');
  for (const source of grant.sources) {
    const row = observed.sources.find(row => row.canonical_id === source.canonical_id);
    check(row && row.id === source.version_id && row.version_no === source.version && !row.is_tombstone && row.status === 'active'
      && sourceDigest(row, grant.project_id) === Buffer.from(source.evidence_digest).toString('hex'), 'reviewed_source_has_changed');
  }
  const nextConfig = serviceConfig(await fs.readFile(path.join(ROOT, 'app.toml')), result.old_sha256, result.sha256);
  const queue = (await fs.readFile(path.join(oldStage, 'queue.mjs'), 'utf8')).replace('const stage = `${root}/questions-60`;', `const stage = \`\${root}/${stageName}\`;`);
  check(queue.includes(`const stage = \`\${root}/${stageName}\`;`), 'queue_stage_binding_missing');
  check((await privateInput(path.join(oldStage, 'queue.mjs'), 65536)).sha256 === oldQueue.sha256, 'queue_changed_during_preparation');
  await fs.mkdir(nextStage, { mode: 0o700 });
  const put = (name, bytes, mode = 0o600) => fs.writeFile(path.join(nextStage, name), bytes, { mode, flag: 'wx' });
  await put('work.json', result.bytes); await put('app.toml', nextConfig); await put('queue.mjs', queue);
  await fs.copyFile(path.join(oldStage, 'worker'), path.join(nextStage, 'worker'), constants.COPYFILE_EXCL); await fs.chmod(path.join(nextStage, 'worker'), 0o700);
  const nextArtifacts = { worker_sha256: worker.sha256, manifest_sha256: result.sha256, service_config_sha256: hash(nextConfig) };
  await put('artifact.json', `${JSON.stringify(nextArtifacts)}\n`);
  const unit = `[Unit]\nDescription=Workshop explicitly confirmed private question worker\nAfter=memory-workshop-preview-relay.service\nRequires=memory-workshop-preview-relay.service\n\n[Service]\nType=simple\nExecStart=/usr/bin/node ${nextStage}/queue.mjs serve\nWorkingDirectory=${ROOT}\nRestart=no\nKillMode=control-group\nTimeoutStopSec=420\nNoNewPrivileges=yes\nUMask=0077\n\n[Install]\nWantedBy=default.target\n`;
  await put('memory-workshop-question-worker.service', unit);
  const args = ['run-next', path.join(nextStage, 'work.json'), result.sha256, grant.instance_id];
  const preflight = spawnSync(path.join(nextStage, 'worker'), [...args, path.join(nextStage, 'app.toml'), hash(nextConfig)],
    { env: { PATH: '/usr/bin:/bin' }, encoding: 'utf8' });
  check(preflight.status === 1 && preflight.stdout === '' && preflight.stderr.trim() === 'Explicit Workshop database is required.', 'offline_new_profile_preflight_failed');
  const oldRejected = spawnSync(path.join(nextStage, 'worker'), [...args, path.join(ROOT, 'app.toml'), oldConfig.sha256],
    { env: { PATH: '/usr/bin:/bin' }, encoding: 'utf8' });
  check(oldRejected.status === 1 && oldRejected.stdout === '' && oldRejected.stderr.trim() === 'Pinned service configuration does not permit question execution.', 'old_profile_still_accepted');
  const report = { schema_version: 1, prepared_at: result.prepared_at, grant_expires_at: result.expires_at,
    old_manifest_sha256: result.old_sha256, ...nextArtifacts, queue_sha256: hash(queue), new_directory: nextStage,
    source_count: 3, sources_unchanged: true, policy_revision: grant.policy_revision, model: observed.policy.model,
    daily_calls: 20, daily_usd_micros: 2500000, observed_budget: observed.budget,
    active_work_at_preparation: observed.unsettled, unexpired_questions_at_preparation: observed.unexpired_questions,
    offline_new_profile_preflight_passed: true, old_profile_rejected: true, database_mutations: 0, provider_calls: 0,
    installed: false, worker_started: false, live_service_config_changed: false };
  await put('renewal.json', `${JSON.stringify(report, null, 2)}\n`); return report;
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  process.umask(0o077);
  prepare(process.argv[2]).then(result => console.log(JSON.stringify(result))).catch(error => { console.error(error.message); process.exitCode = 1; });
}
