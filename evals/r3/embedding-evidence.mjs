// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host bindings only. Hashes establish consistency, not provider origin,
// approval, semantic usefulness or permission to send data to a model.
import { createHash } from 'node:crypto';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const fields = ['provider', 'model', 'dimensions', 'normalization', 'document_purpose', 'query_purpose', 'metric'];

function checkedProfile(profile) {
  if (!profile || typeof profile !== 'object' || Array.isArray(profile)
      || Object.keys(profile).length !== fields.length || fields.some(key => !Object.hasOwn(profile, key))
      || !Number.isInteger(profile.dimensions) || profile.dimensions < 1 || profile.dimensions > 8192
      || profile.metric !== 'cosine'
      || fields.filter(key => key !== 'dimensions').some(key => typeof profile[key] !== 'string'
        || !/^[\x21-\x7e]{1,256}$/.test(profile[key]))) throw Error('invalid embedding profile');
  return structuredClone(profile);
}

export function syntheticProfile(dimensions, model) {
  return checkedProfile({ provider: 'synthetic', model, dimensions, normalization: 'identity-r1',
    document_purpose: 'document', query_purpose: 'query', metric: 'cosine' });
}

export function vectorDigest(vector) {
  if (!Array.isArray(vector) || vector.length < 1 || vector.length > 8192
      || vector.some(value => typeof value !== 'number' || !Number.isFinite(value))
      || !vector.some(value => value !== 0)) throw Error('invalid embedding vector');
  const bytes = Buffer.alloc(vector.length * 8);
  vector.forEach((value, index) => bytes.writeDoubleLE(value, index * 8));
  return digest(bytes);
}

export function bindCorpus(profile, documents, vectors) {
  const checked = checkedProfile(profile);
  if (!Array.isArray(documents) || !documents.length || !Array.isArray(vectors)
      || documents.length !== vectors.length || new Set(documents.map(doc => doc.id)).size !== documents.length) throw Error('invalid embedding corpus');
  return { profile: checked, bindings: documents.map((document, index) => {
    const vector = vectors[index];
    if (typeof document.id !== 'string' || !document.id || typeof document.text !== 'string'
        || !Array.isArray(vector) || vector.length !== checked.dimensions) throw Error('invalid embedding document');
    return { document_id: document.id, text_sha256: digest(document.text), vector_sha256: vectorDigest(vector) };
  }) };
}

export function bindQuery(profile, text, vector) {
  const checked = checkedProfile(profile);
  if (typeof text !== 'string' || !Array.isArray(vector) || vector.length !== checked.dimensions) throw Error('invalid embedding query');
  return { profile: checked, text_sha256: digest(text), vector_sha256: vectorDigest(vector) };
}
