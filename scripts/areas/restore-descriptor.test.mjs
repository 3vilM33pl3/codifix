// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, writeFile, chmod, rm, symlink, link } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { loadRestoreDescriptor, validateDescriptor, validateBindings, privateInput, validateSandbox } from './restore-descriptor.mjs';

const hash = value => createHash('sha256').update(value).digest('hex');
const id = 'a'.repeat(64), image = `sha256:${'b'.repeat(64)}`, name = 'memory-four-area-restore-synthetic';
const metadata = () => ({ Id: id, Image: image, Name: `/${name}`, State: { Running: true },
  HostConfig: { NetworkMode: 'none', ReadonlyRootfs: true }, NetworkSettings: { Ports: { '5432/tcp': null }, Networks: { none: {} } } });
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'memory-restore-descriptor-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const archive = Buffer.from('synthetic encrypted archive placeholder');
  const manifest = { schema_version: 1, artifacts: [{ name: 'database.pgdump.gpg', sha256: hash(archive) }] };
  const encode = value => JSON.stringify(value) + '\n';
  const evidence = { schema_version: 2, database: 'memory_restore', container_name: name, container_id: id, image_id: image,
    network: 'none', published_ports: false, all_table_row_hashes_match: true, all_sequence_states_match: true,
    restored_table_inventory: [{ table: 'public.example', rows: 0, row_set_sha256: hash('') }], restored_sequence_inventory: [],
    invalid_indexes: 0, unvalidated_constraints: 0, snapshot_manifest_sha256: hash(encode(manifest)), database_archive_sha256: hash(archive) };
  const descriptor = { schema_version: 1, kind: 'verified_isolated_restore',
    source_instance: '00000000-0000-0000-0000-000000000001', snapshot_id: '00000000-0000-0000-0000-000000000002',
    container_name: name, container_id: id, image_id: image, database: 'memory_restore',
    snapshot_manifest: { path: join(root, 'snapshot.json'), sha256: hash(encode(manifest)) },
    restore_evidence: { path: join(root, 'restore-evidence.json'), sha256: hash(encode(evidence)) },
    database_archive: { path: join(root, 'database.pgdump.gpg'), sha256: hash(archive) } };
  for (const [file, bytes] of [[descriptor.snapshot_manifest.path, encode(manifest)], [descriptor.restore_evidence.path, encode(evidence)],
    [descriptor.database_archive.path, archive], [join(root, 'descriptor.json'), encode(descriptor)]]) await writeFile(file, bytes, { mode: 0o600, flag: 'wx' });
  return { root, file: join(root, 'descriptor.json'), descriptor, manifest, evidence };
}

test('verified descriptors bind every artifact and exact isolated container identity', async t => {
  const f = await fixture(t);
  const result = await loadRestoreDescriptor(f.file, async requested => { assert.equal(requested, id); return metadata(); });
  assert.equal(result.descriptor.database, 'memory_restore');
  assert.match(result.descriptor_sha256, /^[0-9a-f]{64}$/);
  for (const mutate of [m => ({ ...m, Id: 'c'.repeat(64) }), m => ({ ...m, Image: `sha256:${'c'.repeat(64)}` }),
    m => ({ ...m, HostConfig: { ...m.HostConfig, NetworkMode: 'bridge' } }), m => ({ ...m, State: { Running: false } })])
    await assert.rejects(loadRestoreDescriptor(f.file, () => mutate(metadata())), /identity|sandbox/);
});

test('legacy row-only proof and mismatched database or snapshot evidence are rejected', async t => {
  const f = await fixture(t);
  for (const patch of [{ schema_version: 1 }, { all_sequence_states_match: false }, { restored_sequence_inventory: undefined },
    { database: 'memory' }, { database_archive_sha256: 'c'.repeat(64) }, { snapshot_manifest_sha256: 'c'.repeat(64) },
    { image_id: `sha256:${'c'.repeat(64)}` }])
    assert.throws(() => validateBindings(f.descriptor, f.manifest, { ...f.evidence, ...patch }), /do not agree/);
  assert.throws(() => validateDescriptor({ ...f.descriptor, database: 'memory' }), /descriptor/);
  assert.throws(() => validateDescriptor({ ...f.descriptor, token: 'never-accepted' }), /descriptor/);
  assert.throws(() => validateDescriptor({ ...f.descriptor, container_id: name }), /descriptor/);
});

test('altered artifacts fail before any database access', async t => {
  const f = await fixture(t);
  await writeFile(f.descriptor.database_archive.path, 'changed synthetic bytes');
  await assert.rejects(loadRestoreDescriptor(f.file, () => { throw Error('must not inspect'); }), /digest changed/);
});

test('descriptor private files reject modes, hard links, symlinks and unsafe ancestors', async t => {
  const f = await fixture(t);
  await chmod(f.file, 0o644); await assert.rejects(privateInput(f.file), /owner-only/); await chmod(f.file, 0o600);
  const hard = join(f.root, 'hard.json'); await link(f.file, hard);
  await assert.rejects(privateInput(f.file), /single-link/); await rm(hard);
  const soft = join(f.root, 'soft.json'); await symlink(f.file, soft); await assert.rejects(privateInput(soft));
  await chmod(f.root, 0o777); await assert.rejects(privateInput(f.file), /ancestry/); await chmod(f.root, 0o700);
  await assert.rejects(privateInput('relative.json'), /absolute/);
});

test('host privileges, attached networks and port bindings are refused', () => {
  for (const patch of [{ HostConfig: { ...metadata().HostConfig, Privileged: true } },
    { HostConfig: { ...metadata().HostConfig, PidMode: 'host' } }, { HostConfig: { ...metadata().HostConfig, IpcMode: 'host' } },
    { NetworkSettings: { Networks: { none: {}, bridge: {} } } }, { NetworkSettings: { Ports: { '5432/tcp': [{ HostPort: '5432' }] } } }])
    assert.throws(() => validateSandbox({ ...metadata(), ...patch }, name), /sandbox/);
});
