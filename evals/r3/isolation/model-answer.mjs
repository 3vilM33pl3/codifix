// SPDX-License-Identifier: AGPL-3.0-or-later
// Common tool-free answer path for all five conditions. The caller has already
// retrieved approved context; rubric/condition/source-pool metadata stay local.
import { runOfflineKnowledge } from './knowledge.mjs';
import { dispatchModel } from './model-dispatch.mjs';

export async function runMediatedKnowledge(options) {
  const { mediator, invoke, prepareRequest, ...plain } = options;
  const { model, taskId, requestId, allowance, timeoutMs = 10000, ...knowledge } = structuredClone(plain);
  if (!mediator || typeof mediator.dispatch !== 'function'
      || (typeof invoke === 'function') === (typeof prepareRequest === 'function')
      || !model || Object.keys(model).sort().join(',') !== 'provider,reasoning,resolved_id'
      || Object.values(model).some(v => typeof v !== 'string' || !/^[A-Za-z0-9._-]{1,128}$/.test(v))) throw Error('invalid common answer model');
  return runOfflineKnowledge({ ...knowledge, runAnswer: async input => {
    const receipt = await dispatchModel({ mediator, prepareRequest, taskId, requestId, allowance, timeoutMs, invoke, withReceipt: true,
      payload: { model, input, response_schema: 'r3-grounded-answer-v1', tools: [] } });
    // Reconstruct the task's original cost on recovery, not a fictitious zero.
    // newProviderRequests distinguishes a cached receipt from a new invocation;
    // the campaign must deduplicate task identities, never sum retried tasks.
    return { answer: receipt.output, providerRequests: 1, newProviderRequests: receipt.invoked ? 1 : 0,
      executionMicrousd: receipt.usage.microusd,
      tokenUsage: { input_tokens: receipt.usage.input_tokens, output_tokens: receipt.usage.output_tokens } };
  } });
}
