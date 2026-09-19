import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TARGET, validateDatabase } from '../homelab-loopback-relay.mjs';

test('preview relay has fixed independent loopback targets', () => {
  assert.equal(TARGET.webHost, '127.0.0.2');
  assert.equal(TARGET.webPort, 4261);
  assert.equal(TARGET.databasePort, 55461);
  assert.equal(TARGET.container, 'memory-homelab-preview-app-20260919');
  assert.ok(Object.isFrozen(TARGET));
});
test('database target is an explicit Docker-private IPv4 address only', () => {
  for (const address of ['172.16.0.2', '172.31.255.254']) assert.equal(validateDatabase(address), address);
  for (const address of ['172.32.0.2', '172.15.0.2', '172.20.999.1', '127.0.0.1',
    'localhost', 'https://example.com', '169.254.169.254', '::1', '172.20.0.1:5432', null, {}]) {
    assert.throws(() => validateDatabase(address));
  }
});
