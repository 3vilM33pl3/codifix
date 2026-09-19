// SPDX-License-Identifier: AGPL-3.0-or-later
// Rust materialization frames every table, including tables with no rows.
export function archiveFraming(tables, expectedRows) {
  const expected = new Set(tables), seen = new Set();
  let current = null, previous = null, total = 0;
  const fail = () => { throw Error('Materialized archive framing is invalid'); };
  if (!Number.isSafeInteger(expectedRows) || expectedRows < 0 || !expected.size) fail();
  return {
    consume(event) {
      if (event.event === 'table_start') {
        if (current || !expected.has(event.table) || seen.has(event.table)
          || !Number.isSafeInteger(event.row_count) || event.row_count < 0
          || (previous !== null && Buffer.compare(Buffer.from(previous), Buffer.from(event.table)) >= 0)
          || !Array.isArray(event.columns) || !Array.isArray(event.foreign_keys)) fail();
        current = { table: event.table, expected: event.row_count, rows: 0 };
        seen.add(event.table); previous = event.table;
        return false;
      }
      if (event.event === 'table_end') {
        if (!current || event.table !== current.table || event.row_count !== current.expected
          || current.rows !== current.expected) fail();
        current = null; return false;
      }
      if (event.event !== 'original_row' || !current || event.record?.table !== current.table
        || current.rows >= current.expected || total >= expectedRows) fail();
      current.rows++; total++; return true;
    },
    complete(event) {
      if (current || seen.size !== expected.size || total !== expectedRows
        || event.event !== 'complete' || event.table_count !== expected.size || event.row_count !== total) fail();
    },
  };
}
