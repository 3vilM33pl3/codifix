// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { validateWindowsDispatchEndpoint, createWindowsDispatchRequest, authenticateWindowsDispatchRequest,
  signWindowsDispatchResponse, authenticateWindowsDispatchResponse } from './windows-dispatch-auth.mjs';
const endpoint = { format: 'memory-r3-windows-loopback-v1', host: '127.0.0.1', port: 40001,
  session_id: 'b237d37c-687a-499f-b2fb-c172085ab056', manifest_sha256: 'a'.repeat(64),
  configuration_sha256: 'b'.repeat(64), key: 'c'.repeat(64) };
const expected = { manifestSha256: endpoint.manifest_sha256, configurationSha256: endpoint.configuration_sha256 };
const body = Buffer.from('{"work":"synthetic"}');
test('native endpoint is exact, campaign-bound, and literal loopback only', () => {
  assert.deepEqual(validateWindowsDispatchEndpoint(endpoint, expected), endpoint);
  for (const delta of [{ host: 'localhost' }, { host: '0.0.0.0' }, { port: 0 }, { port: 65536 },
    { manifest_sha256: 'd'.repeat(64) }, { configuration_sha256: 'd'.repeat(64) }, { extra: true }, { key: 'bad' }]) {
    assert.throws(() => validateWindowsDispatchEndpoint({ ...endpoint, ...delta }, expected));
  }
});
test('mutual transcripts bind body, direction, session, status and fresh nonce; key is never sent', () => {
  const request = createWindowsDispatchRequest(endpoint, body);
  assert(!JSON.stringify(request).includes(endpoint.key));
  const proof = authenticateWindowsDispatchRequest(endpoint, request.headers, body);
  assert.deepEqual(proof, request.proof);
  assert.throws(() => authenticateWindowsDispatchRequest(endpoint, request.headers, Buffer.from('changed')));
  assert.throws(() => authenticateWindowsDispatchRequest({ ...endpoint, session_id: 'c237d37c-687a-499f-b2fb-c172085ab056' }, request.headers, body));
  assert.throws(() => authenticateWindowsDispatchRequest({ ...endpoint, manifest_sha256: 'd'.repeat(64) }, request.headers, body));
  const bytes = Buffer.from('{"accepted":true}');
  const headers = signWindowsDispatchResponse(endpoint, proof, 200, bytes);
  authenticateWindowsDispatchResponse(endpoint, proof, 200, headers, bytes);
  assert.throws(() => authenticateWindowsDispatchResponse(endpoint, proof, 409, headers, bytes));
  assert.throws(() => authenticateWindowsDispatchResponse(endpoint, proof, 200, headers, Buffer.from('{}')));
  assert.throws(() => authenticateWindowsDispatchResponse(endpoint, createWindowsDispatchRequest(endpoint, body).proof, 200, headers, bytes));
  assert.throws(() => authenticateWindowsDispatchResponse(endpoint, proof, 200, { 'x-memory-response-mac': request.headers['x-memory-request-mac'] }, bytes));
});
test('fake stale-port owner receives no secret and cannot supply acceptable JSON evidence', () => {
  const request = createWindowsDispatchRequest(endpoint, body);
  const stolen = { request: body.toString(), headers: request.headers };
  assert(!JSON.stringify(stolen).includes(endpoint.key));
  assert.throws(() => authenticateWindowsDispatchResponse(endpoint, request.proof, 200, {}, Buffer.from('{"forged":true}')));
  assert.throws(() => authenticateWindowsDispatchResponse(endpoint, request.proof, 200,
    { 'x-memory-response-mac': '0'.repeat(64) }, Buffer.from('{"forged":true}')));
});
test('transport nonces change while application idempotency bytes stay identical', () => {
  const first = createWindowsDispatchRequest(endpoint, body), second = createWindowsDispatchRequest(endpoint, body);
  assert.equal(first.proof.digest, second.proof.digest);
  assert.notEqual(first.proof.nonce, second.proof.nonce);
  assert.throws(() => authenticateWindowsDispatchRequest(endpoint, { ...first.headers, 'x-memory-nonce': [first.proof.nonce] }, body));
  assert.throws(() => createWindowsDispatchRequest(endpoint, Buffer.alloc(32769)));
});
