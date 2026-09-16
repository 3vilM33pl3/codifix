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
  const error = Error('invalid or incomplete provider receipt or transport; retain uncertain attempt');
  diagnostics.set(error, Object.freeze(safe));
  return error;
}
export function providerDiagnostic(error) {
  return { ...(diagnostics.get(error) ?? { code: 'unknown' }) };
}
export function preserveProviderFailure(error, fallback = 'unknown') {
  return diagnostics.has(error) ? error : providerFailure(fallback);
}
