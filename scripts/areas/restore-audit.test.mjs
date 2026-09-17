import assert from 'node:assert/strict';
import test from 'node:test';
import { Readable } from 'node:stream';
import { copyInventory, validateSandbox, archiveTimezone, inventoryDifferences } from './restore-audit.mjs';

const inventory = text => copyInventory(Readable.from([text]));
test('COPY multiset hashes are order independent and preserve changed values', async () => {
  const prefix = 'COPY public.example (id, value) FROM stdin;\n';
  const one = await inventory(prefix + '1\tsecret-one\n2\tsecret-two\n\\.\n');
  const two = await inventory(prefix + '2\tsecret-two\n1\tsecret-one\n\\.\n');
  assert.deepEqual(one, two); assert.equal(one[0].rows, 2);
  assert.ok(!JSON.stringify(one).includes('secret-one'));
  assert.notDeepEqual(one, await inventory(prefix + '1\tdifferent\n2\tsecret-two\n\\.\n'));
});
test('reject truncated, duplicate, empty or unsupported COPY input', async () => {
  for (const text of ['', 'COPY public.a (id) FROM stdin;\n1\n',
    'COPY public.a (id) FROM stdin;\n\\.\nCOPY public.a (id) FROM stdin;\n\\.\n',
    'COPY public.a FROM PROGRAM anything;\n\\.\n']) await assert.rejects(inventory(text));
});
test('full database audit includes historical non-public schemas', async () => {
  const result = await inventory('COPY mab_full_20260516_150014.memory_entries (id) FROM stdin;\n1\n\\.\n');
  assert.equal(result[0].table, 'mab_full_20260516_150014.memory_entries');
  assert.equal(result[0].rows, 1);
});
test('restore only accepts a sandbox without network or exposed ports', () => {
  const name = 'memory-four-area-restore-test';
  const metadata = { Name: '/' + name, State: { Running: true },
    HostConfig: { NetworkMode: 'none', ReadonlyRootfs: true }, NetworkSettings: { Ports: { '5432/tcp': null } } };
  validateSandbox(metadata, name);
  assert.throws(() => validateSandbox(metadata, 'production'));
  assert.throws(() => validateSandbox({ ...metadata, HostConfig: { ...metadata.HostConfig, NetworkMode: 'bridge' } }, name));
  assert.throws(() => validateSandbox({ ...metadata, NetworkSettings: { Ports: { '5432/tcp': [{ HostPort: '5432' }] } } }, name));
});
test('timestamp rendering must match the archive, not the restore host timezone', () => {
  assert.equal(archiveTimezone({ database_session_timezone: 'UTC' }), 'UTC');
  assert.equal(archiveTimezone({}, 'Europe/London'), 'Europe/London');
  assert.throws(() => archiveTimezone({}));
  assert.throws(() => archiveTimezone({}, 'UTC -c anything=1'));
  assert.throws(() => archiveTimezone({ database_session_timezone: 'UTC' }, 'Europe/London'));
});
test('metadata-only differences detect missing and modified tables', () => {
  const a = { table: 'public.a', rows: 1, row_set_sha256: 'a' };
  assert.deepEqual(inventoryDifferences([a], [a]), []);
  assert.equal(inventoryDifferences([a], [])[0].actual, null);
  assert.equal(inventoryDifferences([a], [{ ...a, rows: 2 }]).length, 1);
});
