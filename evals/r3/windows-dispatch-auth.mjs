// SPDX-License-Identifier: AGPL-3.0-or-later
// Mutual authentication for explicit offline Windows loopback dispatch. The
// secret stays in the protected endpoint, never in headers or agent context.
import { createHash, createHmac, randomBytes, timingSafeEqual } from 'node:crypto';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const hex = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const uuid = value => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
const fields = ['configuration_sha256', 'format', 'host', 'key', 'manifest_sha256', 'port', 'session_id'];
export function validateWindowsDispatchEndpoint(value, expected) {
  if (!value || Object.keys(value).sort().join(',') !== fields.join(',')
      || value.format !== 'memory-r3-windows-loopback-v1' || value.host !== '127.0.0.1'
      || !Number.isSafeInteger(value.port) || value.port < 1 || value.port > 65535
      || !uuid(value.session_id) || !hex(value.key)
      || !hex(value.manifest_sha256) || !hex(value.configuration_sha256)
      || value.manifest_sha256 !== expected.manifestSha256
      || value.configuration_sha256 !== expected.configurationSha256) throw Error('invalid native dispatch endpoint');
  return structuredClone(value);
}
function mac(endpoint, fields) {
  return createHmac('sha256', Buffer.from(endpoint.key, 'hex'))
    .update(['memory-r3-windows-dispatch-v1', endpoint.session_id, endpoint.manifest_sha256,
      endpoint.configuration_sha256, ...fields].join('\n')).digest('hex');
}
function equalMac(actual, expected) {
  return hex(actual) && timingSafeEqual(Buffer.from(actual, 'hex'), Buffer.from(expected, 'hex'));
}
function requestMac(endpoint, nonce, digest) { return mac(endpoint, ['request', nonce, 'POST', '/dispatch', digest]); }
function responseMac(endpoint, proof, status, bytes) { return mac(endpoint, ['response', proof.nonce, proof.digest, String(status), hash(bytes)]); }
export function createWindowsDispatchRequest(endpoint, bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length > 32768) throw Error('invalid native dispatch request');
  const proof = { nonce: randomBytes(32).toString('hex'), digest: hash(bytes) };
  return { proof, headers: { 'x-memory-nonce': proof.nonce,
    'x-memory-request-mac': requestMac(endpoint, proof.nonce, proof.digest) } };
}
export function authenticateWindowsDispatchRequest(endpoint, headers, bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length > 32768 || !hex(headers['x-memory-nonce'])) throw Error('native dispatch authentication failed');
  const proof = { nonce: headers['x-memory-nonce'], digest: hash(bytes) };
  if (!equalMac(headers['x-memory-request-mac'], requestMac(endpoint, proof.nonce, proof.digest))) throw Error('native dispatch authentication failed');
  return proof;
}
export function signWindowsDispatchResponse(endpoint, proof, status, bytes) {
  if (!hex(proof?.nonce) || !hex(proof?.digest) || !Number.isSafeInteger(status) || status < 100 || status > 599
      || !Buffer.isBuffer(bytes) || bytes.length > 16 * 1024 * 1024) throw Error('invalid native dispatch response');
  return { 'x-memory-response-mac': responseMac(endpoint, proof, status, bytes) };
}
export function authenticateWindowsDispatchResponse(endpoint, proof, status, headers, bytes) {
  const expected = signWindowsDispatchResponse(endpoint, proof, status, bytes)['x-memory-response-mac'];
  if (!equalMac(headers['x-memory-response-mac'], expected)) throw Error('native dispatch response authentication failed');
}
