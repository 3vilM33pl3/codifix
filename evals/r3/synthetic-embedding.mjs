// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared offline fixture, not a semantic model. Binary fractions are exactly
// representable in f32, so the neutral baseline and PostgreSQL store agree.
import { createHash } from 'node:crypto';
export const syntheticEmbeddingModel = 'sha256-byte-fixture-r1';
export function syntheticVector(text) {
  if (typeof text !== 'string' || !text.length) throw Error('invalid synthetic embedding input');
  return [...createHash('sha256').update(text).digest().subarray(0, 16)].map(byte => (byte + 1) / 256);
}
export function syntheticServiceEmbedding() {
  return { model: syntheticEmbeddingModel, dimensions: 16, embed: async (text, purpose) => {
    // Memory keeps its existing splitter/summary prefix; the simple baseline
    // keeps fixed whole-source chunks. Same embedding function, not identical
    // ranking inputs or a claim of semantic quality.
    if (!['document', 'query'].includes(purpose)) throw Error('invalid embedding purpose');
    return syntheticVector(text);
  } };
}
