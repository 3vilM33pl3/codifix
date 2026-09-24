// SPDX-License-Identifier: AGPL-3.0-or-later
// Exact inactive receiver profile derived from the CA agent's read-only report.
export const TRUST_RECEIVER_PROFILE = Object.freeze({
  format: 'memory-trust-receiver-profile-v1',
  postgres_major: 15,
  postgres_version: '15.18',
  client_version: '15.18',
  extensions: Object.freeze({ vector: '0.8.6', pgcrypto: '1.3', pg_trgm: '1.6' }),
  database: 'memory_trust',
  role: 'memory_trust',
  service: 'memory-layer@trust.service',
  port: 8091,
  active: false,
});

export const TRUST_RECEIVER_FIXTURE = Object.freeze({
  image_id: 'sha256:e2426fe23642a1e4ddeb6126bb7e4295cfd0b42d35dbb10233d6d31c00e8c9b7',
  postgres_image: 'docker.io/library/postgres@sha256:752791ad6ccfc6fbfcf4d9b43413c8642901f1c2008ebdc1423ea32fd66fd4f1',
  pgvector_image: 'docker.io/pgvector/pgvector@sha256:a6579dded3ff4ed3d9565ae54ab253e2cb7ae24c5d1eb3bec8a89e73f038b68a',
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
