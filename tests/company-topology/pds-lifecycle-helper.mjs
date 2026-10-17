// SPDX-License-Identifier: AGPL-3.0-or-later
// Synthetic-only one-shot helper, run inside the pinned image by the owned
// topology harness. No configurable input paths or arbitrary XRPC methods.
import assert from 'node:assert/strict';
import { createECDH, createHash, randomBytes, randomUUID } from 'node:crypto';
import { mkdir, readFile, writeFile, lstat } from 'node:fs/promises';
import https from 'node:https';
import { createRequire } from 'node:module';
import { PDS_IMAGE, loadConfig } from '/operator/operator.mjs';
import { pinnedSigner } from '/operator/transport.mjs';
import { main } from '/operator/cli.mjs';
import { OperatorError } from '/operator/files.mjs';

const ROOT = '/rehearsal';
const CONFIG = `${ROOT}/config.json`;
const CA = '/trust/ca.pem';
let diagnostic = 'startup';
let observedFailure;
const xrpcErrors = new Set(['InvalidRequest', 'InvalidToken', 'ExpiredToken', 'BadJwtSignature',
  'InvalidInviteCode', 'InvalidHandle', 'AccountTakedown', 'HandleNotAvailable', 'UnsupportedDomain', 'IncompatibleDidDoc',
  'InternalServerError', 'InvalidSignature', 'AuthRequired', 'InvalidEmail', 'RateLimitExceeded']);
