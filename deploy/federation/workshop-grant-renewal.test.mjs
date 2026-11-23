import test from 'node:test';
import assert from 'node:assert/strict';
import { renewal, serviceConfig } from './workshop-grant-renewal.mjs';
const fixture = () => ({ schema_version: 1, action_id: 'original action', question: 'fixed reviewed template',
  grant: { instance_id: '72d1a39c-29fe-4f8d-94ee-b3a8dba51f85', project_id: 'fb83249e-c78c-43eb-a4ba-928513a9023a',
    principal_id: 'ba0b1627-a3ac-47a1-bc62-08ccd50d8cc1', policy_revision: '79c928c4-59db-4dc6-aade-a055156e8cf3', destination: 'openai',
    expires_at: '2026-09-19T13:00:00Z', sources: ['44e45bda-d0e3-4b92-b699-0835fc297d2d', '4fa476fd-6b78-48de-a196-c0a76160bcc2', '5ba41d65-3d5d-4de2-af67-500ffd8dfd79'].map(id => ({ canonical_id: id, version_id: id, version: 1, evidence_digest: Array(32).fill(1) })) },
  sandbox: { pinned: true } });
test('renewal changes only expiry for exactly seven days and invalidates old profile digest', () => {
  const old = fixture(), value = renewal(Buffer.from(JSON.stringify(old)), '2026-09-19T22:00:00Z');
  assert.equal(value.expires_at, '2026-09-26T22:00:00.000Z'); assert.notEqual(value.sha256, value.old_sha256);
  value.manifest.grant.expires_at = old.grant.expires_at; assert.deepEqual(value.manifest, old);
});
test('source count, destination, instance and existing consent remain bounded', () => {
  for (const modify of [x => x.grant.sources.pop(), x => x.grant.destination = 'different_provider',
    x => x.grant.instance_id = 'ffffffff-ffff-ffff-ffff-ffffffffffff', x => x.grant.expires_at = '2026-10-01T00:00:00Z',
    x => x.grant.sources[1] = x.grant.sources[0], x => x.grant.sources[0].version = 2]) {
    const x = fixture(); modify(x); assert.throws(() => renewal(Buffer.from(JSON.stringify(x)), '2026-09-19T22:00:00Z'));
  }
});
test('service update affects only the interaction hash and never enables autonomous model work', () => {
  const before = '[service]\nread_only = false\n[supervisor.interaction]\nmanifest = "/interaction/work.json"\nsha256 = "old"\n';
  assert.equal(serviceConfig(Buffer.from(before), 'old', 'new').toString(), before.replace('"old"', '"new"'));
  assert.throws(() => serviceConfig(Buffer.from(`${before}[supervisor.model_work]\nmanifest="x"\n`), 'old', 'new'));
  assert.throws(() => serviceConfig(Buffer.from(before), 'wrong', 'new'));
});
