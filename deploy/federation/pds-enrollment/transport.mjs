// SPDX-License-Identifier: AGPL-3.0-or-later
import https from 'node:https';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { check, OperatorError, parseJson } from './files.mjs';

const ALLOWED = new Map([
  ['com.atproto.server.createAccount', 'POST'], ['com.atproto.server.createSession', 'POST'],
  ['com.atproto.server.getSession', 'GET'], ['com.atproto.identity.getRecommendedDidCredentials', 'GET'],
  ['com.atproto.server.activateAccount', 'POST'], ['com.atproto.server.refreshSession', 'POST'],
]);

// Fixed HTTPS origins from private operator config; no URL from imported data,
// redirects, proxy environment, shared cookie jar or response body in errors.
export class HttpsTransport {
  rpc(origin, nsid, options, ca) {
    check(ALLOWED.get(nsid) === (options.method ?? 'GET'), 'unsupported_operation');
    return this.request(new URL(`/xrpc/${nsid}`, origin), options, ca);
  }
  document(url, ca) { return this.request(new URL(url), {}, ca); }
  request(url, { method = 'GET', token, body }, ca) {
    check(url.protocol === 'https:' && !url.username && !url.password && !url.hash && !url.search, 'unsafe_endpoint');
    const payload = body === undefined ? undefined : Buffer.from(JSON.stringify(body));
    check(!payload || payload.length <= 8192, 'request_too_large');
    return new Promise((resolve, reject) => {
      const fail = code => reject(new OperatorError(code));
      const request = https.request(url, { method, ca, rejectUnauthorized: true, agent: false,
        headers: { accept: 'application/json', ...(token ? { authorization: `Bearer ${token}` } : {}),
          ...(payload ? { 'content-type': 'application/json', 'content-length': payload.length } : {}) } }, response => {
        const chunks = []; let length = 0;
        response.on('data', chunk => {
          length += chunk.length;
          if (length > 131072) { response.destroy(); request.destroy(); fail('response_too_large'); }
          else chunks.push(chunk);
        });
        response.on('error', () => fail('network_outcome_unconfirmed'));
        response.on('end', () => {
          if (response.statusCode < 200 || response.statusCode >= 300) {
            fail([401, 403].includes(response.statusCode) ? 'credential_rejected' : 'remote_operation_unconfirmed'); return;
          }
          try {
            const bytes = Buffer.concat(chunks);
            if (!bytes.length && method === 'POST' && url.pathname.endsWith('/com.atproto.server.activateAccount')) { resolve({}); return; }
            check(/^application\/(?:json|did\+ld\+json)(?:;|$)/u.test(response.headers['content-type'] ?? ''), 'invalid_response_type');
            resolve(parseJson(bytes.toString('utf8')));
          } catch { fail('invalid_response'); }
        });
      });
      const timeout = setTimeout(() => { request.destroy(); fail('network_outcome_unconfirmed'); }, 30000);
      request.on('close', () => clearTimeout(timeout));
      request.on('error', () => fail('network_outcome_unconfirmed'));
      request.end(payload);
    });
  }
}

/** Load only the dependencies already embedded in the reviewed PDS image.
 * No fallback to host npm installations, package downloads or latest versions. */
export async function pinnedSigner() {
  try {
    const appRequire = createRequire('/app/index.ts');
    const entry = appRequire.resolve('@atproto/pds');
    let root = dirname(entry); let pkg;
    for (let depth = 0; depth < 4; depth++, root = dirname(root)) {
      try { pkg = parseJson(await readFile(join(root, 'package.json'), 'utf8')); }
      catch { continue; }
      if (pkg.name === '@atproto/pds') break;
    }
    check(pkg?.name === '@atproto/pds' && pkg.version === '0.5.27', 'pinned_sdk_required');
    const pdsRequire = createRequire(entry);
    const { Secp256k1Keypair, parseDidKey } = pdsRequire('@atproto/crypto');
    const { createServiceJwt } = pdsRequire('@atproto/xrpc-server');
    return {
      publicKey: async secret => (await Secp256k1Keypair.import(Buffer.from(secret, 'hex'))).did(),
      validPublicKey: async key => { try { return parseDidKey(key).jwtAlg === 'ES256K'; } catch { return false; } },
      serviceJwt: async (secret, config) => createServiceJwt({ iss: config.repository, aud: config.service_did,
        lxm: 'com.atproto.server.createAccount', keypair: await Secp256k1Keypair.import(Buffer.from(secret, 'hex')) }),
    };
  } catch { throw new OperatorError('pinned_sdk_required'); }
}
