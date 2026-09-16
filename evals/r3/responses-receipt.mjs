// SPDX-License-Identifier: AGPL-3.0-or-later
// Pure Responses API receipt decoder. No credentials, network, pricing or
// approval authority. A trusted transport must bound bytes before calling this.
// Reference: https://developers.openai.com/api/reference/typescript/resources/responses/methods/create
const integer = value => Number.isSafeInteger(value) && value >= 0;
import { providerFailure, preserveProviderFailure } from './provider-diagnostics.mjs';

export function decodeResponsesReceipt(bytes, contract) {
  let stage = 'receipt_contract', metadata = {};
  const fail = () => { throw providerFailure(stage, metadata); };
  // Do not expose provider error bodies, refusal text, reasoning or raw output in
  // exceptions. Caller policy decides whether private payload retention is allowed.
  try {
    if (typeof bytes !== 'string' || Buffer.byteLength(bytes) > 1024 * 1024
        || !contract || Object.keys(contract).sort().join(',') !== 'maxInputTokens,maxOutputTokens,model'
        || typeof contract.model !== 'string' || !/^[A-Za-z0-9._-]{1,128}$/.test(contract.model)
        || !integer(contract.maxInputTokens) || !contract.maxInputTokens
        || !integer(contract.maxOutputTokens) || !contract.maxOutputTokens) fail();
    stage = 'receipt_json';
    const response = JSON.parse(bytes);
    metadata = { response_id: response?.id, status: response?.status, reason: response?.incomplete_details?.reason };
    stage = 'receipt_status';
    if (response?.status !== 'completed') fail();
    stage = 'receipt_model';
    if (response?.model !== contract.model) fail();
    stage = 'receipt_identity';
    if (!response || response.object !== 'response' || response.status !== 'completed'
        || response.model !== contract.model || response.error !== null
        || response.incomplete_details !== null || typeof response.id !== 'string'
        || !/^resp_[A-Za-z0-9_-]{1,240}$/.test(response.id)
        || !Array.isArray(response.output) || !response.output.length || response.output.length > 16) fail();
    stage = 'receipt_usage';
    const usage = response.usage;
    if (!usage || ![usage.input_tokens, usage.output_tokens, usage.total_tokens].every(integer)
        || !Number.isSafeInteger(usage.input_tokens + usage.output_tokens)
        || usage.total_tokens !== usage.input_tokens + usage.output_tokens
        || usage.input_tokens > contract.maxInputTokens || usage.output_tokens > contract.maxOutputTokens
        || !integer(usage.input_tokens_details?.cached_tokens)
        || usage.input_tokens_details.cached_tokens > usage.input_tokens
        || !integer(usage.output_tokens_details?.reasoning_tokens)
        || usage.output_tokens_details.reasoning_tokens > usage.output_tokens) fail();
    // Tool-free text profile only: no hosted tool fees, multimodal usage or new
    // billing categories can silently disappear from the receipt.
    stage = 'receipt_usage_shape';
    const keys = (value, expected) => Object.keys(value).sort().join(',') === expected;
    if (!keys(usage, 'input_tokens,input_tokens_details,output_tokens,output_tokens_details,total_tokens')
        || !keys(usage.input_tokens_details, 'cached_tokens')
        || !keys(usage.output_tokens_details, 'reasoning_tokens')) fail();
    stage = 'receipt_output';
    let text;
    for (const item of response.output) {
      if (item?.type === 'reasoning') {
        // Reasoning tokens are already included in output_tokens, never added
        // again. Do not return reasoning summaries to the agent or scorer.
        if (item.status != null && item.status !== 'completed') fail();
        continue;
      }
      if (item?.type !== 'message' || item.role !== 'assistant' || item.status !== 'completed'
          || text !== undefined || !Array.isArray(item.content) || item.content.length !== 1) fail();
      const part = item.content[0];
      if (part?.type !== 'output_text' || typeof part.text !== 'string'
          || Buffer.byteLength(part.text) > 65536
          || !Array.isArray(part.annotations) || part.annotations.length) fail();
      text = part.text;
    }
    if (text === undefined) fail();
    stage = 'receipt_output_json';
    const output = JSON.parse(text);
    if (!output || Array.isArray(output) || typeof output !== 'object') fail();
    return {
      responseId: response.id, model: response.model, output,
      tokenUsage: { input_tokens: usage.input_tokens, output_tokens: usage.output_tokens },
      cachedInputTokens: usage.input_tokens_details.cached_tokens,
      reasoningOutputTokens: usage.output_tokens_details.reasoning_tokens,
    };
  } catch (error) { throw preserveProviderFailure(error, stage); }
}
