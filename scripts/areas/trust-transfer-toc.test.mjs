// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { SCHEMA60_TABLES, SCHEMA60_SEQUENCES } from './verify-staging.mjs';
import { TRUST_TOC_MAX_BYTES, validateTrustDataToc } from './trust-transfer-toc.mjs';

// pg_restore 17 data-only list grammar. Only generated identifiers are used;
// empty/runtime table contents are not represented or approved by this fixture.
function fixture() {
  const header = [
    ';', '; Archive created at 2026-09-22 12:00:00 UTC',
    ';     dbname: synthetic_trust', ';     TOC Entries: 121',
    ';     Compression: gzip', ';     Dump Version: 1.16-0',
    ';     Format: CUSTOM', ';     Integer: 4 bytes', ';     Offset: 8 bytes',
    ';     Dumped from database version: 17.11', ';     Dumped by pg_dump version: 17.11',
    ';', '; Selected TOC Entries:', ';',
  ];
  const tables = SCHEMA60_TABLES.map((name, index) => `${1000 + index}; 0 ${20000 + index} TABLE DATA public ${name.slice(7)} postgres`);
  const sequences = SCHEMA60_SEQUENCES.map((sequence, index) => `${2000 + index}; 0 0 SEQUENCE SET public ${sequence.name.slice(7)} postgres`);
  return { header, tables, sequences, text: [...header, ...tables, ...sequences, ''].join('\n') };
}
const reject = text => assert.throws(() => validateTrustDataToc(text), /Trust transfer data TOC rejected; private diagnostics suppressed/);

test('complete schema60 data-only inventory produces a comment-free exact selection list', () => {
  const f = fixture(), result = validateTrustDataToc(f.text);
  assert.equal(result.table_count, 114); assert.equal(result.sequence_count, 4);
  assert.equal(result.entries.length, 118);
  assert.equal(result.selection_list, [...f.tables, ...f.sequences, ''].join('\n'));
  assert.equal(result.contents_verified, false); assert.equal(result.restoration_authorized, false);
  assert.deepEqual(validateTrustDataToc(result.selection_list), result);
  assert.ok(!result.selection_list.includes('synthetic_trust'));
});

test('catalog order and nonconsecutive IDs are preserved; sequence zero sentinels may repeat', () => {
  const f = fixture(), text = [...f.sequences.reverse(), ...f.tables.reverse(), ''].join('\n');
  const result = validateTrustDataToc(text);
  assert.equal(result.selection_list, text);
  assert.equal(result.entries.filter(entry => entry.object_oid === 0).length, 4);
});

test('missing runtime tables or sequences cannot be silently omitted as empty', () => {
  const f = fixture();
  for (const line of [...f.tables, ...f.sequences]) reject(f.text.replace(line + '\n', ''));
});

test('extra executable, large-object, materialized-view, cross-area and foreign-schema entries fail', () => {
  for (const line of [
    '3000; 1255 90000 FUNCTION public malicious() postgres',
    '3000; 2620 90000 TRIGGER public memory_entries malicious postgres',
    '3000; 0 90000 BLOB - 90000 postgres',
    '3000; 0 0 BLOBS - BLOBS -',
    '3000; 0 90000 MATERIALIZED VIEW DATA public leaks postgres',
    '3000; 0 90000 TABLE DATA public workshop_only postgres',
    '3000; 0 90000 TABLE DATA workshop memory_entries postgres',
    '3000; 0 90000 TABLE DATA mab_full_20260516_150014 memory_entries postgres',
    '3000; 0 0 ACL public memory_entries postgres',
    '3000; 0 0 COMMENT public memory_entries postgres',
    '3000; 2615 90000 SCHEMA - malicious postgres',
    '3000; 3079 90000 EXTENSION - malicious postgres',
  ]) reject(fixture().text + line + '\n');
});

test('duplicate TOC IDs, positive object OIDs, table names and sequence names fail', () => {
  const f = fixture();
  reject(f.text.replace(f.tables[1], f.tables[1].replace('1001;', '1000;')));
  reject(f.text.replace(f.tables[1], f.tables[1].replace('20001', '20000')));
  reject(f.text.replace(f.tables[1], f.tables[1].replace(SCHEMA60_TABLES[1].slice(7), SCHEMA60_TABLES[0].slice(7))));
  reject(f.text.replace(f.sequences[1], f.sequences[1].replace(SCHEMA60_SEQUENCES[1].name.slice(7), SCHEMA60_SEQUENCES[0].name.slice(7))));
  reject(f.text + f.tables[0] + '\n');
  reject(f.text.replace(f.sequences[0], f.sequences[0].replace('2000;', '1000;')));
});

test('malformed, injected, quoted, out-of-range and unsafe identifiers fail', () => {
  const f = fixture();
  for (const replacement of [
    f.tables[0].replace('1000;', '0;'), f.tables[0].replace('1000;', '4294967296;'),
    f.tables[0].replace('1000;', '01000;'), f.tables[0].replace('1000;', '-1;'),
    f.tables[0].replace('; 0 ', '; 1259 '), f.tables[0].replace('20000', '0'),
    f.tables[0].replace('20000', '4294967296'), f.tables[0].replace('20000', '020000'),
    f.tables[0].replace('public ', '"public" '), f.tables[0].replace('_sqlx_migrations', '"_sqlx_migrations"'),
    f.tables[0].replace('_sqlx_migrations', '../private'), f.tables[0].replace('postgres', 'postgres; SELECT 1;'),
    f.tables[0].replace('postgres', '"private owner"'), f.tables[0] + ' unexpected',
    ' ' + f.tables[0], f.tables[0].replace('TABLE DATA', 'TABLE\tDATA'),
    'SELECT pg_sleep(10);', '\\connect forbidden',
  ]) reject(f.text.replace(f.tables[0], replacement));
  reject(f.text.replace(f.sequences[0], f.sequences[0].replace('; 0 0 ', '; 0 20000 ')));
});

test('oversize, invalid text and incomplete formatting fail without reflecting private content', () => {
  for (const value of [null, Buffer.from(fixture().text), '', fixture().text.slice(0, -1),
    fixture().text + '\0', fixture().text.replace('postgres', 'prívate'),
    fixture().text.replace(/\n/g, '\r\n'), ';' + 'x'.repeat(1024) + '\n' + fixture().text,
    ';\n'.repeat(2048) + fixture().text, ';' + 'x'.repeat(TRUST_TOC_MAX_BYTES) + '\n']) reject(value);
  const secret = 'PRIVATE_UNTRUSTED_TEXT';
  assert.throws(() => validateTrustDataToc(fixture().text + secret + '\n'), error => !error.message.includes(secret));
});

test('input comments cannot inject selected entries or authorize restoration', () => {
  const f = fixture();
  const result = validateTrustDataToc('; SELECT malicious();\n; 9999; 1255 90000 FUNCTION public malicious() postgres\n' + f.text);
  assert.equal(result.selection_list, [...f.tables, ...f.sequences, ''].join('\n'));
  assert.equal(result.restoration_authorized, false);
});
