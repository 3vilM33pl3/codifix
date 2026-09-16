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
    request_id: secret, response_id: secret, api_code: secret, parameter: secret, reason: secret, status: secret })), { code: 'unknown' });
  assert(!JSON.stringify(providerDiagnostic(preserveProviderFailure(Error(secret)))).includes(secret));
});
