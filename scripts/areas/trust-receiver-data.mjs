// SPDX-License-Identifier: AGPL-3.0-or-later
// Reduce pg_restore 17 output to the exact COPY/sequence subset accepted by a
// Trust receiver. No source-provided schema or executable DDL survives.
import { SCHEMA_PROFILES, checkedProfile } from './verify-staging.mjs';

const copyStart = /^COPY public\.([a-z0-9_]+) \(([^;\r\n]+)\) FROM stdin;$/;
const sequenceSet = /^SELECT pg_catalog\.setval\('public\.([a-z0-9_]+)', ([0-9]+), (true|false)\);$/;
const meta = /^\\(?:un)?restrict [A-Za-z0-9]+$/;
const ignored = new Set([
  "SET statement_timeout = 0;", "SET lock_timeout = 0;",
  "SET idle_in_transaction_session_timeout = 0;", "SET transaction_timeout = 0;",
  "SET client_encoding = 'UTF8';", 'SET standard_conforming_strings = on;',
  'SET check_function_bodies = false;', 'SET xmloption = content;',
  'SET client_min_messages = warning;', 'SET row_security = off;',
  "SELECT pg_catalog.set_config('search_path', '', false);",
]);

function fail() {
  throw new Error('Trust receiver data selection rejected; no private dump diagnostics disclosed');
}

export function canonicalTrustDataSql(input, profile = SCHEMA_PROFILES[60]) {
  checkedProfile(profile);
  const tableNames = new Set(profile.tables.map(value => value.slice('public.'.length)));
  const sequenceNames = new Set(profile.sequences.map(value => value.name.slice('public.'.length)));
  if (typeof input !== 'string' || Buffer.byteLength(input) > 256 * 1024 * 1024
      || input.includes('\r') || input.includes('\0')) fail();
  const lines = input.split('\n'), copies = new Map(), sequences = new Map();
  for (let index = 0; index < lines.length;) {
    const line = lines[index];
    if (!line || line.startsWith('--') || ignored.has(line) || meta.test(line)) { index++; continue; }
    const start = copyStart.exec(line);
    if (start) {
      const [, table] = start;
      if (!tableNames.has(table) || copies.has(table)) fail();
      const block = [line]; let ended = false;
      for (index++; index < lines.length; index++) {
        block.push(lines[index]);
        if (lines[index] === '\\.') { ended = true; index++; break; }
      }
      if (!ended) fail();
      copies.set(table, block.join('\n'));
      continue;
    }
    const sequence = sequenceSet.exec(line);
    if (sequence) {
      const [, name] = sequence;
      if (!sequenceNames.has(name) || sequences.has(name)) fail();
      sequences.set(name, line); index++; continue;
    }
    fail();
  }
  if (copies.size !== tableNames.size || sequences.size !== sequenceNames.size) fail();
  const output = [
    "SET client_encoding = 'UTF8';",
    'SET standard_conforming_strings = on;',
    "SELECT pg_catalog.set_config('search_path', '', false);",
    ...[...tableNames].sort().map(name => copies.get(name)),
    ...[...sequenceNames].sort().map(name => sequences.get(name)),
    '',
  ].join('\n');
  return output;
}

export function freshTrustReceiverGuard(database = 'memory_trust') {
  if (!/^[a-z][a-z0-9_]{0,62}$/.test(database)) fail();
  return `DO $memory_trust_guard$ BEGIN
IF current_database()<>'${database}'
OR EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND backend_type='client backend')
OR EXISTS(SELECT 1 FROM pg_namespace WHERE nspname NOT IN ('public','information_schema') AND nspname !~ '^pg_')
OR EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_depend d JOIN pg_namespace n ON d.refobjid=n.oid WHERE d.refclassid='pg_namespace'::regclass AND n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_extension WHERE extname<>'plpgsql')
OR EXISTS(SELECT 1 FROM pg_event_trigger) OR EXISTS(SELECT 1 FROM pg_largeobject_metadata)
OR EXISTS(SELECT 1 FROM pg_foreign_server) OR EXISTS(SELECT 1 FROM pg_foreign_data_wrapper)
OR EXISTS(SELECT 1 FROM pg_default_acl) OR EXISTS(SELECT 1 FROM pg_publication)
OR EXISTS(SELECT 1 FROM pg_subscription WHERE subdbid=(SELECT oid FROM pg_database WHERE datname=current_database()))
THEN RAISE EXCEPTION 'Fresh isolated Trust restore target required'; END IF;
END $memory_trust_guard$;\n`;
}
