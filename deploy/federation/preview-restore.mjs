#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Recovery from an explicit encrypted preview backup; never starts old writers.
import fs from 'node:fs/promises';
import { createReadStream, createWriteStream } from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { pipeline } from 'node:stream/promises';
import { pathToFileURL } from 'node:url';
import { fileURLToPath } from 'node:url';
import { validate as validateBackup } from './preview-backup.mjs';
import { canonical } from './pds-enrollment/files.mjs';
import { archiveInventory, inventoryDifferences } from '../../scripts/areas/restore-audit.mjs';
import { inspectContainer, privateInput, validateSandbox } from '../../scripts/areas/restore-descriptor.mjs';

const check = (value, code) => { if (!value) throw Error(code); };
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const image = 'sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180';
const run = (command, args, extra = {}) => {
  const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, ...extra });
  check(result.status === 0, /^offline_pds_proof_failed_at_\d+\n$/u.test(result.stderr ?? '')
    ? result.stderr.trim() : `${path.basename(command)}_failed_private_diagnostics_suppressed`); return result.stdout;
};
const write = (file, value) => fs.writeFile(file, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600, flag: 'wx' });
async function fileHash(file) { const hash = createHash('sha256'); for await (const bytes of createReadStream(file)) hash.update(bytes); return hash.digest('hex'); }
function child(command, args) {
  const process = spawn(command, args, { stdio: ['pipe', 'pipe', 'ignore'] });
  const done = new Promise((resolve, reject) => { process.once('error', () => reject(Error('restore_child_start_failed')));
    process.once('close', code => code === 0 ? resolve() : reject(Error('restore_child_failed_diagnostics_suppressed'))); });
  return { process, done };
}
async function streamTo(command, args, destination) {
  const source = child(command, args); source.process.stdin.end();
  try { await Promise.all([pipeline(source.process.stdout, destination), source.done]); }
  catch (error) { source.process.kill('SIGTERM'); await source.done.catch(() => {}); throw error; }
}
export function allowedArchiveEntries(listing, configured, backupName) {
  const required = ['intent.json', 'workshop.dump', 'homelab.dump', 'gateway.dump', 'workshop.roles.sql', 'homelab.roles.sql', 'gateway.roles.sql']
    .map(name => `${backupName}/${name}`);
  const allowed = [...configured, ...required], found = new Set(), entries = [];
  for (const line of listing.trimEnd().split('\n')) {
    const match = /^([d-])[rwxstST-]{9}\s+\d+\/\d+\s+(\d+)\s+\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}\s+(.+)$/u.exec(line);
    check(match, 'archive_non_regular_or_unknown_entry');
    const name = match[3].replace(/\/$/u, '');
    check(/^[A-Za-z0-9_.:/-]+$/u.test(name) && !name.startsWith('/')
      && !name.split('/').some(part => ['', '.', '..'].includes(part)), 'archive_unsafe_path');
    check(allowed.some(item => name === item || name.startsWith(`${item}/`)), 'archive_unreviewed_path');
    check(!found.has(name), 'archive_duplicate_path'); found.add(name);
    check(Number.isSafeInteger(Number(match[2])) && Number(match[2]) <= 1024 * 1024 * 1024, 'archive_entry_too_large');
    entries.push({ name, type: match[1] === 'd' ? 'directory' : 'file', bytes: Number(match[2]) });
  }
  check(required.every(name => found.has(name)) && configured.every(name => found.has(name)), 'archive_required_path_missing');
  check(entries.reduce((n, entry) => n + entry.bytes, 0) <= 4 * 1024 * 1024 * 1024, 'archive_too_large');
  return entries;
}
export function roleRestoreSql(sql) {
  // The freshly initialized clone already has exactly postgres. Suppress only
  // that CREATE, never ALTER, grants, credentials or unexpected restore errors.
  let removed = 0;
  const filtered = sql.split('\n').filter(line => {
    if (line === 'CREATE ROLE postgres;') { removed++; return false; }
    return true;
  }).join('\n');
  check(removed <= 1 && !/^CREATE TABLESPACE /mu.test(sql), 'unsupported_global_role_state');
  return { sql: filtered, existing_postgres_create_omitted: removed };
}
function normalizedGlobals(sql) {
  return sql.split('\n').filter(line => line && !line.startsWith('--') && !/^\\(?:un)?restrict /u.test(line)).join('\n');
}
async function config(file, receiptPath, output, expectedSha) {
  check(/^[a-f0-9]{64}$/u.test(expectedSha), 'explicit_archive_digest_required');
  const c = validateBackup((await privateInput(file, 1024 * 1024, true)).value);
  const receipt = (await privateInput(receiptPath, 1024 * 1024, true)).value;
  check(receipt.sha256 === expectedSha && receipt.review === sha(JSON.stringify(c))
    && receipt.copy === path.join(c.nas_directory, `${path.basename(c.directory)}.tar.gpg`), 'backup_receipt_binding_mismatch');
  check(path.dirname(output) === '/data/tmp' && /^memory-preview-recovery-[a-z0-9-]+$/u.test(path.basename(output)), 'dedicated_recovery_directory_required');
  const parent = await fs.lstat('/data/tmp'); check(parent.isDirectory() && [0, process.getuid()].includes(parent.uid)
    && (!(parent.mode & 0o022) || (parent.mode & 0o1000)), 'unsafe_recovery_parent');
  return { c, receipt, output };
}
export async function extract(file, receiptPath, output, expectedSha) {
  const { c, receipt } = await config(file, receiptPath, output, expectedSha);
  const archive = await fs.lstat(receipt.copy); check(archive.isFile() && !archive.isSymbolicLink(), 'archive_regular_file_required');
  check(await fileHash(receipt.copy) === expectedSha, 'nas_archive_digest_mismatch');
  await fs.mkdir(output, { mode: 0o700 });
  const tar = path.join(output, 'archive.tar');
  await streamTo('gpg', ['--batch', '--no-tty', '--pinentry-mode', 'error', '--decrypt', receipt.copy], createWriteStream(tar, { flags: 'wx', mode: 0o600 }));
  const entries = allowedArchiveEntries(run('tar', ['--list', '--verbose', '--numeric-owner', '--quoting-style=escape', '--file', tar]), c.archive_paths, path.basename(c.directory));
  const extracted = path.join(output, 'extracted'); await fs.mkdir(extracted, { mode: 0o700 });
  run('tar', ['--extract', '--file', tar, '--directory', extracted, '--no-same-owner', '--no-same-permissions', '--keep-old-files', '--delay-directory-restore']);
  const files = [];
  for (const entry of entries) {
    const target = path.join(extracted, entry.name), metadata = await fs.lstat(target);
    check(entry.type === 'directory' ? metadata.isDirectory() : metadata.isFile() && metadata.nlink === 1, 'extracted_type_changed');
    await fs.chmod(target, entry.type === 'directory' ? 0o700 : 0o600);
    if (entry.type === 'file') {
      check(metadata.size === entry.bytes, 'extracted_size_mismatch');
      files.push({ name: entry.name, bytes: entry.bytes, sha256: await fileHash(target) });
    }
  }
  const evidence = { schema_version: 1, archive_sha256: expectedSha, archive_copy: receipt.copy, tar_sha256: await fileHash(tar),
    extracted_at: new Date().toISOString(), files, old_writers_started: false, provider_calls: 0 };
  await write(path.join(output, 'extraction.json'), evidence);
  return { extracted: true, files: files.length, archive_sha256: expectedSha, recovery_directory: output };
}
async function inventory(command, args) {
  const source = child(command, args); source.process.stdin.end();
  try { const [result] = await Promise.all([archiveInventory(source.process.stdout), source.done]); return result; }
  catch (error) { source.process.kill('SIGTERM'); await source.done.catch(() => {}); throw error; }
}
const psql = (id, database, sql) => run('docker', ['exec', id, 'psql', '-XAt', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', database, '-c', sql]).trim();
async function restoreDatabase(c, output, label, suffix, resume) {
  const source = c.databases.find(db => db.label === label), sourceRoot = path.join(output, 'extracted', path.basename(c.directory));
  const container = `memory-four-area-restore-preview-${suffix}-${label}`, volume = `${container}-data`;
  const exists = spawnSync('docker', ['container', 'inspect', container], { stdio: 'ignore' });
  const volumeExists = spawnSync('docker', ['volume', 'inspect', volume], { stdio: 'ignore' });
  let id;
  if (resume && exists.status === 0 && volumeExists.status === 0) {
    const recorded = (await privateInput(path.join(output, `${label}-container.json`), 4096, true)).value;
    const inspected = inspectContainer(recorded.id); validateSandbox(inspected, container);
    check(recorded.container === container && recorded.volume === volume && recorded.image === image && inspected.Image === image
      && inspected.Mounts.some(mount => mount.Type === 'volume' && mount.Name === volume && mount.Destination === '/var/lib/postgresql/data'), 'resumed_target_binding_mismatch');
    id = recorded.id;
  } else {
    check(exists.status !== 0 && volumeExists.status !== 0, 'recovery_target_already_exists');
    await write(path.join(output, `${label}-intent.json`), { container, volume, image, database: source.database, source_database_container_id: source.id });
    run('docker', ['volume', 'create', '--label', `memory.preview.restore=${suffix}`, volume]);
    id = run('docker', ['run', '-d', '--name', container, '--network', 'none', '--read-only', '--security-opt', 'no-new-privileges',
      '--restart', 'no', '--label', `memory.preview.restore=${suffix}`, '--mount', `type=volume,source=${volume},target=/var/lib/postgresql/data`,
      '--tmpfs', '/tmp:rw,noexec,nosuid', '--tmpfs', '/var/run/postgresql:rw,noexec,nosuid', '-e', 'POSTGRES_HOST_AUTH_METHOD=trust', image]).trim();
    await write(path.join(output, `${label}-container.json`), { container, id, volume, image });
  }
  check(/^[a-f0-9]{64}$/u.test(id), 'invalid_restore_container_id');
  for (let attempt = 0; ; attempt++) {
    const ready = spawnSync('docker', ['exec', id, 'pg_isready', '-U', 'postgres'], { stdio: 'ignore' });
    const pid = spawnSync('docker', ['exec', id, 'head', '-n', '1', '/var/lib/postgresql/data/postmaster.pid'], { encoding: 'utf8' });
    if (ready.status === 0 && pid.status === 0 && pid.stdout.trim() === '1') break;
    check(attempt < 60, 'restore_database_not_ready'); await new Promise(resolve => setTimeout(resolve, 500));
  }
  validateSandbox(inspectContainer(id), container);
  const globals = await fs.readFile(path.join(sourceRoot, `${label}.roles.sql`), 'utf8');
  const roles = roleRestoreSql(globals);
  const userRoles = psql(id, 'postgres', "SELECT string_agg(rolname,',' ORDER BY rolname) FROM pg_roles WHERE rolname !~ '^pg_'");
  if (userRoles === 'postgres') run('docker', ['exec', '-i', id, 'psql', '-X', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres'], { input: roles.sql });
  else check(resume, 'unexpected_existing_restore_role');
  const databaseExists = psql(id, 'postgres', `SELECT count(*) FROM pg_database WHERE datname='${source.database}'`);
  if (databaseExists === '0') {
    const restored = child('docker', ['exec', '-i', id, 'pg_restore', '--create', '--exit-on-error', '-U', 'postgres', '--dbname=postgres']);
    restored.process.stdout.resume();
    try { await Promise.all([pipeline(createReadStream(path.join(sourceRoot, `${label}.dump`)), restored.process.stdin), restored.done]); }
    catch (error) { restored.process.kill('SIGTERM'); await restored.done.catch(() => {}); throw error; }
  } else check(resume && databaseExists === '1', 'restore_database_not_empty');
  const expected = await inventory('pg_restore', ['--data-only', '--no-owner', '--no-privileges', '--file=-', path.join(sourceRoot, `${label}.dump`)]);
  const actual = await inventory('docker', ['exec', '--env', 'PGOPTIONS=-c timezone=Etc/UTC', id, 'pg_dump', '-U', 'postgres', '-d', source.database,
    '--data-only', '--no-owner', '--no-privileges']);
  const differences = [...inventoryDifferences(expected.tables, actual.tables), ...inventoryDifferences(expected.sequences, actual.sequences, 'sequence')];
  check(differences.length === 0, 'restored_row_or_sequence_mismatch');
  check(psql(id, source.database, 'SELECT count(*) FROM pg_index WHERE NOT indisvalid; SELECT count(*) FROM pg_constraint WHERE NOT convalidated;') === '0\n0', 'invalid_restored_schema');
  const actualGlobals = run('docker', ['exec', id, 'pg_dumpall', '-U', 'postgres', '-l', source.database, '--globals-only']);
  const expectedNormalized = normalizedGlobals(globals), actualNormalized = normalizedGlobals(actualGlobals);
  // A gateway dump may have used a different bootstrap superuser. The only
  // permitted extra restore role is the known initial postgres, with no grants.
  const expectedNames = [...globals.matchAll(/^CREATE ROLE ([a-z_][a-z0-9_]*);$/gmu)].map(match => match[1]);
  const normalizedActual = expectedNames.includes('postgres') ? actualNormalized : actualNormalized.split('\n')
    .filter(line => !/^CREATE ROLE postgres;$|^ALTER ROLE postgres WITH /u.test(line)).join('\n');
  check(normalizedActual === expectedNormalized, 'restored_roles_or_grants_mismatch');
  // Compare schema DDL too, including ownership, ACLs, RLS, indexes and routines.
  const expectedSchema = run('pg_restore', ['--schema-only', '--file=-', path.join(sourceRoot, `${label}.dump`)]);
  const actualSchema = run('docker', ['exec', id, 'pg_dump', '-U', 'postgres', '-d', source.database, '--schema-only']);
  const normalizeSchema = text => text.split('\n').filter(line => !line.startsWith('--') && !/^\\(?:un)?restrict /u.test(line) && line.trim()).join('\n');
  check(normalizeSchema(expectedSchema) === normalizeSchema(actualSchema), 'restored_schema_or_ownership_mismatch');
  validateSandbox(inspectContainer(id), container);
  const report = { schema_version: 1, label, database: source.database, container, container_id: id, image,
    network: 'none', published_ports: false, tables: actual.tables, sequences: actual.sequences,
    all_rows_match: true, all_sequences_match: true, roles_and_grants_match: true, schema_and_ownership_match: true,
    existing_postgres_create_omitted: roles.existing_postgres_create_omitted, known_bootstrap_postgres_extra: !expectedNames.includes('postgres'),
    invalid_indexes: 0, unvalidated_constraints: 0, verified_at: new Date().toISOString() };
  const reportPath = path.join(output, `${label}-verified.json`);
  if (resume && await fs.stat(reportPath).then(() => true, () => false)) {
    const previous = (await privateInput(reportPath, 4 * 1024 * 1024, true)).value;
    check(JSON.stringify({ ...previous, verified_at: null }) === JSON.stringify({ ...report, verified_at: null }), 'previous_verification_changed');
  } else await write(reportPath, report);
  return report;
}
export async function verify(file, receiptPath, output, expectedSha, resume = false) {
  const { c } = await config(file, receiptPath, output, expectedSha);
  const extraction = (await privateInput(path.join(output, 'extraction.json'), 4 * 1024 * 1024, true)).value;
  check(extraction.archive_sha256 === expectedSha, 'extraction_binding_mismatch');
  for (const entry of extraction.files) check(await fileHash(path.join(output, 'extracted', entry.name)) === entry.sha256, 'extracted_artifact_changed');
  const suffix = path.basename(output).slice('memory-preview-recovery-'.length);
  const reports = [];
  for (const label of ['workshop', 'homelab', 'gateway']) reports.push(await restoreDatabase(c, output, label, suffix, resume));
  const result = { schema_version: 1, archive_sha256: expectedSha, completed_at: new Date().toISOString(), databases: reports.map(report => ({ label: report.label,
    container_id: report.container_id, tables: report.tables.length, rows: report.tables.reduce((n, table) => n + table.rows, 0), sequences: report.sequences.length,
    all_rows_match: true, all_sequences_match: true, roles_and_grants_match: true, schema_and_ownership_match: true })),
  original_writers_affected: false, providers_invoked: false, old_workers_started: false, pds_verified: false, production_disaster_recovery: false };
  await write(path.join(output, 'database-recovery.json'), result); return result;
}
export async function operationalProof(file, receiptPath, output, expectedSha) {
  await config(file, receiptPath, output, expectedSha);
  const extraction = (await privateInput(path.join(output, 'extraction.json'), 4 * 1024 * 1024, true)).value;
  const recovery = (await privateInput(path.join(output, 'database-recovery.json'), 4 * 1024 * 1024, true)).value;
  check(extraction.archive_sha256 === expectedSha && recovery.archive_sha256 === expectedSha, 'recovery_binding_mismatch');
  for (const entry of extraction.files) check(await fileHash(path.join(output, 'extracted', entry.name)) === entry.sha256, 'extracted_artifact_changed');
  const root = path.join(output, 'extracted', 'memory-company-20260919');
  const gateway = JSON.parse(await fs.readFile(path.join(root, 'gateway/gateway.json'), 'utf8'));
  const snapshot = {};
  for (const db of recovery.databases) {
    const proof = (await privateInput(path.join(output, `${db.label}-verified.json`), 4 * 1024 * 1024, true)).value;
    validateSandbox(inspectContainer(proof.container_id), proof.container); snapshot[db.label] = proof;
  }
  const gatewayProof = snapshot.gateway;
  const state = JSON.parse(psql(gatewayProof.container_id, gatewayProof.database,
    "SELECT jsonb_build_object('spaces',(SELECT jsonb_agg(to_jsonb(g) - 'projection') FROM federation_gateway_spaces g),'unpublished',(SELECT count(*) FROM federation_publish_outbox WHERE NOT published),'leased',(SELECT count(*) FROM federation_publish_outbox WHERE lease_token IS NOT NULL),'record_ids',(SELECT jsonb_agg(record_id::text ORDER BY record_id) FROM federation_accepted),'audits',(SELECT jsonb_agg(receipt ORDER BY sequence) FROM federation_audit),'trust',(SELECT jsonb_agg(keys) FROM federation_security),'answers',(SELECT coalesce(jsonb_agg(jsonb_build_object('request_id',request_id,'status',status,'answer_sha256',encode(sha256(convert_to(answer::text,'UTF8')),'hex')) ORDER BY request_id),'[]') FROM federation_expertise_mailbox WHERE answer IS NOT NULL));"));
  const deliveries = JSON.parse(psql(gatewayProof.container_id, gatewayProof.database, 'SELECT jsonb_agg(delivery ORDER BY sequence) FROM federation_accepted;'));
  check(state.spaces.length === 1 && state.spaces[0].space === gateway.space && state.spaces[0].authority === gateway.authority, 'gateway_identity_not_restored');
  const head = state.spaces[0];
  check(head.accepted_sequence === head.published_sequence && state.record_ids.length === head.published_sequence
    && state.audits.length === head.published_sequence && state.unpublished === 0 && state.leased === 0, 'publication_state_not_consistent');
  check(canonical(state.trust) === canonical([gateway.receipt_keys]), 'gateway_receipt_trust_mismatch');
  const participants = [];
  for (const label of ['workshop', 'homelab']) {
    const member = JSON.parse(await fs.readFile(path.join(root, `members/${label}/federation.json`), 'utf8'));
    check(member.space === gateway.space && member.authority === gateway.authority
      && canonical(member.receipt_keys) === canonical(gateway.receipt_keys)
      && gateway.credentials.some(credential => credential.instance === member.instance), 'member_identity_or_key_not_restored');
    const db = snapshot[label];
    const actual = JSON.parse(psql(db.container_id, db.database,
      "SELECT jsonb_build_object('replicas',(SELECT jsonb_agg(to_jsonb(r)-'projection') FROM federation_replicas r),'trust',(SELECT jsonb_agg(keys) FROM federation_replica_trust),'supervisor',(SELECT jsonb_agg(to_jsonb(s)-'created_at') FROM supervisor_instance_binding s),'unsettled_model',(SELECT count(*) FROM supervisor_attempt_states WHERE state IN ('reserved','in_flight','uncertain')),'answers',(SELECT coalesce(jsonb_agg(jsonb_build_object('request_id',request_id,'answer_digest',answer_digest,'answer_sha256',encode(sha256(convert_to(answer::text,'UTF8')),'hex')) ORDER BY request_id),'[]') FROM federation_expertise_origin_results WHERE answer IS NOT NULL));"));
    check(actual.replicas.length === 1 && actual.replicas[0].receiver === member.instance && actual.replicas[0].space === member.space
      && actual.replicas[0].authority === member.authority && actual.replicas[0].cursor === head.published_sequence
      && actual.replicas[0].sync_failures === 0 && actual.replicas[0].sync_error === null, 'replica_state_not_consistent');
    check(canonical(actual.trust) === canonical([member.receipt_keys]) && actual.unsettled_model === 0, 'participant_authority_not_restored');
    if (actual.supervisor) check(actual.supervisor.every(row => row.instance_id === member.instance), 'supervisor_identity_not_restored');
    for (const answer of actual.answers) {
      const mailbox = state.answers.find(row => row.request_id === answer.request_id);
      check(mailbox && mailbox.answer_sha256 === answer.answer_sha256 && mailbox.status === 'answered', 'retained_answer_bytes_not_consistent');
    }
    participants.push({ area: label, instance: member.instance, cursor: actual.replicas[0].cursor,
      receipt_keys_match: true, unsettled_model_attempts: 0, retained_answers: actual.answers.length,
      retained_answer_hashes: actual.answers.map(answer => ({ request_id: answer.request_id, sha256: answer.answer_sha256 })) });
  }
  const sqliteEntries = extraction.files.filter(entry => /^memory-company-20260919\/pds\/data\/.+\.sqlite$/u.test(entry.name));
  check(sqliteEntries.length >= 4, 'pds_sqlite_backup_incomplete');
  for (const entry of sqliteEntries) {
    const uri = `file:${path.join(output, 'extracted', entry.name)}?mode=ro&immutable=1`;
    check(run('sqlite3', [uri, 'PRAGMA integrity_check;']).trim() === 'ok', 'pds_sqlite_integrity_failed');
  }
  const actorEntries = sqliteEntries.filter(entry => entry.name.includes('/actors/') && entry.name.endsWith('/store.sqlite'));
  check(actorEntries.length === 1, 'single_reviewed_actor_required');
  const actorPath = path.posix.dirname(actorEntries[0].name.slice('memory-company-20260919/pds/data/'.length));
  const enrolled = JSON.parse(await fs.readFile(path.join(root, 'pds/inputs/config.json'), 'utf8'));
  const journal = JSON.parse(await fs.readFile(path.join(root, 'pds/state/journal.json'), 'utf8'));
  check(gateway.repository === enrolled.repository && gateway.signing_key === journal.signing_key, 'pds_gateway_enrollment_mismatch');
  const proofDir = path.join(output, 'proof-v2'); await fs.mkdir(proofDir, { mode: 0o700 }).catch(error => { if (error.code !== 'EEXIST') throw error; });
  const writeBinding = async (file, value) => {
    try { await write(file, value); } catch (error) {
      if (error.code !== 'EEXIST') throw error;
      check(canonical((await privateInput(file, 4 * 1024 * 1024, true)).value) === canonical(value), 'offline_proof_binding_changed');
    }
  };
  await writeBinding(path.join(proofDir, 'expectation.json'), { repository: gateway.repository, actor_path: actorPath,
    deliveries, receipt_keys: gateway.receipt_keys, audit_receipts: state.audits, space: gateway.space, authority: gateway.authority });
  await writeBinding(path.join(proofDir, 'pds-input-config.json'), { repository: enrolled.repository, signing_key: journal.signing_key, bootstrap_public_key: enrolled.bootstrap_public_key });
  const helper = fileURLToPath(new URL('./preview-pds-offline-proof.mjs', import.meta.url));
  const pinnedPds = 'ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12';
  const result = JSON.parse(run('docker', ['run', '--rm', '--network', 'none', '--read-only', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges',
    '--user', `${process.getuid()}:${process.getgid()}`, '--entrypoint', 'node',
    '--tmpfs', `/tmp:rw,noexec,nosuid,size=16777216,uid=${process.getuid()},gid=${process.getgid()},mode=700`,
    '--mount', `type=bind,source=${helper},target=/operator.mjs,readonly`,
    '--mount', `type=bind,source=${proofDir},target=/proof,readonly`,
    '--mount', `type=bind,source=${path.join(root, 'gateway/audit-key')},target=/audit-key,readonly`,
    '--mount', `type=bind,source=${path.join(root, 'pds/data')},target=/pds,readonly`,
    '--mount', `type=bind,source=${path.join(root, 'pds/state')},target=/enrollment,readonly`, pinnedPds, '/operator.mjs']));
  const report = { schema_version: 1, archive_sha256: expectedSha, completed_at: new Date().toISOString(),
    database_recovery_sha256: await fileHash(path.join(output, 'database-recovery.json')), extraction_sha256: await fileHash(path.join(output, 'extraction.json')),
    all_database_rows_sequences_schema_roles_grants_verified: true, accepted_sequence: head.accepted_sequence, published_sequence: head.published_sequence,
    unpublished: 0, leased: 0, participants, pds_sqlite_databases_checked: sqliteEntries.length, pds: result,
    original_writers_affected: false, workers_started: false, provider_calls: 0, production_disaster_recovery: false,
    limitations: ['No cloned application or PDS server was started.', 'No live routing, online key/session recovery, browser workflow or writer-fencing cutover was performed.',
      'Snapshot-only restore evidence: later source/preview writes are not included.', 'Archived credentials and identities are sensitive and must never be attached to the live network.'] };
  await write(path.join(output, 'operational-recovery.json'), report); return report;
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  process.umask(0o077);
  const [phase, file, receipt, output, expectedSha] = process.argv.slice(2);
  Promise.resolve().then(() => { check(['extract', 'verify', 'resume', 'proof'].includes(phase), 'invalid_restore_phase');
    return (phase === 'extract' ? extract : phase === 'proof' ? operationalProof : verify)(file, receipt, output, expectedSha, phase === 'resume'); }).then(value => console.log(JSON.stringify(value)))
    .catch(error => { console.error(error.message); process.exitCode = 1; });
}
