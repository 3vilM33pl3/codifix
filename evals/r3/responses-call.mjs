// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted preparation for the existing model adapters. Input counting MUST be
// local and backed by a reviewed/pinned tokenizer for the chosen model. Hashes
// identify the operator's contract; they do not prove tokenizer correctness.
import { prepareResponsesRequest } from './responses-request.mjs';

export function createResponsesPreparation({ mediator, model, tokenizer, countInputTokens, send }) {
  const locked = structuredClone(model), tokenContract = structuredClone(tokenizer);
  if (typeof countInputTokens !== 'function' || typeof send !== 'function' || typeof mediator?.price !== 'function'
      || !locked || ['max_input_tokens', 'max_output_tokens', 'input_microusd_per_million', 'output_microusd_per_million']
        .some(key => !Number.isSafeInteger(locked[key]) || locked[key] < 0)
      || (locked.cached_input_microusd_per_million != null
        && (!Number.isSafeInteger(locked.cached_input_microusd_per_million) || locked.cached_input_microusd_per_million < 0))
      || !tokenContract || Object.keys(tokenContract).sort().join(',') !== 'id,sha256'
      || typeof tokenContract.id !== 'string' || typeof tokenContract.sha256 !== 'string'
      || !/^[A-Za-z0-9._-]{1,128}$/.test(tokenContract.id) || !/^[a-f0-9]{64}$/.test(tokenContract.sha256)) {
    throw Error('explicit private tokenizer and pricing contract required');
  }
  return async payload => {
    if (payload?.model?.provider !== locked.provider || payload.model.resolved_id !== locked.resolved_id
        || payload.model.reasoning !== locked.reasoning) throw Error('model differs from locked provider profile');
    const codec = prepareResponsesRequest(payload, { maxInputTokens: locked.max_input_tokens, maxOutputTokens: locked.max_output_tokens });
    // Count the exact request including instructions, schema and framing, not
    // just the question. No network preflight is permitted by this interface.
    const inputTokens = countInputTokens(structuredClone(codec.body));
    if (!Number.isSafeInteger(inputTokens) || inputTokens < 1 || inputTokens > locked.max_input_tokens) {
      throw Error('unverified or excessive input token count');
    }
    const allowance = await mediator.price({ model: locked, inputTokens, outputTokens: locked.max_output_tokens });
    const contract = { version: 1, protocol: 'r3-responses-call-v1', model: locked,
      tokenizer: tokenContract, input_tokens: inputTokens, request: codec.body };
    const bytes = JSON.stringify(contract);
    return { payload: JSON.parse(bytes), allowance,
      invoke: async (received, signal) => {
        if (JSON.stringify(received) !== bytes) throw Error('changed prepared request');
        const receipt = codec.decode(await send(received.request, signal));
        // Reject a tokenizer undercount before claiming a confirmed bill. The
        // outer mediator retains the complete uncertain reservation on failure.
        if (receipt.tokenUsage.input_tokens > inputTokens) throw Error('input token count exceeded quote');
        const usage = await mediator.price({ model: locked, inputTokens: receipt.tokenUsage.input_tokens,
          outputTokens: receipt.tokenUsage.output_tokens, cachedInputTokens: receipt.cachedInputTokens });
        return { output: { version: 1, answer: receipt.output, provider: { response_id: receipt.responseId, model: receipt.model,
          ...receipt.tokenUsage, cached_input_tokens: receipt.cachedInputTokens, reasoning_output_tokens: receipt.reasoningOutputTokens } }, usage };
      },
      decodeStored: stored => {
        if (!stored || Object.keys(stored).sort().join(',') !== 'answer,provider,version' || stored.version !== 1
            || stored.provider?.model !== locked.resolved_id) throw Error('invalid saved provider result');
        return structuredClone(stored.answer);
      },
    };
  };
}
