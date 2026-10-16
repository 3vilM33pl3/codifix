// SPDX-License-Identifier: AGPL-3.0-or-later
// Dependency execution proof only; no accounts, filesystem DB or TCP listener.
const { readFileSync } = require('node:fs');
const { dirname, join } = require('node:path');
if (process.platform !== 'win32' || process.versions.node !== '24.18.1') {
  throw new Error('pinned native Windows Node required');
}
const entry = require.resolve('@atproto/pds');
const metadata = JSON.parse(readFileSync(join(dirname(entry), '..', 'package.json'), 'utf8'));
if (metadata.version !== '0.5.27') throw new Error('pinned PDS required');
const Database = require('better-sqlite3');
const database = new Database(':memory:');
try {
  const version = database.prepare('select sqlite_version() as version').get().version;
  require('@atproto/pds');
  console.log(JSON.stringify({ native_sqlite: version, pds: metadata.version, node: process.versions.node }));
} finally {
  database.close();
}
