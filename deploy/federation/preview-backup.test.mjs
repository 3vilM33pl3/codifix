// SPDX-License-Identifier: AGPL-3.0-or-later
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { validate } from './preview-backup.mjs';
const fixture = () => ({ schema_version: 1, base: '/private/state', directory: '/private/state/dependable-backup-test',
  nas_directory: '/nas/encrypted', nas_mount: '/nas //example/share cifs', recipient: 'A'.repeat(40),
  worker: 'memory-workshop-question-worker.service',
  writers: ['workshop-preview-app', 'homelab-preview-app', 'company-gateway', 'company-pds'].map((s, i) => ({ id: String(i).repeat(64), name: `memory-${s}` })),
  databases: ['workshop', 'homelab', 'gateway'].map((label, i) => ({ id: String(i + 4).repeat(64), name: `memory-company-${label}-db`, user: 'postgres', database: 'synthetic', label })),
  archive_paths: ['memory-company-20260919/gateway/gateway.json'] });
test('explicit preview-only contract accepts a bounded complete topology', () => assert.equal(validate(fixture()).writers.length, 4));
test('unsafe scope, duplicate identities and path escapes are rejected', () => {
  for (const mutate of [c => { c.archive_paths = ['memory-company-20260919/../unrelated']; }, c => { c.archive_paths = ['/etc']; },
    c => { c.writers[0].name = 'production-db'; }, c => { c.writers[0].id = c.databases[0].id; },
    c => { c.directory = '/'; }, c => { c.worker = 'unrelated.service'; }, c => { c.extra = true; },
    c => { c.databases[0].database = 'db; DROP DATABASE other'; }]) {
    const c = fixture(); mutate(c); assert.throws(() => validate(c));
  }
});
