// SPDX-License-Identifier: AGPL-3.0-or-later
// Closed vocabulary only. Never serialize an exception, provider body or prompt.
const diagnostics = new WeakMap();
const codes = new Set(['transport', 'cancelled', 'http', 'content_type', 'oversized',
  'encoding', 'receipt_contract', 'receipt_json', 'receipt_status', 'receipt_model',
  'receipt_identity', 'receipt_usage', 'receipt_usage_shape', 'receipt_output',
  'receipt_output_json', 'structured_output', 'approval', 'deadline', 'unknown']);
const apiCodes = new Set(['invalid_api_key', 'insufficient_quota', 'rate_limit_exceeded',
  'model_not_found', 'invalid_json_schema', 'invalid_request_error', 'unsupported_parameter',
  'unsupported_value', 'context_length_exceeded', 'server_error']);
const params = new Set(['model', 'reasoning.effort', 'max_output_tokens', 'tools', 'tool_choice',
  'parallel_tool_calls', 'store', 'background', 'stream', 'truncation', 'service_tier',
  'text.format', 'text.format.schema', 'input']);
const outputReasons = new Set(['reasoning_status', 'unexpected_item', 'message_shape',
  'multiple_messages', 'content_shape', 'refusal', 'text_shape', 'text_oversized',
  'annotations', 'missing_text', 'unknown_phase', 'phase_order', 'ambiguous_phase']);
const category = (value, allowed) => value === undefined ? 'absent'
  : value === null ? 'null' : allowed.includes(value) ? value : 'other';
// Project only a bounded structural description, never raw strings, IDs,
// annotations, reasoning, refusal text or extra provider fields. The shape is
// diagnostic evidence, not an answer selector or permission to settle usage.
function outputShape(items) {
  if (!Array.isArray(items) || items.length > 16) return undefined;
  return items.map(item => {
    const content = Array.isArray(item?.content) ? item.content : null;
    return {
      type: category(item?.type, ['message', 'reasoning']),
      phase: category(item?.phase, ['commentary', 'final_answer']),
      status: category(item?.status, ['completed', 'incomplete', 'in_progress']),
      role: category(item?.role, ['assistant', 'user', 'system', 'developer']),
      content_count: content ? Math.min(content.length, 1024) : null,
      content_truncated: content !== null && content.length > 16,
      content: content ? content.slice(0, 16).map(part => ({
        type: category(part?.type, ['output_text', 'refusal']),
        text_bytes: typeof part?.text === 'string' ? Math.min(Buffer.byteLength(part.text), 1048576) : null,
      })) : [],
    };
  });
}
export function providerFailure(code, metadata = {}) {
  const safe = { code: codes.has(code) ? code : 'unknown' };
  if (Number.isInteger(metadata.http_status) && metadata.http_status >= 100 && metadata.http_status <= 599)
    safe.http_status = metadata.http_status;
  if (/^req_[a-f0-9-]{8,80}$/.test(metadata.request_id ?? '')) safe.request_id = metadata.request_id;
  if (/^resp_[a-f0-9]{8,240}$/.test(metadata.response_id ?? '')) safe.response_id = metadata.response_id;
  if (apiCodes.has(metadata.api_code)) safe.api_code = metadata.api_code;
  if (params.has(metadata.parameter)) safe.parameter = metadata.parameter;
  if (['completed', 'incomplete', 'failed', 'cancelled', 'in_progress', 'queued'].includes(metadata.status)) safe.status = metadata.status;
  if (['max_output_tokens', 'content_filter'].includes(metadata.reason)) safe.reason = metadata.reason;
  if (safe.code === 'receipt_output' && outputReasons.has(metadata.output_reason))
    safe.output_reason = metadata.output_reason;
  if (safe.code === 'receipt_output') {
    const shape = outputShape(metadata.output_items);
    if (shape) safe.output_shape = shape;
  }
  const error = Error('invalid or incomplete provider receipt or transport; retain uncertain attempt');
  diagnostics.set(error, Object.freeze(safe));
  return error;
}
export function providerDiagnostic(error) {
  return structuredClone(diagnostics.get(error) ?? { code: 'unknown' });
}
export function preserveProviderFailure(error, fallback = 'unknown') {
  return diagnostics.has(error) ? error : providerFailure(fallback);
}
