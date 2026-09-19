#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// A descriptor binds an isolated restore, not a live connection or import consent.
import { constants } from 'node:fs';
import { open, lstat, link, unlink } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const digest = /^[0-9a-f]{64}$/;
const uuid = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const fields = ['schema_version', 'kind', 'source_instance', 'snapshot_id', 'container_name', 'container_id', 'image_id', 'database', 'snapshot_manifest', 'restore_evidence', 'database_archive'];

export async function privateInput(file, maxBytes = Infinity, json = false) {
  if (!isAbsolute(file)) throw Error('Restore bindings require absolute protected paths');
  let ancestor = dirname(file);
  for (;;) {
    const entry = await lstat(ancestor);
    if (!entry.isDirectory() || ![0, process.getuid()].includes(entry.uid)
      || ((entry.mode & 0o022) !== 0 && (entry.mode & 0o1000) === 0)) throw Error('Untrusted restore binding ancestry');
    const parent = dirname(ancestor); if (parent === ancestor) break; ancestor = parent;
  }
  const handle = await open(file, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const info = await handle.stat();
    if (!info.isFile() || info.uid !== process.getuid() || (info.mode & 0o077) !== 0
      || info.nlink !== 1 || info.size > maxBytes) throw Error('Restore bindings must be bounded owner-only single-link files');
    const hash = createHash('sha256'), chunks = [];
    for await (const chunk of handle.createReadStream({ autoClose: false })) {
      hash.update(chunk); if (json) chunks.push(chunk);
    }
    let value;
    if (json) { try { value = JSON.parse(Buffer.concat(chunks).toString('utf8')); }
      catch { throw Error('Invalid restore binding JSON'); } }
    return { sha256: hash.digest('hex'), value };
  } finally { await handle.close(); }
}

export function validateSandbox(metadata, container) {
  if (!/^memory-four-area-restore-[a-z0-9-]+$/.test(container)
    || metadata.Name !== `/${container}` || !metadata.State?.Running
    || metadata.HostConfig?.NetworkMode !== 'none' || !metadata.HostConfig?.ReadonlyRootfs
    || metadata.HostConfig?.Privileged || metadata.HostConfig?.PidMode === 'host'
    || metadata.HostConfig?.IpcMode === 'host'
    || Object.keys(metadata.NetworkSettings?.Networks || {}).some(name => name !== 'none')
    || Object.values(metadata.NetworkSettings?.Ports || {}).some(value => value !== null))
    throw Error('Restore target must be an explicitly named network-none, read-only-root sandbox without published ports or host privileges');
}

export function inspectContainer(identity) {
  const inspected = spawnSync('docker', ['inspect', '--format', '{{json .}}', identity], { encoding: 'utf8', maxBuffer: 1024 * 1024 });
  if (inspected.status !== 0) throw Error('Restore sandbox unavailable; diagnostics suppressed');
  try { return JSON.parse(inspected.stdout); } catch { throw Error('Invalid restore sandbox metadata'); }
}

export function validateDescriptor(value) {
  if (!value || Object.keys(value).length !== fields.length || Object.keys(value).some(key => !fields.includes(key))
    || value.schema_version !== 1 || value.kind !== 'verified_isolated_restore'
    || !uuid.test(value.source_instance) || !uuid.test(value.snapshot_id)
    || !/^memory-four-area-restore-[a-z0-9-]+$/.test(value.container_name)
    || !digest.test(value.container_id) || !/^sha256:[0-9a-f]{64}$/.test(value.image_id)
    || !/^memory_restore(?:_[a-z0-9_]+)?$/.test(value.database)) throw Error('Invalid explicit restore descriptor');
  for (const key of ['snapshot_manifest', 'restore_evidence', 'database_archive']) {
    const binding = value[key];
    if (!binding || Object.keys(binding).length !== 2 || !isAbsolute(binding.path ?? '') || !digest.test(binding.sha256))
      throw Error('Invalid digest-bound restore artifact');
  }
  if (new Set(['snapshot_manifest', 'restore_evidence', 'database_archive'].map(key => value[key].path)).size !== 3)
    throw Error('Restore artifact paths must be distinct');
  return value;
}

export function validateBindings(descriptor, manifest, evidence) {
  const archive = manifest?.artifacts?.filter(item => item.name === 'database.pgdump.gpg');
  if (manifest?.schema_version !== 1 || archive?.length !== 1 || archive[0].sha256 !== descriptor.database_archive.sha256
    || evidence?.schema_version !== 2 || evidence.all_table_row_hashes_match !== true || evidence.all_sequence_states_match !== true
    || !Array.isArray(evidence.restored_table_inventory) || !evidence.restored_table_inventory.length
    || !Array.isArray(evidence.restored_sequence_inventory) || evidence.network !== 'none' || evidence.published_ports !== false
    || evidence.invalid_indexes !== 0 || evidence.unvalidated_constraints !== 0
    || evidence.container_id !== descriptor.container_id || evidence.image_id !== descriptor.image_id
    || evidence.container_name !== descriptor.container_name || evidence.database !== descriptor.database
    || evidence.snapshot_manifest_sha256 !== descriptor.snapshot_manifest.sha256
    || evidence.database_archive_sha256 !== descriptor.database_archive.sha256)
    throw Error('Restore descriptor, snapshot and verified row/sequence evidence do not agree');
}