// Test-only observation of failed responses from the fixed synthetic operator.
// Successful responses (which contain credentials) are never buffered here.
// No upstream text is emitted: only status and explicitly enumerated diagnoses.
async function observeSyntheticFailure(work) {
  const original = https.request;
  https.request = function (url, options, listener) {
    return original.call(https, url, options, response => {
      if (['operator.company.example.com', 'space.company.example.com'].includes(url.hostname) && response.statusCode >= 400) {
        const chunks = []; let size = 0;
        response.on('data', chunk => { size += chunk.length; if (size <= 8192) chunks.push(chunk); });
        response.on('end', () => {
          observedFailure = { upstream: url.hostname.startsWith('space.') ? 'did_document' : 'operator',
            http_status: response.statusCode, xrpc_error: 'unclassified', upstream_condition: 'unclassified' };
          if (size > 8192) return;
          try {
            const body = JSON.parse(Buffer.concat(chunks).toString('utf8'));
            if (xrpcErrors.has(body.error)) observedFailure.xrpc_error = body.error;
            const conditions = new Map([
              ['This email address is not supported, please use a different email.', 'unsupported_email'],
              ['No invite code provided', 'missing_invite'],
              ['Email is required', 'missing_email'],
            ]);
            observedFailure.upstream_condition = conditions.get(body.message) ?? 'unclassified';
          } catch { /* Never print unclassified server content. */ }
        });
      }
      listener(response);
    });
  };
  try { return await work(); } finally { https.request = original; }
}
const privateFile = (name, data) => writeFile(`${ROOT}/${name}`, data, { mode: 0o600, flag: 'wx' });
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
async function context() {
  assert.equal(await readFile(`${ROOT}/synthetic-only`, 'utf8'), 'memory-pds-disposable-rehearsal-v1\n');
  const config = await loadConfig(CONFIG);
  assert.equal(config.handle, 'space.company.example.com');
  assert.equal(config.repository, 'did:web:space.company.example.com');
  assert.equal(config.email, 'synthetic-rehearsal@company.example.com');
  return config;
}
async function request(url, { method = 'GET', token, basic, body } = {}) {
  const parsed = new URL(url);
  assert.equal(parsed.protocol, 'https:');
  assert.ok(['operator.company.example.com', 'pds.company.example.com'].includes(parsed.hostname));
  const ca = await readFile(CA);
  return new Promise((resolve, reject) => {
    const payload = body === undefined ? undefined : Buffer.from(JSON.stringify(body));
    const req = https.request(parsed, { method, ca, rejectUnauthorized: true, agent: false,
      headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(basic ? { authorization: `Basic ${basic}` } : {}),
        ...(payload ? { 'content-type': 'application/json', 'content-length': payload.length } : {}) } }, res => {
      const chunks = []; let length = 0;
      res.on('data', chunk => { length += chunk.length; if (length > 4 * 1024 * 1024) { res.destroy(); reject(Error('bounded synthetic response exceeded')); } else chunks.push(chunk); });
      res.on('error', () => reject(Error('synthetic upstream unavailable')));
      res.on('end', () => resolve({ status: res.statusCode, bytes: Buffer.concat(chunks), type: res.headers['content-type'] ?? '' }));
    });
    const timer = setTimeout(() => { req.destroy(); reject(Error('synthetic request timed out')); }, 15000);
    req.on('close', () => clearTimeout(timer)); req.on('error', () => reject(Error('synthetic request unavailable'))); req.end(payload);
  });
}
async function json(url, options) {
  const response = await request(url, options); assert.equal(response.status, 200);
  return JSON.parse(response.bytes.toString('utf8'));
}
async function run(args) {
  const [operation, ...values] = args;
  if (operation === 'prepare') {
    diagnostic = 'prepare_arguments';
    assert.deepEqual(values, ['company.example.com', 'space.company.example.com', 'operator.company.example.com']);
    diagnostic = 'prepare_private_root';
    const meta = await lstat(ROOT); assert.equal(meta.mode & 0o777, 0o700);
    diagnostic = 'prepare_pinned_sdk';
    const signer = await pinnedSigner();
    diagnostic = 'prepare_key_derivation';
    const bootstrap = createECDH('secp256k1'); bootstrap.generateKeys();
    const rotation = createECDH('secp256k1'); rotation.generateKeys();
    const key = bootstrap.getPrivateKey('hex');
    const publicKey = await signer.publicKey(key);
    const admin = randomBytes(32).toString('hex');
    diagnostic = 'prepare_private_files';
    await privateFile('synthetic-only', 'memory-pds-disposable-rehearsal-v1\n');
    await privateFile('bootstrap-key.hex', key); await privateFile('password', randomBytes(32).toString('hex'));
    await privateFile('admin', admin); await mkdir(`${ROOT}/state`, { mode: 0o700 });
    diagnostic = 'prepare_configuration';
    const config = { schema_version: 1, image: PDS_IMAGE, operation_id: randomUUID(),
      repository: 'did:web:space.company.example.com', handle: 'space.company.example.com', email: 'synthetic-rehearsal@company.example.com',
      pds_origin: 'https://pds.company.example.com', operator_origin: 'https://operator.company.example.com:8443', service_did: 'did:web:pds.company.example.com',
      tls_ca_file: CA, tls_ca_sha256: digest(await readFile(CA)), bootstrap_public_key: publicKey,
      bootstrap_key_file: `${ROOT}/bootstrap-key.hex`, password_file: `${ROOT}/password`, invite_file: `${ROOT}/invite`, state_directory: `${ROOT}/state`,
      review_not_before: new Date(Date.now() - 60000).toISOString().replace(/\.\d{3}Z$/u, 'Z'),
      review_expires_at: new Date(Date.now() + 3600000).toISOString().replace(/\.\d{3}Z$/u, 'Z') };
    await privateFile('config.json', JSON.stringify(config));
    diagnostic = 'prepare_pds_environment';
    await privateFile('pds.env', [
      `PDS_JWT_SECRET=${randomBytes(32).toString('hex')}`, `PDS_ADMIN_PASSWORD=${admin}`,
      `PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX=${rotation.getPrivateKey('hex')}`,
      'PDS_DID_PLC_URL=https://unavailable.company.example.com', 'LOG_LEVEL=warn',
    ].join('\n') + '\n');
    return { synthetic: true, image: PDS_IMAGE, sdk_version: '0.5.27', private_inputs_prepared: true };
  }
  diagnostic = 'read_synthetic_context';
  const config = await context();
  if (operation === 'phase') {
    assert.equal(values.length, 1);
    assert.ok(['bootstrap', 'create', 'discover-key', 'handoff', 'activate', 'refresh'].includes(values[0]));
    diagnostic = `preview_${values[0]}`;
    const review = await main([CONFIG, values[0]]);
    diagnostic = `execute_${values[0]}`;
    const result = await observeSyntheticFailure(() => main(['execute', CONFIG, values[0], review.review_sha256]));
    return { synthetic: true, review_sha256: review.review_sha256, result };
  }
  if (operation === 'invite') {
    diagnostic = 'invite_readiness';
    assert.equal(values.length, 0);
    let ready = false;
    for (let attempt = 0; attempt < 40; attempt++) {
      const response = await request(`${config.operator_origin}/xrpc/com.atproto.server.getSession`).catch(() => null);
      if (response?.status === 401) { ready = true; break; }
      await new Promise(resolve => setTimeout(resolve, 250));
    }
    assert.equal(ready, true);
    diagnostic = 'invite_private_credential';
    const admin = await readFile(`${ROOT}/admin`, 'utf8');
    diagnostic = 'invite_issue';
    const invitation = await json(`${config.operator_origin}/xrpc/com.atproto.server.createInviteCode`, {
      method: 'POST', basic: Buffer.from(`admin:${admin}`).toString('base64'), body: { useCount: 1 } });
    assert.equal(typeof invitation.code, 'string'); assert.ok(invitation.code.length >= 8);
    diagnostic = 'invite_persist'; await privateFile('invite', invitation.code);
    return { synthetic: true, invitation_persisted: true };
  }
  if (operation === 'publish-verify') {
    diagnostic = 'verify_private_session';
    assert.equal(values.length, 0);
    const session = JSON.parse(await readFile(`${ROOT}/state/session.json`, 'utf8'));
    const state = JSON.parse(await readFile(`${ROOT}/state/journal.json`, 'utf8'));
    assert.equal(state.phase, 'active'); assert.equal(session.did, config.repository);
    const record = { $type: 'dev.codifix.unspecced.rehearsal', purpose: 'Synthetic isolated SDK and signed-CAR verification only', id: randomUUID() };
    const rkey = randomUUID();
    diagnostic = 'verify_publish';
    const created = await json(`${config.pds_origin}/xrpc/com.atproto.repo.createRecord`, { method: 'POST', token: session.access_jwt,
      body: { repo: config.repository, collection: record.$type, rkey, record, validate: false } });
    assert.equal(created.uri, `at://${config.repository}/${record.$type}/${rkey}`);
    diagnostic = 'verify_readback';
    const read = await json(`${config.pds_origin}/xrpc/com.atproto.repo.getRecord?${new URLSearchParams({ repo: config.repository, collection: record.$type, rkey })}`, { token: session.access_jwt });
    assert.deepEqual(read.value, record); assert.equal(read.cid, created.cid);
    diagnostic = 'verify_download';
    const response = await request(`${config.pds_origin}/xrpc/com.atproto.sync.getRepo?${new URLSearchParams({ did: config.repository })}`, { token: session.access_jwt });
    assert.equal(response.status, 200); assert.ok(response.bytes.length > 0);
    diagnostic = 'verify_independent_sdk';
    const appRequire = createRequire('/app/index.ts'); const pdsRequire = createRequire(appRequire.resolve('@atproto/pds'));
    const { verifyRepoCar } = pdsRequire('@atproto/repo');
    assert.ok(await verifyRepoCar(response.bytes, config.repository, state.signing_key));
    diagnostic = 'verify_wrong_key_rejection';
    await assert.rejects(verifyRepoCar(response.bytes, config.repository, config.bootstrap_public_key));
    return { synthetic: true, repository_verified: true, wrong_key_rejected: true, records: 1, car_bytes: response.bytes.length };
  }
  throw Error('unsupported synthetic helper operation');
}
run(process.argv.slice(2)).then(value => process.stdout.write(JSON.stringify(value) + '\n')).catch(error => {
  const known = new Set(['prepare', 'phase', 'invite', 'publish-verify']);
  process.stderr.write(JSON.stringify({ error: error instanceof OperatorError ? error.code : 'synthetic_pds_lifecycle_failed',
    stage: known.has(process.argv[2]) ? process.argv[2] : 'invalid', diagnostic, ...(observedFailure ?? {}), credentials_logged: false }) + '\n'); process.exitCode = 1;
});
