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
    // The current Responses schema adds cache_write_tokens. Zero introduces no
    // unpriced usage; nonzero writes need a separately approved pricing profile.
    // Keep rejecting all other new billing fields and preserve old receipts.
    const inputDetails = usage.input_tokens_details;
    const inputShape = keys(inputDetails, 'cached_tokens')
      || (keys(inputDetails, 'cache_write_tokens,cached_tokens') && inputDetails.cache_write_tokens === 0);
    if (!keys(usage, 'input_tokens,input_tokens_details,output_tokens,output_tokens_details,total_tokens')
        || !inputShape
        || !keys(usage.output_tokens_details, 'reasoning_tokens')) fail();
    stage = 'receipt_output';
    metadata.output_items = response.output;
    const failOutput = reason => { metadata.output_reason = reason; fail(); };
    let text, sawCommentary = false, commentaryBytes = 0;
    for (const item of response.output) {
      if (item?.type === 'reasoning') {
        // Reasoning tokens are already included in output_tokens, never added
        // again. Do not return reasoning summaries to the agent or scorer.
        if (item.status != null && item.status !== 'completed') failOutput('reasoning_status');
        continue;
      }
      if (item?.type !== 'message') failOutput('unexpected_item');
      if (item.role !== 'assistant' || item.status !== 'completed') failOutput('message_shape');
      // Responses may include intermediate assistant updates before the answer.
      // Never parse a preamble as an action, concatenate messages, or guess that
      // the last unlabelled message is final. Legacy single-message receipts
      // remain supported. See the GPT-5.4 guidance's assistant phase contract.
      if (item.phase != null && !['commentary', 'final_answer'].includes(item.phase)) failOutput('unknown_phase');
      if (text !== undefined && item.phase === 'commentary') failOutput('phase_order');
      if (text !== undefined) failOutput('multiple_messages');
      if (sawCommentary && item.phase == null) failOutput('ambiguous_phase');
      if (!Array.isArray(item.content) || item.content.length !== 1) failOutput('content_shape');
      const part = item.content[0];
      if (part?.type === 'refusal') failOutput('refusal');
      if (part?.type !== 'output_text' || typeof part.text !== 'string') failOutput('text_shape');
      if (Buffer.byteLength(part.text) > 65536) failOutput('text_oversized');
      if (!Array.isArray(part.annotations) || part.annotations.length) failOutput('annotations');
      if (item.phase === 'commentary') {
        sawCommentary = true;
        commentaryBytes += Buffer.byteLength(part.text);
        if (commentaryBytes > 65536) failOutput('text_oversized');
        continue;
      }
      text = part.text;
    }
    if (text === undefined) failOutput('missing_text');
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
