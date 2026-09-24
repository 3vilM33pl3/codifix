// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { splitTrustedPreData } from './trust-receiver-schema.mjs';

const marker = '\n--\n-- Name: allocate_timeline_position(); Type: FUNCTION; Schema: public; Owner: -\n--\n';
const fixture = (prefix = '', owned = 'CREATE FUNCTION public.allocate_timeline_position() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;\n') =>
  Buffer.from(`${prefix}CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;\nCOMMENT ON EXTENSION pg_trgm IS 'trgm';\n`
  + `CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;\nCOMMENT ON EXTENSION pgcrypto IS 'crypto';\n`
  + `CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA public;\nCOMMENT ON EXTENSION vector IS 'vector';\n${marker}${owned}`);

test('trusted pre-data split keeps only extensions in the administrator section', () => {
  const result = splitTrustedPreData(fixture('SET statement_timeout = 0;\n'));
  assert.match(result.admin.toString(), /CREATE EXTENSION.*vector/s);
  assert.doesNotMatch(result.admin.toString(), /CREATE FUNCTION/);
  assert.match(result.owned.toString(), /CREATE FUNCTION/);
});

test('schema split rejects executable prefix, wrong order and extension commands after role boundary', () => {
  assert.throws(() => splitTrustedPreData(fixture('DO $$ BEGIN END $$;\n')), /rejected/);
  assert.throws(() => splitTrustedPreData(Buffer.from(fixture().toString().replace('pg_trgm', 'vector'))), /rejected/);
  assert.throws(() => splitTrustedPreData(fixture('', 'CREATE EXTENSION vector;')), /rejected/);
});
