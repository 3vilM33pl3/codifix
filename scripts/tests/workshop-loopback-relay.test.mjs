import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_WEB_CONNECTIONS, MAX_DATABASE_CONNECTIONS, WEB_IDLE_MS, validateTargets } from '../workshop-loopback-relay.mjs';

test('preview relay keeps finite connection and existing WebSocket idle bounds', () => {
  assert.equal(MAX_WEB_CONNECTIONS, 32);
  assert.equal(MAX_DATABASE_CONNECTIONS, 64);
  assert.equal(WEB_IDLE_MS, 300_000);
});

test('preview relay only admits internal Docker IPv4 targets, never arbitrary URLs', () => {
  for (const host of ['172.16.0.2', '172.31.255.254']) {
    const config = { database: host, application: host };
    assert.equal(validateTargets(config), config);
  }
  for (const host of ['127.0.0.1', 'localhost', '169.254.169.254', '172.15.0.2', '172.32.0.2', '172.16.999.1', '::1', 'https://example.com', '172.16.0.2:5432']) {
    assert.throws(() => validateTargets({ database: host }));
    assert.throws(() => validateTargets({ application: host }));
  }
  for (const value of [null, '172.16.0.2', []]) assert.throws(() => validateTargets(value));
});
