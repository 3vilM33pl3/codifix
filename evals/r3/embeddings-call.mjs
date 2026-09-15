// SPDX-License-Identifier: AGPL-3.0-or-later
// One text per request, explicit model/dimensions and float32 storage. The
// caller owns approval, local tokenizer verification and preparation/task caps.
// Wire reference: https://developers.openai.com/api/reference/ruby/resources/embeddings/methods/create
import { checkedVector } from './isolation/embedding-proxy.mjs';

const exact = (value, fields) => value && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).sort().join(',') === fields.split(',').sort().join(',');

export function prepareEmbeddingsRequest({ model, dimensions, input, maxInputTokens }) {
  if (typeof model !== 'string' || !/^[A-Za-z0-9._-]{1,128}$/.test(model)
      || !Number.isInteger(dimensions) || dimensions < 1 || dimensions > 8192
      || !Number.isInteger(maxInputTokens) || maxInputTokens < 1 || maxInputTokens > 8192
      || typeof input !== 'string' || !input.trim() || Buffer.byteLength(input) > 32768) throw Error('invalid embedding request');
  const body = { model, dimensions, input: [input], encoding_format: 'float' };
  return { body, decode: bytes => {
    try {
      if (typeof bytes !== 'string' || Buffer.byteLength(bytes) > 1024 * 1024) throw Error('oversized');
      const receipt = JSON.parse(bytes);
      if (!exact(receipt, 'data,model,object,usage') || receipt.model !== model || receipt.object !== 'list'
          || !Array.isArray(receipt.data) || receipt.data.length !== 1
          || !exact(receipt.data[0], 'embedding,index,object') || receipt.data[0].index !== 0
          || receipt.data[0].object !== 'embedding' || !exact(receipt.usage, 'prompt_tokens,total_tokens')
          || !Number.isSafeInteger(receipt.usage.prompt_tokens) || receipt.usage.prompt_tokens < 1
          || receipt.usage.prompt_tokens > maxInputTokens || receipt.usage.total_tokens !== receipt.usage.prompt_tokens) throw Error('invalid receipt');
      return { vector: checkedVector(receipt.data[0].embedding, dimensions), inputTokens: receipt.usage.prompt_tokens };
    } catch { throw Error('invalid embedding receipt; retain uncertain attempt'); }
  } };
}

export function createEmbeddingsPreparation({ mediator, model, dimensions, tokenizer, countInputTokens, send }) {
  const locked = structuredClone(model), tokenContract = structuredClone(tokenizer);
  if (locked?.provider !== 'openai' || typeof mediator?.price !== 'function' || typeof send !== 'function'
      || typeof countInputTokens !== 'function' || !exact(tokenContract, 'id,sha256')
      || typeof tokenContract.id !== 'string' || !/^[A-Za-z0-9._-]{1,128}$/.test(tokenContract.id)
      || typeof tokenContract.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(tokenContract.sha256)
      || !Number.isSafeInteger(locked.input_microusd_per_million) || locked.input_microusd_per_million < 0) throw Error('explicit embedding model, tokenizer and pricing required');
  // Validate the immutable request profile before any provider activity.
  prepareEmbeddingsRequest({ model: locked.resolved_id, dimensions, input: 'profile validation', maxInputTokens: locked.max_input_tokens });
  return async payload => {
    if (!exact(payload, 'input,purpose') || !['document', 'query'].includes(payload.purpose)) throw Error('invalid embedding purpose or input');
    const codec = prepareEmbeddingsRequest({ model: locked.resolved_id, dimensions, input: payload.input, maxInputTokens: locked.max_input_tokens });
    let inputTokens;
    try {
      inputTokens = countInputTokens(structuredClone(codec.body));
      if (inputTokens instanceof Promise) { inputTokens.catch(() => {}); throw Error('async counter'); }
    } catch { throw Error('local embedding token counter failed'); }
    if (!Number.isSafeInteger(inputTokens) || inputTokens < 1 || inputTokens > locked.max_input_tokens) throw Error('unverified or excessive embedding token count');
    const allowance = await mediator.price({ model: locked, inputTokens, outputTokens: 0 });
    const contract = { version: 1, protocol: 'r3-embeddings-call-v1', model: locked, dimensions,
      tokenizer: tokenContract, purpose: payload.purpose, input_tokens: inputTokens, request: codec.body };
    const bytes = JSON.stringify(contract);
    return { payload: JSON.parse(bytes), allowance,
      invoke: async (received, signal) => {
        if (JSON.stringify(received) !== bytes) throw Error('changed prepared embedding request');
        const receipt = codec.decode(await send(received.request, signal));
        if (receipt.inputTokens > inputTokens) throw Error('embedding token count exceeded quote');
        const usage = await mediator.price({ model: locked, inputTokens: receipt.inputTokens, outputTokens: 0 });
        return { output: { version: 1, vector: receipt.vector, provider: { model: locked.resolved_id,
          input_tokens: receipt.inputTokens, output_tokens: 0 } }, usage };
      },
      decodeStored: saved => {
        if (!exact(saved, 'version,vector,provider') || saved.version !== 1
            || !exact(saved.provider, 'model,input_tokens,output_tokens') || saved.provider.model !== locked.resolved_id
            || saved.provider.output_tokens !== 0 || !Number.isSafeInteger(saved.provider.input_tokens)
            || saved.provider.input_tokens < 1 || saved.provider.input_tokens > inputTokens) throw Error('invalid saved embedding result');
        return checkedVector(saved.vector, dimensions);
      },
    };
  };
}