export async function loadRestoreDescriptor(file, inspect = inspectContainer) {
  const loaded = await privateInput(file, 64 * 1024, true);
  const descriptor = validateDescriptor(loaded.value);
  const [snapshot, evidence, archive] = await Promise.all([
    privateInput(descriptor.snapshot_manifest.path, 1024 * 1024, true),
    privateInput(descriptor.restore_evidence.path, 8 * 1024 * 1024, true),
    privateInput(descriptor.database_archive.path),
  ]);
  if (snapshot.sha256 !== descriptor.snapshot_manifest.sha256 || evidence.sha256 !== descriptor.restore_evidence.sha256
    || archive.sha256 !== descriptor.database_archive.sha256) throw Error('Restore artifact digest changed');
  validateBindings(descriptor, snapshot.value, evidence.value);
  const metadata = await inspect(descriptor.container_id);
  validateSandbox(metadata, descriptor.container_name);
  if (metadata.Id !== descriptor.container_id || metadata.Image !== descriptor.image_id) throw Error('Restore container or image identity changed');
  return { descriptor, descriptor_sha256: loaded.sha256, metadata };
}

async function publishDescriptor(output, descriptor) {
  if (!isAbsolute(output)) throw Error('Descriptor output must be an absolute private path');
  const parent = await lstat(dirname(output));
  if (!parent.isDirectory() || parent.uid !== process.getuid() || (parent.mode & 0o777) !== 0o700)
    throw Error('Descriptor parent must be owner-only mode 0700');
  const temporary = `${output}.partial-${randomUUID()}`;
  const file = await open(temporary, 'wx', 0o600);
  try {
    await file.writeFile(JSON.stringify(descriptor, null, 2) + '\n'); await file.sync(); await file.close();
    // Validate all paths, digests and current container before exclusive publication.
    await loadRestoreDescriptor(temporary);
    await link(temporary, output); await unlink(temporary);
    const directory = await open(dirname(output), 'r');
    try { await directory.sync(); } finally { await directory.close(); }
  } catch (error) { await file.close().catch(() => {}); await unlink(temporary).catch(() => {}); throw error; }
}

export async function createRestoreDescriptor(directory, sourceInstance, snapshotId, output) {
  const root = resolve(directory);
  const snapshotPath = join(root, 'snapshot.json'), evidencePath = join(root, 'restore-evidence.json'), archivePath = join(root, 'database.pgdump.gpg');
  const [snapshot, evidence, archive] = await Promise.all([
    privateInput(snapshotPath, 1024 * 1024, true), privateInput(evidencePath, 8 * 1024 * 1024, true), privateInput(archivePath),
  ]);
  const proof = evidence.value;
  const descriptor = validateDescriptor({ schema_version: 1, kind: 'verified_isolated_restore',
    source_instance: sourceInstance, snapshot_id: snapshotId, container_name: proof.container_name,
    container_id: proof.container_id, image_id: proof.image_id, database: proof.database,
    snapshot_manifest: { path: snapshotPath, sha256: snapshot.sha256 }, restore_evidence: { path: evidencePath, sha256: evidence.sha256 },
    database_archive: { path: archivePath, sha256: archive.sha256 } });
  validateBindings(descriptor, snapshot.value, proof);
  await publishDescriptor(output, descriptor);
  return { descriptor_written: true, descriptor_sha256: (await privateInput(output)).sha256, cutover_ready: false };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2), options = {};
  try {
    for (let i = 0; i < args.length; i += 2) {
      if (!['--snapshot-directory', '--source-instance', '--snapshot-id', '--output'].includes(args[i])
        || !args[i + 1] || options[args[i]]) throw Error('Invalid descriptor arguments');
      options[args[i]] = args[i + 1];
    }
    if (Object.keys(options).length !== 4) throw Error('Require --snapshot-directory PATH --source-instance UUID --snapshot-id UUID --output PATH');
    console.log(JSON.stringify(await createRestoreDescriptor(options['--snapshot-directory'], options['--source-instance'], options['--snapshot-id'], options['--output'])));
  } catch { console.error('Restore descriptor failed; require private digest-matching artifacts and verified isolated row/sequence evidence. No database diagnostics disclosed.'); process.exitCode = 1; }
}
