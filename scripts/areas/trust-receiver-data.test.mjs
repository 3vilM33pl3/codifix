// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { SCHEMA60_SEQUENCES, SCHEMA60_TABLES } from './verify-staging.mjs';
import { canonicalTrustDataSql, freshTrustReceiverGuard } from './trust-receiver-data.mjs';

function dump(extra = '') {
  const tables = SCHEMA60_TABLES.map(table => `-- table\nCOPY ${table} (id) FROM stdin;\n\\.\n`).join('');
  const sequences = SCHEMA60_SEQUENCES.map(({ name }) => `SELECT pg_catalog.setval('${name}', 1, false);\n`).join('');
  return `-- PostgreSQL database dump\n\\restrict Token123\nSET statement_timeout = 0;\nSET transaction_timeout = 0;\n${tables}${sequences}${extra}\\unrestrict Token123\n`;
}

test('canonical receiver data keeps only the exact data and sequence allowlist', () => {
  const sql = canonicalTrustDataSql(dump());
  assert.doesNotMatch(sql, /transaction_timeout|\\restrict|PostgreSQL database dump/);
  assert.equal((sql.match(/^COPY /gm) ?? []).length, SCHEMA60_TABLES.length);
  assert.equal((sql.match(/^SELECT pg_catalog\.setval/gm) ?? []).length, SCHEMA60_SEQUENCES.length);
});

test('receiver data rejects executable, duplicate, missing and unknown entries', () => {
  assert.throws(() => canonicalTrustDataSql(dump('DO $$ BEGIN END $$;\n')), /rejected/);
  assert.throws(() => canonicalTrustDataSql(dump().replace('\\unrestrict Token123',
    "COPY public.projects (id) FROM stdin;\n\\.\n\\unrestrict Token123")), /rejected/);
  assert.throws(() => canonicalTrustDataSql(dump().replace(/COPY public\.projects[\s\S]*?\\\.\n/, '')), /rejected/);
  assert.throws(() => canonicalTrustDataSql(dump().replace('COPY public.projects', 'COPY public.not_allowed')), /rejected/);
});

test('fresh receiver guard is exact-database bound', () => {
  assert.match(freshTrustReceiverGuard(), /current_database\(\)<>'memory_trust'/);
  assert.match(freshTrustReceiverGuard('memory_staging_trust'), /memory_staging_trust/);
  assert.throws(() => freshTrustReceiverGuard('memory-trust'), /rejected/);
});
