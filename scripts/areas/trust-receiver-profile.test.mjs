// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { TRUST_RECEIVER_PROFILE, validateTrustReceiverObservation } from './trust-receiver-profile.mjs';

const observation = () => ({
  format: 'memory-trust-receiver-observation-v1',
  postgres_version: '15.18',
  client_version: '15.18',
  extensions: { vector: '0.8.6', pgcrypto: '1.3', pg_trgm: '1.6' },
});

test('CA Trust receiver profile is exact and inactive', () => {
  assert.equal(TRUST_RECEIVER_PROFILE.database, 'memory_trust');
  assert.equal(TRUST_RECEIVER_PROFILE.role, 'memory_trust');
  assert.equal(TRUST_RECEIVER_PROFILE.service, 'memory-layer@trust.service');
  assert.equal(TRUST_RECEIVER_PROFILE.port, 8091);
  assert.equal(TRUST_RECEIVER_PROFILE.active, false);
  assert.equal(validateTrustReceiverObservation(observation()).postgres_version, '15.18');
});

test('receiver profile rejects version drift, missing extensions and extra fields', () => {
  for (const mutate of [
    value => { value.postgres_version = '15.19'; },
    value => { delete value.extensions.vector; },
    value => { value.extensions.vector = '0.8.5'; },
    value => { value.extensions.extra = '1'; },
    value => { value.unreviewed = true; },
  ]) {
    const value = observation(); mutate(value);
    assert.throws(() => validateTrustReceiverObservation(value), /pinned CA profile/);
  }
});
