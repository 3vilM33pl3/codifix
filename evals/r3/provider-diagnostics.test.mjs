// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { providerFailure, providerDiagnostic, preserveProviderFailure } from './provider-diagnostics.mjs';

test('only closed-vocabulary metadata escapes; forged diagnostics and raw errors do not', () => {
  const secret = 'PRIVATE_INPUT_KEY_OUTPUT';
  const e = providerFailure('http', { http_status: 400, request_id: 'req_12345678',
    api_code: 'invalid_json_schema', parameter: 'text.format.schema', message: secret, stack: secret });
  assert.deepEqual(providerDiagnostic(e), { code: 'http', http_status: 400,
    request_id: 'req_12345678', api_code: 'invalid_json_schema', parameter: 'text.format.schema' });
  assert.equal(preserveProviderFailure(e), e);
  assert.deepEqual(providerDiagnostic({ code: 'http', message: secret }), { code: 'unknown' });
  assert.deepEqual(providerDiagnostic(providerFailure(secret, { http_status: secret,
    request_id: secret, response_id: secret, api_code: secret, parameter: secret, reason: secret,
    status: secret, output_reason: secret })), { code: 'unknown' });
  assert.deepEqual(providerDiagnostic(providerFailure('receipt_output', { output_reason: secret })),
    { code: 'receipt_output' });
  assert.deepEqual(providerDiagnostic(providerFailure('http', { output_reason: 'refusal' })),
    { code: 'http' });
  assert(!JSON.stringify(providerDiagnostic(preserveProviderFailure(Error(secret)))).includes(secret));
});

test('output shape metadata is bounded, content-free and defensively copied', () => {
  const secret = 'PRIVATE_OUTPUT_SECRET';
  const raw = [{ type: 'message', phase: secret, status: secret, role: secret,
    content: [{ type: 'output_text', text: secret, annotations: [{ text: secret }] },
      { type: 'refusal', refusal: secret }, { type: secret, text: secret }], extra: secret }];
  const error = providerFailure('receipt_output', { output_items: raw, output_shape: secret });
  const d = providerDiagnostic(error);
  assert.equal(d.output_shape[0].phase, 'other');
  assert.deepEqual(d.output_shape[0].content.map(p => p.type), ['output_text', 'refusal', 'other']);
  assert.equal(d.output_shape[0].content[0].text_bytes, Buffer.byteLength(secret));
  assert(!JSON.stringify(d).includes(secret));
  raw[0].type = secret; d.output_shape[0].type = secret;
  d.output_shape[0].content[0].text_bytes = secret;
  assert(!JSON.stringify(providerDiagnostic(error)).includes(secret));
  assert.equal(providerDiagnostic(providerFailure('http', { output_items: raw })).output_shape, undefined);
  assert.equal(providerDiagnostic(providerFailure('receipt_output', { output_items: Array(17).fill(raw[0]) })).output_shape, undefined);
  const large = providerDiagnostic(providerFailure('receipt_output', {
    output_items: [{ content: Array(2000).fill({ type: secret, text: secret }) }],
  })).output_shape[0];
  assert.equal(large.content.length, 16); assert.equal(large.content_count, 1024);
  assert.equal(large.content_truncated, true);
});
