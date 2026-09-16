// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host transport primitive. No environment lookup, automatic retries,
// redirects, SDK tools or credential persistence. Calling it still requires the
// campaign's approval and durable pre-dispatch reservation; this is not a runner.
import { providerFailure, preserveProviderFailure } from './provider-diagnostics.mjs';
export function createProviderTransport({ endpoint, bearer, offline = true, api = 'responses' }) {
  if (!['responses', 'embeddings'].includes(api)) throw Error('invalid provider API');
  let url;
  try { url = new URL(endpoint); } catch { throw Error('invalid provider destination'); }
  const local = url.protocol === 'http:' && url.hostname === '127.0.0.1' && !!url.port
    && url.pathname === `/v1/${api}`;
  const hosted = url.href === `https://api.openai.com/v1/${api}`;
  if (typeof offline !== 'boolean' || url.username || url.password || url.search || url.hash
      || (offline ? !local : !hosted) || typeof bearer !== 'string'
      || !/^[A-Za-z0-9._-]{1,512}$/.test(bearer)
      || (offline && !bearer.startsWith('synthetic-'))) throw Error('invalid provider destination or credential contract');
  const target = url.href;
  return async (body, signal) => {
    if (!(signal instanceof AbortSignal) || signal.aborted) throw Error('provider dispatch requires active deadline signal');
    let bytes;
    try { bytes = JSON.stringify(body); } catch { throw Error('invalid provider request'); }
    if (!bytes || !body || Array.isArray(body) || typeof body !== 'object'
        || Buffer.byteLength(bytes) > 1024 * 1024) throw Error('invalid provider request');
    const deadline = AbortSignal.any([signal, AbortSignal.timeout(600000)]);
    let response;
    try {
      response = await fetch(target, { method: 'POST', redirect: 'error', signal: deadline,
        headers: { Authorization: `Bearer ${bearer}`, 'Content-Type': 'application/json', Accept: 'application/json' }, body: bytes });
      const metadata = { http_status: response.status, request_id: response.headers.get('x-request-id') };
      if (response.status !== 200) {
        // Read a small bounded error envelope only to select closed-vocabulary
        // codes. Never retain its message or an unrestricted parameter string.
        const reader = response.body?.getReader(), chunks = [];
        let size = 0;
        try {
          if (reader) for (;;) {
            const { value, done } = await reader.read();
            if (done) break;
            if ((size += value.length) > 16384) break;
            chunks.push(value);
          }
          if (size <= 16384) {
            const detail = JSON.parse(Buffer.concat(chunks).toString('utf8'))?.error;
            metadata.api_code = detail?.code ?? detail?.type;
            metadata.parameter = detail?.param;
          }
        } catch { /* A malformed error is still an HTTP failure. */ }
        finally { if (reader) { try { await reader.cancel(); } finally { reader.releaseLock(); } } }
        throw providerFailure('http', metadata);
      }
      if (response.redirected
          || response.headers.get('content-type')?.split(';')[0].trim().toLowerCase() !== 'application/json') {
        await response.body?.cancel();
        throw providerFailure('content_type', metadata);
      }
      const declared = response.headers.get('content-length');
      if (declared !== null && (!/^[0-9]+$/.test(declared) || Number(declared) > 1024 * 1024)) {
        await response.body?.cancel();
        throw providerFailure('oversized', metadata);
      }
      if (!response.body) throw Error('missing response');
      const reader = response.body.getReader();
      const chunks = [];
      let length = 0;
      try {
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          if ((length += value.byteLength) > 1024 * 1024) throw providerFailure('oversized', metadata);
          chunks.push(value);
        }
        try { return new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks)); }
        catch { throw providerFailure('encoding', metadata); }
      } finally {
        try { await reader.cancel(); } finally { reader.releaseLock(); }
      }
    } catch (error) {
      // Dispatch may already have reached the provider. Never infer that an HTTP
      // failure, deadline or lost response authorizes another paid invocation.
      throw preserveProviderFailure(error, deadline.aborted ? 'cancelled' : 'transport');
    }
  };
}
