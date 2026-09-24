// SPDX-License-Identifier: AGPL-3.0-or-later
// Split the pinned PostgreSQL-17 pre-data stream at the exact first Memory
// object. Only extension creation runs as the database administrator; all
// Memory-owned objects are created after SET ROLE by the receiver.
const FIRST_MEMORY_OBJECT = '\n--\n-- Name: allocate_timeline_position(); Type: FUNCTION; Schema: public; Owner: -\n--\n';
const extensionCreate = /CREATE EXTENSION IF NOT EXISTS ([a-z_]+) WITH SCHEMA public;/g;
const extensionComment = /COMMENT ON EXTENSION ([a-z_]+) IS '[^'\n]*';/g;
const expected = ['pg_trgm', 'pgcrypto', 'vector'];
function fail() { throw new Error('Trusted receiver schema split rejected'); }

export function splitTrustedPreData(input) {
  if (!Buffer.isBuffer(input) || input.length > 32 * 1024 * 1024 || input.includes(0)) fail();
  const text = input.toString('utf8'); if (Buffer.from(text).compare(input) !== 0 || text.includes('\r')) fail();
  const index = text.indexOf(FIRST_MEMORY_OBJECT);
  if (index < 0 || text.indexOf(FIRST_MEMORY_OBJECT, index + 1) >= 0) fail();
  const admin = text.slice(0, index), owned = text.slice(index);
  const creates = [...admin.matchAll(extensionCreate)].map(match => match[1]);
  const comments = [...admin.matchAll(extensionComment)].map(match => match[1]);
  if (JSON.stringify(creates) !== JSON.stringify(expected) || JSON.stringify(comments) !== JSON.stringify(expected)
      || /\b(?:CREATE|ALTER|DROP|COPY|INSERT|UPDATE|DELETE|DO)\b/.test(admin
        .replace(extensionCreate, '').replace(extensionComment, ''))
      || /CREATE EXTENSION|COMMENT ON EXTENSION/.test(owned)) fail();
  return { admin: Buffer.from(admin), owned: Buffer.from(owned) };
}
