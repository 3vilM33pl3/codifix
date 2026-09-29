// SPDX-License-Identifier: AGPL-3.0-or-later
// Pure validation of an exact schema60 data-only pg_restore --list inventory.
// A valid TOC neither authenticates the archive nor verifies its private rows.
import { SCHEMA_PROFILES, checkedProfile } from './verify-staging.mjs';

export const TRUST_TOC_MAX_BYTES = 256 * 1024;
const MAX_LINES = 2048;
const MAX_LINE_BYTES = 1024;
const UINT32_MAX = 4294967295;
const invalid = () => { throw Error('Trust transfer data TOC rejected; private diagnostics suppressed'); };

function unsigned(value, allowZero) {
  if (!/^(?:0|[1-9][0-9]{0,9})$/.test(value)) invalid();
  const result = Number(value);
  if (result > UINT32_MAX || (!allowZero && result === 0)) invalid();
  return result;
}

/**
 * Accept only the full data-only schema60 table and sequence catalog. Return a
 * freshly rendered pg_restore selection list, never an input comment or SQL.
 *
 * PostgreSQL emits `0 0` catalog/object-OID sentinels for SEQUENCE SET entries;
 * these repeated zero sentinels are not duplicate database object identities.
 * Positive TABLE DATA object OIDs, TOC dump IDs and qualified names are unique.
 *
 * Empty runtime tables MUST still have entries. Their emptiness, provenance,
 * snapshot, schema definitions and actual row bytes require independent checks.
 * A caller must separately bind the exact archive, tool and inspected list; this
 * function does not prevent a caller substituting another archive afterwards.
 */
export function validateTrustDataToc(text, profile = SCHEMA_PROFILES[60]) {
  checkedProfile(profile);
  const tableNames = new Set(profile.tables);
  const sequenceNames = new Set(profile.sequences.map(sequence => sequence.name));
  if (typeof text !== 'string' || Buffer.byteLength(text, 'utf8') > TRUST_TOC_MAX_BYTES
    || !text.endsWith('\n') || /[^\x20-\x7e\n]/.test(text)) invalid();
  const lines = text.split('\n');
  if (lines.length > MAX_LINES || lines.some(line => line.length > MAX_LINE_BYTES)) invalid();
  const entries = [], dumpIds = new Set(), objectOids = new Set(), names = new Set();
  const seenTables = new Set(), seenSequences = new Set();
  for (const line of lines) {
    if (line === '' || line.startsWith(';')) continue;
    // Reject quoted identifiers, other schemas, SQL, unsupported entry kinds,
    // continuation lines and additional fields instead of attempting repairs.
    const match = /^(\d+); (\d+) (\d+) (TABLE DATA|SEQUENCE SET) public ([a-z_][a-z0-9_]{0,62}) ([a-z_][a-z0-9_]{0,62}|-)$/.exec(line);
    if (!match) invalid();
    const dumpId = unsigned(match[1], false), catalogOid = unsigned(match[2], true);
    const objectOid = unsigned(match[3], true), kind = match[4];
    const name = 'public.' + match[5], owner = match[6];
    if (catalogOid !== 0 || dumpIds.has(dumpId) || names.has(name)) invalid();
    if (kind === 'TABLE DATA') {
      if (!tableNames.has(name) || objectOid === 0 || objectOids.has(objectOid)) invalid();
      objectOids.add(objectOid); seenTables.add(name);
    } else {
      if (!sequenceNames.has(name) || objectOid !== 0) invalid();
      seenSequences.add(name);
    }
    dumpIds.add(dumpId); names.add(name);
    entries.push({ dump_id: dumpId, catalog_oid: catalogOid, object_oid: objectOid, kind, name, owner });
    if (entries.length > tableNames.size + sequenceNames.size) invalid();
  }
  if (seenTables.size !== tableNames.size || seenSequences.size !== sequenceNames.size) invalid();
  return {
    schema_version: 1,
    profile: `trust_schema${profile.head}_data_only_toc`,
    table_count: seenTables.size,
    sequence_count: seenSequences.size,
    entries,
    selection_list: entries.map(entry => `${entry.dump_id}; ${entry.catalog_oid} ${entry.object_oid} ${entry.kind} public ${entry.name.slice(7)} ${entry.owner}\n`).join(''),
    contents_verified: false,
    restoration_authorized: false,
  };
}
