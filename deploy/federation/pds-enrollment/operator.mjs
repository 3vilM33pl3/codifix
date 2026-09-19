// SPDX-License-Identifier: AGPL-3.0-or-later
import { absolute, canonical, check, digest, hash, parseJson, privateRead, StateFiles, strict } from './files.mjs';
import { join } from 'node:path';

export const PDS_IMAGE = 'ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12';
const PHASES = ['bootstrap', 'create', 'discover-key', 'handoff', 'activate', 'reconcile', 'refresh'];
const CONFIG_KEYS = ['schema_version', 'image', 'operation_id', 'repository', 'handle', 'email', 'pds_origin', 'operator_origin', 'service_did', 'tls_ca_file', 'tls_ca_sha256', 'bootstrap_public_key', 'bootstrap_key_file', 'password_file', 'invite_file', 'state_directory', 'review_not_before', 'review_expires_at'];
const STATES = ['bootstrap', 'create_pending', 'created', 'key_review', 'handoff', 'activate_pending', 'active', 'refresh_pending'];
const HEX = /^[a-f0-9]{64}$/u;
const publicKey = key => typeof key === 'string' && /^did:key:z[1-9A-HJ-NP-Za-km-z]{40,100}$/u.test(key);
function host(value) { return typeof value === 'string' && value.length <= 190 && value.includes('.') && !/^[0-9.]+$/u.test(value) && value.split('.').every(part => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/u.test(part)); }
function timestamp(value) { check(typeof value === 'string' && /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/u.test(value)); const n = Date.parse(value); check(Number.isFinite(n) && new Date(n).toISOString() === value.replace('Z', '.000Z')); return n; }
function origin(value) {
  const endpoint = new URL(value);
  check(endpoint.protocol === 'https:' && host(endpoint.hostname) && endpoint.origin === value
    && !endpoint.username && !endpoint.password && ['', '8443'].includes(endpoint.port)
    && endpoint.pathname === '/' && !endpoint.search && !endpoint.hash);
  return endpoint;
}
export function validateConfig(config) {
  strict(config, CONFIG_KEYS);
  check(config.schema_version === 1 && config.image === PDS_IMAGE);
  check(/^[a-f0-9]{8}-[a-f0-9]{4}-[1-8][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/u.test(config.operation_id));
  check(host(config.handle) && config.repository === `did:web:${config.handle}` && publicKey(config.bootstrap_public_key));
  check(typeof config.email === 'string' && config.email.length <= 254 && /^[^\s@]+@[^\s@]+\.[^\s@]+$/u.test(config.email));
  const endpoint = origin(config.pds_origin); const operator = origin(config.operator_origin);
  check(config.service_did === `did:web:${endpoint.hostname}` && endpoint.hostname !== config.handle
    && operator.hostname !== config.handle && operator.hostname !== endpoint.hostname);
  check(HEX.test(config.tls_ca_sha256));
  for (const key of ['tls_ca_file', 'bootstrap_key_file', 'password_file', 'invite_file', 'state_directory']) absolute(config[key]);
  check(new Set(['tls_ca_file', 'bootstrap_key_file', 'password_file', 'invite_file', 'state_directory'].map(key => config[key])).size === 5);
  const outputs = ['did.json', 'journal.json', 'session.json', 'operator.lock'].map(name => join(config.state_directory, name));
  for (const key of ['tls_ca_file', 'bootstrap_key_file', 'password_file', 'invite_file']) check(!outputs.includes(config[key]), 'input_output_alias');
  const start = timestamp(config.review_not_before); const end = timestamp(config.review_expires_at);
  check(end > start && end - start <= 86400000);
  return config;
}
export async function loadConfig(path) {
  return validateConfig(parseJson((await privateRead(path)).toString('utf8')));
}
export function identityDigest(config) {
  const { review_not_before, review_expires_at, ...identity } = config;
  return digest(identity);
}
function document(config, key) {
  return { '@context': ['https://www.w3.org/ns/did/v1'], id: config.repository,
    alsoKnownAs: [`at://${config.handle}`],
    verificationMethod: [{ id: `${config.repository}#atproto`, type: 'Multikey', controller: config.repository, publicKeyMultibase: key.slice('did:key:'.length) }],
    service: [{ id: `${config.repository}#atproto_pds`, type: 'AtprotoPersonalDataServer', serviceEndpoint: config.pds_origin }] };
}
function validateJournal(config, state) {
  return validateEnrolledJournal(state, identityDigest(config));
}
export function validateEnrolledJournal(state, identity) {
  if (!state) return;
  strict(state, ['schema_version', 'identity_sha256', 'phase', 'revision', 'session_generation', 'signing_key', 'input_sha256', 'resume_phase', 'pending_started_at', 'resume_pending_started_at']);
  check(state.schema_version === 1 && state.identity_sha256 === identity && STATES.includes(state.phase)
    && Number.isSafeInteger(state.revision) && state.revision > 0 && Number.isSafeInteger(state.session_generation) && state.session_generation >= 0
    && (state.signing_key === null || publicKey(state.signing_key))
    && (state.input_sha256 === null || HEX.test(state.input_sha256))
    && (state.resume_phase === null || ['created', 'key_review', 'handoff', 'activate_pending', 'active'].includes(state.resume_phase))
    && (state.pending_started_at === null || (Number.isSafeInteger(state.pending_started_at) && state.pending_started_at > 0))
    && (state.resume_pending_started_at === null || (Number.isSafeInteger(state.resume_pending_started_at) && state.resume_pending_started_at > 0)), 'journal_binding_mismatch');
  check((state.phase === 'refresh_pending') === (state.resume_phase !== null)
    && (state.resume_phase === 'activate_pending') === (state.resume_pending_started_at !== null)
    && (['create_pending', 'activate_pending', 'refresh_pending'].includes(state.phase)) === (state.pending_started_at !== null)
    && (!['key_review', 'handoff', 'activate_pending', 'active'].includes(state.phase) || state.signing_key !== null)
    && (state.phase === 'bootstrap' || state.input_sha256 !== null), 'journal_binding_mismatch');
}
function nextPhase(state) {
  return !state ? 'bootstrap' : ({ bootstrap: 'create', create_pending: 'reconcile', created: 'discover-key', key_review: 'handoff', handoff: 'activate', activate_pending: 'reconcile', active: 'refresh', refresh_pending: 'reconcile' })[state.phase];
}
function allowed(state, phase) {
  if (!state) return phase === 'bootstrap';
  const normal = nextPhase(state) === phase;
  return normal || (phase === 'refresh' && ['created', 'key_review', 'handoff', 'activate_pending', 'active'].includes(state.phase));
}
function operations(state, phase) {
  return ({ bootstrap: [], create: ['GET reviewed bootstrap DID document', 'POST com.atproto.server.createAccount'],
    'discover-key': ['GET com.atproto.server.getSession', 'GET com.atproto.identity.getRecommendedDidCredentials'], handoff: [],
    activate: ['GET reviewed handoff DID document', 'GET com.atproto.server.getSession', 'POST com.atproto.server.activateAccount', 'GET com.atproto.server.getSession'],
    refresh: ['POST com.atproto.server.refreshSession'],
    reconcile: state?.phase === 'create_pending' ? ['POST com.atproto.server.createSession for exact DID']
      : state?.phase === 'activate_pending' ? ['GET reviewed handoff DID document', 'GET com.atproto.server.getSession']
        : ['GET com.atproto.server.getSession for already saved session, otherwise POST com.atproto.server.refreshSession using retained token'] })[phase];
}

export class EnrollmentOperator {
  constructor(config, { transport, signer, now = () => Date.now(), files = new StateFiles(config.state_directory) } = {}) {
    this.config = validateConfig(config); this.transport = transport; this.signer = signer; this.now = now; this.files = files;
  }
  async state() { await this.files.validate(); const state = await this.files.read('journal.json'); validateJournal(this.config, state); return state; }
  async preview(phase) {
    const state = await this.state(); phase ??= nextPhase(state);
    check(PHASES.includes(phase) && allowed(state, phase), 'phase_requires_reconciliation');
    const review = { purpose: 'memory-private-pds-enrollment-v1', config_sha256: digest(this.config),
      identity_sha256: identityDigest(this.config), journal_sha256: digest(state), phase,
      repository: this.config.repository, pds_origin: this.config.pds_origin, operator_origin: this.config.operator_origin,
      review_not_before: this.config.review_not_before, review_expires_at: this.config.review_expires_at,
      current_state: state?.phase ?? 'uninitialized', network_operations: operations(state, phase),
      did_document: ['bootstrap', 'handoff'].includes(phase) ? document(this.config, phase === 'bootstrap' ? this.config.bootstrap_public_key : state.signing_key) : null };
    review.did_document_sha256 = review.did_document ? hash(`${canonical(review.did_document)}\n`) : null;
    return { ...review, review_sha256: digest(review), executed: false };
  }
  async execute(phase, confirmation) {
    return this.files.locked(async () => {
      // A separately approved refresh-only owner makes the enrollment operator
      // unavailable until an operator explicitly retires that owner. Check
      // inside the enrollment lock so adoption cannot race a manual refresh.
      check(!await this.files.read('refresh-owner.json'), 'refresh_only_maintenance_owns_session');
      const review = await this.preview(phase);
      check(HEX.test(confirmation) && review.review_sha256 === confirmation, 'exact_review_digest_required');
      this.currentReview();
      let state = await this.state();
      const ca = await privateRead(this.config.tls_ca_file, { publicFile: true, limit: 16384 });
      check(hash(ca) === this.config.tls_ca_sha256, 'tls_trust_changed');
      const save = async updates => {
        state = { ...(state ?? { schema_version: 1, identity_sha256: identityDigest(this.config), revision: 0,
          session_generation: 0, signing_key: null, input_sha256: null, resume_phase: null, pending_started_at: null, resume_pending_started_at: null }), ...updates, revision: (state?.revision ?? 0) + 1 };
        await this.files.write('journal.json', state);
      };
      if (phase === 'bootstrap') {
        const key = await this.secret('bootstrap_key_file');
        check(/^[a-f0-9]{64}$/u.test(key), 'invalid_bootstrap_key');
        check(await this.signer.publicKey(key) === this.config.bootstrap_public_key, 'bootstrap_key_mismatch');
        const old = await this.files.read('did.json');
        check(!await this.files.read('session.json') && (!old || canonical(old) === canonical(document(this.config, this.config.bootstrap_public_key))), 'unowned_output_present');
        await this.files.write('did.json', document(this.config, this.config.bootstrap_public_key));
        await save({ phase: 'bootstrap' });
      } else if (phase === 'create') {
        await this.verifyDocument(this.config.bootstrap_public_key, ca);
        const inputs = await this.inputs();
        check(await this.signer.publicKey(inputs.key) === this.config.bootstrap_public_key, 'bootstrap_key_mismatch');
        const jwt = await this.signer.serviceJwt(inputs.key, this.config);
        // Durable intent before the first potentially accepted create. No
        // subsequent invocation can blindly repeat this create operation.
        await save({ phase: 'create_pending', input_sha256: digest(inputs), pending_started_at: this.now() });
        const response = await this.rpc('com.atproto.server.createAccount', { method: 'POST', token: jwt,
          body: { did: this.config.repository, handle: this.config.handle, email: this.config.email, password: inputs.password, inviteCode: inputs.invite } }, ca);
        await this.saveSession(response, state.session_generation + 1);
        await save({ phase: 'created', session_generation: state.session_generation + 1, pending_started_at: null });
      } else if (phase === 'discover-key') {
        const session = await this.session(state); this.currentToken(session, 'access');
        await this.getSession(session, ca);
        const response = await this.rpc('com.atproto.identity.getRecommendedDidCredentials', { token: session.access_jwt }, ca);
        const key = response?.verificationMethods?.atproto;
        check(publicKey(key) && key !== this.config.bootstrap_public_key && await this.signer.validPublicKey(key)
          && canonical(response.alsoKnownAs) === canonical([`at://${this.config.handle}`])
          && response.services?.atproto_pds?.type === 'AtprotoPersonalDataServer'
          && response.services.atproto_pds.endpoint === this.config.pds_origin, 'recommended_identity_mismatch');
        await save({ phase: 'key_review', signing_key: key });
      } else if (phase === 'handoff') {
        const old = await this.files.read('did.json');
        check([this.config.bootstrap_public_key, state.signing_key].some(key => canonical(old) === canonical(document(this.config, key))), 'local_did_binding_mismatch');
        await this.files.write('did.json', document(this.config, state.signing_key));
        await save({ phase: 'handoff' });
      } else if (phase === 'activate') {
        await this.verifyDocument(state.signing_key, ca);
        const session = await this.session(state); this.currentToken(session, 'access');
        await this.getSession(session, ca);
        await save({ phase: 'activate_pending', pending_started_at: this.now() });
        await this.rpc('com.atproto.server.activateAccount', { method: 'POST', token: session.access_jwt }, ca);
        const confirmed = await this.getSession(session, ca);
        check(confirmed.active === true, 'activation_not_confirmed');
        await save({ phase: 'active', pending_started_at: null });
      } else if (phase === 'refresh') {
        const previous = state.phase;
        const session = await this.session(state); this.currentToken(session, 'refresh');
        await save({ phase: 'refresh_pending', resume_phase: previous, pending_started_at: this.now(),
          resume_pending_started_at: previous === 'activate_pending' ? state.pending_started_at : null });
        await this.refresh(state, session, ca, save);
      } else if (state.phase === 'create_pending') {
        const inputs = await this.inputs();
        check(digest(inputs) === state.input_sha256, 'original_inputs_changed');
        // Exact DID + original password only. A lost create reply may have
        // produced a real account; never mint an alternate identity or key.
        const response = await this.rpc('com.atproto.server.createSession', { method: 'POST',
          body: { identifier: this.config.repository, password: inputs.password } }, ca);
        await this.saveSession(response, state.session_generation + 1);
        await save({ phase: 'created', session_generation: state.session_generation + 1, pending_started_at: null });
      } else if (state.phase === 'activate_pending') {
        await this.verifyDocument(state.signing_key, ca);
        const session = await this.session(state); this.currentToken(session, 'access');
        const response = await this.getSession(session, ca);
        // An authoritative inactive response permits a newly reviewed activate
        // invocation; the reconciliation itself never repeats activation.
        await save({ phase: response.active === true ? 'active' : 'handoff', pending_started_at: null });
      } else if (state.phase === 'refresh_pending') {
        const session = await this.session(state, true);
        if (session.generation === state.session_generation + 1) {
          // Output persisted before a crash advancing the journal. Never invoke
          // the provider again merely to finish our local bookkeeping.
          if (session.access_expires_at > this.now() + 30000) await this.getSession(session, ca);
          await save({ phase: state.resume_phase, resume_phase: null, session_generation: session.generation,
            pending_started_at: state.resume_pending_started_at, resume_pending_started_at: null });
        } else {
          check(this.now() < state.pending_started_at + 2 * 3600000, 'refresh_reconciliation_window_expired');
          this.currentToken(session, 'refresh');
          await this.refresh(state, session, ca, save);
        }
      }
      const currentSession = await this.files.read('session.json');
      return { executed: true, phase, state: state.phase, journal_revision: state.revision,
        repository: this.config.repository, session_generation: state.session_generation,
        access_expires_at: currentSession?.access_expires_at ?? null, refresh_expires_at: currentSession?.refresh_expires_at ?? null,
        signing_key: state.signing_key, did_document_sha256: ['bootstrap', 'handoff'].includes(phase) ? hash(await this.files.bytes('did.json')) : null,
        gateway_ready: false, repository_verified: false, gateway_restart_required: ['create', 'refresh', 'reconcile'].includes(phase) };
    });
  }
  async secret(field) {
    const value = (await privateRead(this.config[field], { limit: 4096 })).toString('utf8').trim();
    check(value.length >= (field === 'invite_file' ? 8 : 32) && value.length <= 1024 && !/[\u0000-\u001f\u007f]/u.test(value), 'invalid_secret_file');
    return value;
  }
  async inputs() { return { key: await this.secret('bootstrap_key_file'), password: await this.secret('password_file'), invite: await this.secret('invite_file') }; }
  currentReview() { check(this.now() >= timestamp(this.config.review_not_before) && this.now() < timestamp(this.config.review_expires_at), 'review_expired'); }
  async rpc(nsid, options, ca) { this.currentReview(); return this.transport.rpc(this.config.operator_origin, nsid, options, ca); }
  async verifyDocument(key, ca) {
    this.currentReview();
    const result = await this.transport.document(`https://${this.config.handle}/.well-known/did.json`, ca);
    check(canonical(result) === canonical(document(this.config, key)), 'served_did_document_mismatch');
  }
  async getSession(session, ca) {
    const result = await this.rpc('com.atproto.server.getSession', { token: session.access_jwt }, ca);
    check(result?.did === this.config.repository && result.handle === this.config.handle && typeof result.active === 'boolean', 'session_identity_mismatch');
    return result;
  }
  token(token, scope) {
    check(typeof token === 'string' && token.length >= 32 && token.length <= 4096 && token.split('.').length === 3, 'invalid_session_response');
    const parts = token.split('.');
    check(parts.every(part => /^[A-Za-z0-9_-]+$/u.test(part)), 'invalid_session_response');
    const claims = parseJson(Buffer.from(parts[1], 'base64url').toString('utf8'));
    check(claims.sub === this.config.repository && claims.aud === this.config.service_did
      && claims.scope === `com.atproto.${scope}` && Number.isSafeInteger(claims.exp) && claims.exp > 0
      && claims.exp <= Math.floor(Number.MAX_SAFE_INTEGER / 1000), 'session_identity_mismatch');
    // Decoding schedules refresh, never verifies a token cryptographically.
    // Its authority comes from the fixed authenticated TLS PDS response.
    return claims.exp * 1000;
  }
  async saveSession(response, generation) {
    check(response?.did === this.config.repository && response.handle === this.config.handle, 'session_identity_mismatch');
    const session = { schema_version: 1, identity_sha256: identityDigest(this.config), generation,
      did: response.did, handle: response.handle, access_jwt: response.accessJwt, refresh_jwt: response.refreshJwt,
      access_expires_at: this.token(response.accessJwt, 'access'), refresh_expires_at: this.token(response.refreshJwt, 'refresh') };
    this.currentToken(session, 'access'); this.currentToken(session, 'refresh');
    await this.files.write('session.json', session);
  }
  async session(state, pending = false) {
    const session = await this.files.read('session.json');
    strict(session, ['schema_version', 'identity_sha256', 'generation', 'did', 'handle', 'access_jwt', 'refresh_jwt', 'access_expires_at', 'refresh_expires_at']);
    check(session.schema_version === 1 && session.identity_sha256 === identityDigest(this.config)
      && (session.generation === state.session_generation || (pending && session.generation === state.session_generation + 1))
      && session.did === this.config.repository && session.handle === this.config.handle
      && session.access_expires_at === this.token(session.access_jwt, 'access')
      && session.refresh_expires_at === this.token(session.refresh_jwt, 'refresh'), 'session_binding_mismatch');
    return session;
  }
  currentToken(session, kind) { check(session[`${kind}_expires_at`] > this.now() + 30000, `${kind}_credential_expired`); }
  async refresh(state, session, ca, save) {
    const response = await this.rpc('com.atproto.server.refreshSession', { method: 'POST', token: session.refresh_jwt }, ca);
    await this.saveSession(response, state.session_generation + 1);
    await save({ phase: state.resume_phase, resume_phase: null, session_generation: state.session_generation + 1,
      pending_started_at: state.resume_pending_started_at, resume_pending_started_at: null });
  }
}
