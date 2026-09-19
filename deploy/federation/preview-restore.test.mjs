import test from 'node:test';
import assert from 'node:assert/strict';
import { allowedArchiveEntries, roleRestoreSql } from './preview-restore.mjs';
const name = 'dependable-backup-test';
const required = ['intent.json', 'workshop.dump', 'homelab.dump', 'gateway.dump', 'workshop.roles.sql', 'homelab.roles.sql', 'gateway.roles.sql'];
const listing = extra => [...required.map(file => `${name}/${file}`), ...extra].map(file => `-rw------- 1000/1000 1 2026-09-19 22:00 ${file}`).join('\n');
test('archive validator allows only complete explicit regular-file scope', () => {
  assert.equal(allowedArchiveEntries(listing(['memory-company-20260919/pds/data']), ['memory-company-20260919/pds/data'], name).length, 8);
  for (const changed of ['../escape', '/etc/passwd', 'memory-company-20260919/pds/data/../escape', 'memory-company-20260919/unreviewed'])
    assert.throws(() => allowedArchiveEntries(listing([changed]), ['memory-company-20260919/pds/data'], name));
  assert.throws(() => allowedArchiveEntries(listing([]), ['memory-company-20260919/pds/data'], name));
  assert.throws(() => allowedArchiveEntries(listing([`${name}/intent.json`]), [], name), /archive_duplicate_path/u);
  assert.throws(() => allowedArchiveEntries(listing([]).replace('-rw-------', 'lrwxrwxrwx'), [], name), /archive_non_regular/u);
});
test('globals restoration omits only the known existing postgres CREATE', () => {
  const result = roleRestoreSql("CREATE ROLE postgres;\nALTER ROLE postgres WITH SUPERUSER;\nCREATE ROLE memory_owner;\nGRANT memory_owner TO postgres;");
  assert.equal(result.existing_postgres_create_omitted, 1);
  assert.ok(result.sql.includes('ALTER ROLE postgres')); assert.ok(result.sql.includes('CREATE ROLE memory_owner;')); assert.ok(result.sql.includes('GRANT memory_owner TO postgres;'));
  assert.throws(() => roleRestoreSql('CREATE ROLE postgres;\nCREATE ROLE postgres;'), /unsupported_global_role_state/u);
  assert.throws(() => roleRestoreSql("CREATE TABLESPACE unknown LOCATION '/private';"), /unsupported_global_role_state/u);
});
