// SPDX-License-Identifier: AGPL-3.0-or-later
// Conservative capacity reservations, NOT a tokenizer or a paid-run approval.
// Reviewed 2026-09-16 against official model/embedding documentation. Adding a
// model requires reviewing its context, input limits, modalities and price tiers.
// https://developers.openai.com/api/docs/models/gpt-5.4-mini
// https://developers.openai.com/api/docs/guides/embeddings
export function validContextReservation(model, profile) {
  return profile.version === 2 && profile.input_accounting === 'full_context_reservation'
    && model.resolved_id === 'gpt-5.4-mini-2026-03-17' && model.max_input_tokens === 400000
    && model.max_output_tokens >= 1 && model.max_output_tokens <= 128000
    && model.input_microusd_per_million === 750000 && model.output_microusd_per_million === 4500000
    && model.cached_input_microusd_per_million === 75000 && profile.embedding.input_microusd_per_million === 20000
    && ['none', 'low', 'medium', 'high', 'xhigh'].includes(model.reasoning)
    && profile.embedding.resolved_id === 'text-embedding-3-small'
    && profile.embedding.max_input_tokens === 8192 && profile.embedding.dimensions === 1536
    && profile.responses_tokenizer.id === 'full-context-responses-v1'
    && profile.embedding.tokenizer.id === 'full-context-embeddings-v1';
}

export function createContextReservations(model, profile) {
  const locked = structuredClone({ model, profile });
  if (!validContextReservation(locked.model, locked.profile)) throw Error('unreviewed full-context reservation profile');
  const counter = (pin, countInputTokens) => ({ id: pin.id, sha256: pin.asset.sha256, countInputTokens });
  return {
    responsesCounter: counter(locked.profile.responses_tokenizer, body => {
      if (body?.model !== locked.model.resolved_id || body.max_output_tokens !== locked.model.max_output_tokens
          || body.truncation !== 'disabled' || !Array.isArray(body.tools) || body.tools.length
          || Buffer.byteLength(JSON.stringify(body)) > 131072) throw Error('invalid context reservation input');
      // Includes the entire documented context, so unknown message/schema
      // framing cannot make an accepted request exceed this reservation.
      // Output is reserved separately as an additional conservative allowance.
      return 400000;
    }),
    embeddingCounter: counter(locked.profile.embedding.tokenizer, body => {
      if (body?.model !== locked.profile.embedding.resolved_id || body.dimensions !== 1536
          || !Array.isArray(body.input) || body.input.length !== 1 || typeof body.input[0] !== 'string'
          || Buffer.byteLength(body.input[0]) > 8192) throw Error('invalid embedding reservation input');
      return 8192;
    }),
  };
}
