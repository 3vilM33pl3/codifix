// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { TRUST_CA_OBSERVED_PROFILE, TRUST_RECEIVER_PROFILE, validateCaObservedPostgres15,
  validateTrustReceiverObservation } from './trust-receiver-profile.mjs';

const observation = () => ({
  format: 'memory-trust-receiver-observation-v1',
  postgres_version: '17.11',
  client_version: '17.11',
  extensions: { vector: '0.8.6', pgcrypto: '1.3', pg_trgm: '1.6' },
});

test('selected CA Trust receiver profile is PostgreSQL 17 and inactive', () => {
  assert.equal(TRUST_RECEIVER_PROFILE.postgres_major, 17);
  assert.equal(TRUST_RECEIVER_PROFILE.database, 'memory_trust');
  assert.equal(TRUST_RECEIVER_PROFILE.database_port, 5433);
  assert.equal(TRUST_RECEIVER_PROFILE.role, 'memory_trust');
  assert.equal(TRUST_RECEIVER_PROFILE.service, 'codifix@trust.service');
  assert.equal(TRUST_RECEIVER_PROFILE.port, 8091);
  assert.equal(TRUST_RECEIVER_PROFILE.active, false);
  assert.equal(validateTrustReceiverObservation(observation()).postgres_version, '17.11');
});

test('receiver profile rejects version drift, missing extensions and extra fields', () => {
  for (const mutate of [
    value => { value.postgres_version = '17.12'; },
    value => { delete value.extensions.vector; },
    value => { value.extensions.vector = '0.8.5'; },
    value => { value.extensions.extra = '1'; },
    value => { value.unreviewed = true; },
  ]) {
    const value = observation(); mutate(value);
    assert.throws(() => validateTrustReceiverObservation(value), /pinned CA profile/);
  }
});

test('pre-upgrade CA PostgreSQL 15 observation remains separately recorded', () => {
  assert.equal(validateCaObservedPostgres15({
    format: TRUST_CA_OBSERVED_PROFILE.format, postgres_version: '15.18', client_version: '15.18',
    extensions: { vector: null, pgcrypto: '1.3', pg_trgm: '1.6' },
  }).extensions.vector, null);
});
