// SPDX-License-Identifier: AGPL-3.0-or-later
// Exact inactive receiver target selected after the CA agent's read-only report.
export const TRUST_RECEIVER_PROFILE = Object.freeze({
  format: 'memory-trust-receiver-profile-v1',
  postgres_major: 17,
  postgres_version: '17.11',
  client_version: '17.11',
  extensions: Object.freeze({ vector: '0.8.6', pgcrypto: '1.3', pg_trgm: '1.6' }),
  database: 'memory_trust',
  database_port: 5433,
  role: 'memory_trust',
  service: 'memory-layer@trust.service',
  port: 8091,
  active: false,
});

export const TRUST_RECEIVER_FIXTURE = Object.freeze({
  image_id: 'sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180',
  pgvector_image: 'docker.io/pgvector/pgvector@sha256:cf134a767f474095eeba57e0117be8e568e011a63f33fbf252f14c9b760f8e6f',
});

export const TRUST_CA_OBSERVED_PROFILE = Object.freeze({
  format: 'memory-trust-ca-observed-profile-v1', postgres_version: '15.18', client_version: '15.18',
  extensions: Object.freeze({ vector: null, pgcrypto: '1.3', pg_trgm: '1.6' }),
});

const exactKeys = (value, keys) => value && typeof value === 'object'
  && !Array.isArray(value) && Object.keys(value).sort().join('\0') === [...keys].sort().join('\0');

/** Reject near matches so a future PostgreSQL minor is deliberately re-rehearsed. */
export function validateTrustReceiverObservation(value) {
  if (!exactKeys(value, ['format', 'postgres_version', 'client_version', 'extensions'])
      || value.format !== 'memory-trust-receiver-observation-v1'
      || value.postgres_version !== TRUST_RECEIVER_PROFILE.postgres_version
      || value.client_version !== TRUST_RECEIVER_PROFILE.client_version
      || !exactKeys(value.extensions, ['vector', 'pgcrypto', 'pg_trgm'])) {
    throw new Error('Trust receiver compatibility observation does not match the pinned CA profile');
  }
  for (const [name, version] of Object.entries(TRUST_RECEIVER_PROFILE.extensions)) {
    if (value.extensions[name] !== version) {
      throw new Error('Trust receiver extension profile does not match the pinned CA profile');
    }
  }
  return Object.freeze(structuredClone(value));
}

export function validateCaObservedPostgres15(value) {
  if (!exactKeys(value, ['format', 'postgres_version', 'client_version', 'extensions'])
      || value.format !== TRUST_CA_OBSERVED_PROFILE.format
      || value.postgres_version !== TRUST_CA_OBSERVED_PROFILE.postgres_version
      || value.client_version !== TRUST_CA_OBSERVED_PROFILE.client_version
      || !exactKeys(value.extensions, ['vector', 'pgcrypto', 'pg_trgm'])) {
    throw new Error('CA PostgreSQL observation does not match the recorded pre-upgrade profile');
  }
  for (const [name, version] of Object.entries(TRUST_CA_OBSERVED_PROFILE.extensions)) {
    if (value.extensions[name] !== version) throw new Error('CA PostgreSQL observation does not match the recorded pre-upgrade profile');
  }
  return Object.freeze(structuredClone(value));
}
