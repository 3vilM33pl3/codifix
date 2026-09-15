// SPDX-License-Identifier: AGPL-3.0-or-later
// Common private prepared-call boundary. The controller verifies asset bytes
// and supplies reviewed local counters/transports; this module neither loads
// executable code nor discovers credentials nor grants spending authority.
import { createResponsesPreparation } from './responses-call.mjs';
import { createEmbeddingsPreparation } from './embeddings-call.mjs';
import { dispatchModel } from './isolation/model-dispatch.mjs';

const exact = (object, keys) => object && typeof object === 'object' && !Array.isArray(object)
  && Object.keys(object).sort().join(',') === keys.split(',').sort().join(',');
const identifier = value => typeof value === 'string' && /^[A-Za-z0-9._-]{1,128}$/.test(value);
const integer = value => Number.isSafeInteger(value) && value >= 0;

export function validateProviderProfile(manifest) {
  const profile = structuredClone(manifest.provider_profile), model = structuredClone(manifest.model);
  const pin = value => exact(value, 'path,sha256') && typeof value.path === 'string' && !value.path.includes('\\')
    && value.path.split('/').every(part => part && part !== '.' && part !== '..')
    && /^[a-f0-9]{64}$/.test(value.sha256) && manifest.assets?.some(asset => asset.path === value.path
      && asset.sha256 === value.sha256 && asset.kind === 'configuration');
  const tokenizer = value => exact(value, 'id,asset') && identifier(value.id) && pin(value.asset);
  if (!exact(profile, 'version,protocol,responses_tokenizer,embedding,configuration_evidence')
      || profile.version !== 1 || profile.protocol !== 'openai-text-v1' || model?.provider !== 'openai'
      || !identifier(model.resolved_id) || !identifier(model.embedding_id)
      || !['none','minimal','low','medium','high','xhigh','max','ultra'].includes(model.reasoning)
      || !tokenizer(profile.responses_tokenizer) || !pin(profile.configuration_evidence)
      || !exact(profile.embedding, 'resolved_id,dimensions,max_input_tokens,input_microusd_per_million,tokenizer')
      || profile.embedding.resolved_id !== model.embedding_id || !tokenizer(profile.embedding.tokenizer)
      || !Number.isInteger(profile.embedding.dimensions) || profile.embedding.dimensions < 1 || profile.embedding.dimensions > 8192
      || !Number.isInteger(profile.embedding.max_input_tokens) || profile.embedding.max_input_tokens < 1 || profile.embedding.max_input_tokens > 8192
      || ![model.max_input_tokens, model.max_output_tokens, model.input_microusd_per_million,
        model.output_microusd_per_million, model.cached_input_microusd_per_million ?? 0,
        profile.embedding.input_microusd_per_million, model.max_input_tokens + model.max_output_tokens].every(integer)
      || model.max_input_tokens < 1 || model.max_output_tokens < 1
      || (model.cached_input_microusd_per_million ?? 0) > model.input_microusd_per_million) throw Error('invalid locked provider profile');
  return { profile, model };
}

export function createProviderBindings({ manifest, mediator, taskId, responsesCounter, embeddingCounter,
  sendResponses, sendEmbeddings, signal }) {
  const { profile, model } = validateProviderProfile(manifest);
  if (typeof taskId !== 'string' || !/^(preparation|work-[0-9]{5})$/.test(taskId)
      || typeof sendResponses !== 'function' || typeof sendEmbeddings !== 'function'
      || (signal !== undefined && !(signal instanceof AbortSignal))) throw Error('invalid private provider binding');
  const counter = (received, expected) => {
    if (!exact(received, 'id,sha256,countInputTokens') || received.id !== expected.id
        || received.sha256 !== expected.asset.sha256 || typeof received.countInputTokens !== 'function') {
      throw Error('counter does not match locked provider profile');
    }
    return { tokenizer: { id: received.id, sha256: received.sha256 }, countInputTokens: received.countInputTokens };
  };
  const merge = current => signal ? AbortSignal.any([current, signal]) : current;
  const responsePreparation = createResponsesPreparation({ mediator, model,
    ...counter(responsesCounter, profile.responses_tokenizer), send: (body, current) => sendResponses(body, merge(current)) });
  const embeddingModel = { provider: model.provider, resolved_id: profile.embedding.resolved_id,
    reasoning: 'none', embedding_id: profile.embedding.resolved_id,
    max_input_tokens: profile.embedding.max_input_tokens, max_output_tokens: 1,
    input_microusd_per_million: profile.embedding.input_microusd_per_million, output_microusd_per_million: 0 };
  const embeddingPreparation = createEmbeddingsPreparation({ mediator, model: embeddingModel,
    dimensions: profile.embedding.dimensions, ...counter(embeddingCounter, profile.embedding.tokenizer),
    send: (body, current) => sendEmbeddings(body, merge(current)) });
  let embeddingCalls = 0;
  return {
    // For shared vector provenance; never claim normalization not performed.
    embeddingProfile: { provider: model.provider, model: profile.embedding.resolved_id,
      dimensions: profile.embedding.dimensions, normalization: 'float32-rounding-v1',
      document_purpose: 'document', query_purpose: 'query', metric: 'cosine' },
    async prepareRequest(payload) {
      if (taskId === 'preparation' || signal?.aborted) throw Error('response invocation not permitted in this phase');
      return responsePreparation(payload);
    },
    async embed(input, purpose, current) {
      if (purpose !== (taskId === 'preparation' ? 'document' : 'query')) throw Error('embedding phase mismatch');
      const requestId = `${purpose === 'document' ? 'document' : 'query-embedding'}-${embeddingCalls++}`;
      return (await dispatchModel({ mediator, prepareRequest: embeddingPreparation, taskId, requestId,
        payload: { input, purpose }, timeoutMs: 10000, signal: current ? merge(current) : signal })).output;
    },
  };
}
