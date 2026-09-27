// SPDX-License-Identifier: AGPL-3.0-or-later
import { constants } from 'node:fs';
import { open } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { X509Certificate } from 'node:crypto';
import { join } from 'node:path';
import { absolute, canonical, check, digest, directoryHandle, hash, OperatorError, parseJson, privateRead, safeFile, StateFiles, strict } from './files.mjs';
import { EnrollmentOperator, PDS_IMAGE, validateEnrolledJournal } from './operator.mjs';

const PURPOSE = 'memory-private-pds-refresh-only-v1';
const HEX = /^[a-f0-9]{64}$/u;
const POLICY_KEYS = ['schema_version', 'purpose', 'policy_id', 'image', 'enrollment_identity_sha256',
  'repository', 'handle', 'pds_origin', 'operator_origin', 'service_did', 'signing_key', 'did_document_sha256',
  'tls_ca_file', 'tls_ca_sha256', 'state_directory', 'output_directory', 'not_before'];
export const REFRESH_DEFAULTS = Object.freeze({ poll_seconds: 60, refresh_before_seconds: 900,
  max_attempts: 5, initial_retry_seconds: 10, max_retry_seconds: 600, reconciliation_seconds: 7200 });
const STATES = ['healthy', 'refreshing', 'retry_wait', 'intervention'];
const TRANSIENT = new Set(['network_outcome_unconfirmed', 'remote_operation_unconfirmed', 'remote_rate_limited', 'local_persistence_unconfirmed']);
const hostname = value => typeof value === 'string' && value.length <= 190 && value.includes('.')
  && !/^[0-9.]+$/u.test(value) && value.split('.').every(part => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/u.test(part));
function origin(value) {
  const url = new URL(value);
  check(url.origin === value && url.protocol === 'https:' && hostname(url.hostname)
    && !url.username && !url.password && ['', '8443'].includes(url.port), 'invalid_refresh_policy');
  return url;
}
export function validateRefreshPolicy(policy) {
  strict(policy, POLICY_KEYS);
  check(policy.schema_version === 1 && policy.purpose === PURPOSE && policy.image === PDS_IMAGE, 'invalid_refresh_policy');
  check(/^[a-f0-9]{8}-[a-f0-9]{4}-[1-8][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/u.test(policy.policy_id), 'invalid_refresh_policy');
  check(hostname(policy.handle) && policy.repository === `did:web:${policy.handle}`, 'invalid_refresh_policy');
  const pds = origin(policy.pds_origin); const operator = origin(policy.operator_origin);
  check(policy.service_did === `did:web:${pds.hostname}` && new Set([policy.handle, pds.hostname, operator.hostname]).size === 3, 'invalid_refresh_policy');
  check(typeof policy.signing_key === 'string' && /^did:key:z[1-9A-HJ-NP-Za-km-z]{40,100}$/u.test(policy.signing_key), 'invalid_refresh_policy');
  for (const field of ['enrollment_identity_sha256', 'tls_ca_sha256', 'did_document_sha256']) check(HEX.test(policy[field]), 'invalid_refresh_policy');
  for (const field of ['tls_ca_file', 'state_directory', 'output_directory']) absolute(policy[field]);
  const { state_directory: state, output_directory: output } = policy;
  check(state !== output && !state.startsWith(`${output}/`) && !output.startsWith(`${state}/`)
    && !policy.tls_ca_file.startsWith(`${output}/`) && !policy.tls_ca_file.startsWith(`${state}/`), 'refresh_input_output_alias');
  check(Number.isSafeInteger(policy.not_before) && policy.not_before > 0, 'invalid_refresh_policy');
  return policy;
}
export async function loadRefreshPolicy(path) {
  return validateRefreshPolicy(parseJson((await privateRead(path)).toString('utf8')));
}

// flock is acquired on an inherited descriptor sharing our open-file description.
// Its short-lived subprocess exits, but Linux retains the lock until this parent
// closes the descriptor. A killed parent therefore releases it without PID-based
// stale-lock deletion. The lock inode is persistent and must never be removed.
export class RefreshFiles extends StateFiles {
  path(name) {
    if (/^(?:recovery|tls-recovery)-[1-9][0-9]{0,3}\.json$/u.test(name)) return join(this.directory, name);
    return super.path(name);
  }
  async kernelLocked(work) {
    check(!this.bound, 'refresh_maintainer_busy');
    const directory = await directoryHandle(this.directory, true);
    let lock;
    try {
      const path = `/proc/self/fd/${directory.fd}/refresh.lock`;
      lock = await open(path, constants.O_RDWR | constants.O_CREAT | constants.O_NOFOLLOW | constants.O_NONBLOCK, 0o600);
      const owned = await lock.stat(); safeFile(owned, 4096, false);
      const code = await new Promise((resolve, reject) => {
        const child = spawn('/usr/bin/flock', ['--exclusive', '--nonblock', '3'], { stdio: ['ignore', 'ignore', 'ignore', lock.fd] });
        child.once('error', () => reject(new OperatorError('kernel_lock_unavailable')));
        child.once('exit', code => resolve(code));
      });
      check(code === 0, code === 1 ? 'refresh_maintainer_busy' : 'kernel_lock_unavailable');
      await this.checkBinding(directory); await this.verifyOwned(directory, 'refresh.lock', owned);
      this.bound = directory;
      return await work();
    } finally { this.bound = null; await lock?.close(); await directory.close(); }
  }
}
export class AccessFiles extends StateFiles {
  path(name) { check(['session.json', 'status.json'].includes(name)); return join(this.directory, name); }
}

export class RefreshMaintainer {
  constructor(policy, { transport, now = () => Date.now(), files = new RefreshFiles(policy.state_directory), output = new AccessFiles(policy.output_directory) } = {}) {
    this.policy = validateRefreshPolicy(policy); this.transport = transport; this.now = now; this.files = files; this.output = output;
  }
  seconds() { return Math.floor(this.now() / 1000); }
  async state() {
    const state = await this.files.read('journal.json');
    check(state, 'enrolled_active_account_required');
    validateEnrolledJournal(state, this.policy.enrollment_identity_sha256);
    check((state.phase === 'active' || (state.phase === 'refresh_pending' && state.resume_phase === 'active'))
      && state.signing_key === this.policy.signing_key && state.session_generation > 0, 'enrolled_active_account_required');
    check(hash(await this.files.bytes('did.json')) === this.policy.did_document_sha256, 'enrolled_identity_changed');
    return state;
  }
  async preview() {
    await this.files.validate(); await this.output.validate(); const state = await this.state();
    const review = { purpose: PURPOSE, policy_sha256: digest(this.policy), repository: this.policy.repository,
      pds: this.policy.pds_origin, signing_key: this.policy.signing_key, enrolled_identity_sha256: this.policy.enrollment_identity_sha256,
      initial_journal_sha256: digest(state), session_generation: state.session_generation, not_before: this.policy.not_before,
      defaults: REFRESH_DEFAULTS, operations: ['com.atproto.server.refreshSession', 'com.atproto.server.getSession'],
      automatic_enrollment: false, password_fallback: false, automatic_policy_extension: false };
    return { ...review, review_sha256: digest(review), executed: false };
  }
  async authorize(confirmation) {
    return this.files.kernelLocked(() => this.files.withDirectory(async directory => {
      // Use a distinct StateFiles binding for the legacy adoption lock. Every
      // enrollment execute checks the marker while it owns this same lock.
      const legacy = new StateFiles(this.policy.state_directory);
      await this.files.checkBinding(directory);
      return legacy.locked(async () => {
        const review = await this.preview();
        check(review.review_sha256 === confirmation, 'exact_review_digest_required');
        check(this.seconds() >= this.policy.not_before, 'refresh_policy_not_active');
        const expected = { schema_version: 1, purpose: PURPOSE, policy_sha256: digest(this.policy) };
        const existing = await this.files.read('refresh-owner.json');
        check(!existing || canonical(existing) === canonical(expected), 'refresh_owner_changed');
        if (!existing) await this.files.write('refresh-owner.json', expected);
        return { authorized: true, policy_sha256: digest(this.policy), repository: this.policy.repository, network_calls: 0 };
      });
    }));
  }
  async owner() {
    check(this.seconds() >= this.policy.not_before, 'refresh_policy_not_active');
    const owner = await this.files.read('refresh-owner.json');
    check(canonical(owner) === canonical({ schema_version: 1, purpose: PURPOSE, policy_sha256: digest(this.policy) }), 'refresh_policy_not_authorized');
    check(!await this.files.bytes('operator.lock'), 'enrollment_lock_requires_manual_recovery');
  }
  async session(state) {
    const session = await this.files.read('session.json');
    strict(session, ['schema_version', 'identity_sha256', 'generation', 'did', 'handle', 'access_jwt', 'refresh_jwt', 'access_expires_at', 'refresh_expires_at']);
    check(session.schema_version === 1 && session.identity_sha256 === this.policy.enrollment_identity_sha256
      && Number.isSafeInteger(session.generation) && (session.generation === state.session_generation
        || (state.phase === 'refresh_pending' && session.generation === state.session_generation + 1))
      && session.did === this.policy.repository && session.handle === this.policy.handle
      && session.access_expires_at === this.token(session.access_jwt, 'access')
      && session.refresh_expires_at === this.token(session.refresh_jwt, 'refresh'), 'session_binding_mismatch');
    return session;
  }
  token(value, kind) { return EnrollmentOperator.prototype.token.call({ config: this.policy }, value, kind); }
  async maintenance() {
    const state = await this.files.read('maintenance.json');
    if (!state) return { schema_version: 1, policy_sha256: digest(this.policy), state: 'healthy', attempts: 0,
      generation: 0, last_success: null, retry_at: null, reason: null, tls_expires_at: null, recovery_generation: 0 };
    strict(state, ['schema_version', 'policy_sha256', 'state', 'attempts', 'generation', 'last_success', 'retry_at', 'reason', 'tls_expires_at', 'recovery_generation']);
    check(state.schema_version === 1 && state.policy_sha256 === digest(this.policy) && STATES.includes(state.state)
      && Number.isSafeInteger(state.attempts) && state.attempts >= 0 && state.attempts <= REFRESH_DEFAULTS.max_attempts
      && Number.isSafeInteger(state.generation) && state.generation >= 0
      && Number.isSafeInteger(state.recovery_generation) && state.recovery_generation >= 0 && state.recovery_generation <= 9999
      && [state.last_success, state.retry_at, state.tls_expires_at].every(n => n === null || (Number.isSafeInteger(n) && n > 0))
      && (state.reason === null || /^[a-z][a-z0-9_]{0,79}$/u.test(state.reason)), 'maintenance_binding_mismatch');
    return state;
  }
  async recoveryPreview() {
    await this.owner(); const state = await this.state(); const maintenance = await this.maintenance();
    check(maintenance.state === 'intervention' && maintenance.recovery_generation < 9999, 'intervention_recovery_required');
    const review = { purpose: 'memory-private-pds-refresh-recovery-v1', policy_sha256: digest(this.policy),
      repository: this.policy.repository, journal_sha256: digest(state), maintenance_sha256: digest(maintenance),
      recovery_generation: maintenance.recovery_generation + 1, prior_attempts: maintenance.attempts, prior_reason: maintenance.reason,
      bounded_attempts: REFRESH_DEFAULTS.max_attempts, changes_identity_or_grace_window: false };
    return { ...review, review_sha256: digest(review), executed: false };
  }
  async recover(confirmation) {
    return this.files.kernelLocked(async () => {
      const review = await this.recoveryPreview();
      check(review.review_sha256 === confirmation, 'exact_review_digest_required');
      const receipt = { ...review, executed: true };
      const name = `recovery-${review.recovery_generation}.json`;
      const existing = await this.files.read(name);
      check(!existing || canonical(existing) === canonical(receipt), 'recovery_receipt_changed');
      // Immutable receipt first; a crash cannot erase the failed generation.
      if (!existing) await this.files.write(name, receipt);
      const maintenance = { ...await this.maintenance(), state: 'retry_wait', attempts: 0, reason: 'operator_reviewed_recovery',
        retry_at: this.seconds() + 60, recovery_generation: review.recovery_generation };
      await this.files.write('maintenance.json', maintenance); await this.status(maintenance);
      return { recovered: true, repository: this.policy.repository, recovery_generation: maintenance.recovery_generation,
        network_calls: 0, identities_changed: false, next_check_at: maintenance.retry_at };
    });
  }
  async tlsRecoveryPreview() {
    await this.owner(); const state = await this.state(); const maintenance = await this.maintenance();
    check(maintenance.state === 'intervention' && state.phase === 'refresh_pending'
      && state.resume_phase === 'active' && maintenance.recovery_generation < 9999,
    'tls_intervention_recovery_required');
    let evidence = null;
    if (maintenance.reason === 'tls_peer_verification_failed') {
      evidence = { kind: 'maintenance', sha256: digest(maintenance) };
    } else if (maintenance.reason === 'refresh_reconciliation_window_expired' && maintenance.recovery_generation > 0) {
      const prior = await this.files.read(`recovery-${maintenance.recovery_generation}.json`);
      if (prior?.executed === true && prior.prior_reason === 'tls_peer_verification_failed'
        && prior.repository === this.policy.repository && prior.policy_sha256 === digest(this.policy)
        && prior.journal_sha256 === digest(state)
        && prior.recovery_generation === maintenance.recovery_generation) {
        evidence = { kind: 'reviewed_recovery_receipt', sha256: digest(prior) };
      }
    }
    // A TLS peer-certificate verification failure happens during the handshake,
    // before Node can transmit the HTTP refresh request. Only that pre-HTTP
    // failure may retire an old pending intent outside the ordinary grace window.
    check(evidence, 'tls_pre_http_failure_evidence_required');
    const resolved = { ...state, revision: state.revision + 1, phase: 'active', resume_phase: null,
      pending_started_at: null, resume_pending_started_at: null };
    const review = { purpose: 'memory-private-pds-tls-refresh-recovery-v1', policy_sha256: digest(this.policy),
      repository: this.policy.repository, journal_sha256: digest(state), resolved_journal_sha256: digest(resolved),
      maintenance_sha256: digest(maintenance), evidence,
      recovery_generation: maintenance.recovery_generation + 1, prior_reason: maintenance.reason,
      network_calls: 0, changes_identity: false, password_fallback: false,
      retires_pre_http_pending_intent: true, next_attempt_may_call_refresh_session: true };
    return { ...review, review_sha256: digest(review), executed: false };
  }
  async recoverTls(confirmation) {
    return this.files.kernelLocked(async () => {
      let maintenance = await this.maintenance();
      const generation = maintenance.recovery_generation + 1;
      const name = `tls-recovery-${generation}.json`;
      let receipt = await this.files.read(name);
      if (!receipt) {
        const review = await this.tlsRecoveryPreview();
        check(review.review_sha256 === confirmation, 'exact_review_digest_required');
        receipt = { ...review, executed: true };
        await this.files.write(name, receipt);
      } else {
        check(receipt.executed === true && receipt.review_sha256 === confirmation
          && receipt.policy_sha256 === digest(this.policy) && receipt.repository === this.policy.repository
          && receipt.recovery_generation === generation, 'tls_recovery_receipt_changed');
      }
      let state = await this.state();
      if (digest(state) === receipt.journal_sha256) {
        state = { ...state, revision: state.revision + 1, phase: 'active', resume_phase: null,
          pending_started_at: null, resume_pending_started_at: null };
        check(digest(state) === receipt.resolved_journal_sha256, 'tls_recovery_receipt_changed');
        await this.files.write('journal.json', state);
      } else {
        check(digest(state) === receipt.resolved_journal_sha256, 'tls_recovery_receipt_changed');
      }
      maintenance = { ...maintenance, state: 'retry_wait', attempts: 0, reason: 'operator_reviewed_tls_recovery',
        retry_at: this.seconds() + 60, recovery_generation: generation };
      await this.files.write('maintenance.json', maintenance); await this.status(maintenance);
      return { recovered: true, repository: this.policy.repository, recovery_generation: generation,
        network_calls: 0, identities_changed: false, pending_intent_retired: true,
        next_check_at: maintenance.retry_at };
    });
  }
  async status(maintenance) {
    const result = { schema_version: 1, repository: this.policy.repository, pds: this.policy.pds_origin,
      observed_at: this.seconds(), last_success: maintenance.last_success, retry_at: maintenance.retry_at,
      state: maintenance.state, reason: maintenance.reason, generation: maintenance.generation, tls_expires_at: maintenance.tls_expires_at };
    await this.output.write('status.json', result);
    return result;
  }
  async bundle(session, maintenance) {
    check(session.access_expires_at > this.now() + 30000, 'access_credential_expired');
    const previous = await this.output.read('session.json');
    if (previous) {
      strict(previous, ['schema_version', 'repository', 'pds', 'generation', 'access_token', 'issued_at', 'expires_at']);
      check(previous.schema_version === 1 && previous.repository === this.policy.repository && previous.pds === this.policy.pds_origin
        && Number.isSafeInteger(previous.generation) && previous.generation > 0 && previous.generation <= session.generation,
      'access_bundle_binding_mismatch');
      if (previous.generation === session.generation) {
        check(previous.access_token === session.access_jwt && previous.expires_at === session.access_expires_at / 1000
          && Number.isSafeInteger(previous.issued_at) && previous.issued_at <= this.seconds(), 'access_bundle_binding_mismatch');
        return;
      }
    }
    await this.output.write('session.json', { schema_version: 1, repository: this.policy.repository, pds: this.policy.pds_origin,
      generation: session.generation, access_token: session.access_jwt, issued_at: maintenance.last_success ?? this.seconds(),
      expires_at: session.access_expires_at / 1000 });
  }
  async rpc(nsid, token, ca) {
    check(['com.atproto.server.refreshSession', 'com.atproto.server.getSession'].includes(nsid), 'refresh_only_operation_required');
    await this.owner();
    return this.transport.rpc(this.policy.operator_origin, nsid,
      { ...(nsid.endsWith('refreshSession') ? { method: 'POST' } : {}), token }, ca);
  }
  async tick() {
    return this.files.kernelLocked(async () => {
      await this.owner(); await this.output.validate();
      let maintenance = await this.maintenance();
      const save = async updates => {
        maintenance = { ...maintenance, ...updates,
          tls_expires_at: this.transport?.peerCertificateExpiresAt ?? maintenance.tls_expires_at };
        await this.files.write('maintenance.json', maintenance);
      };
      try {
        const ca = await privateRead(this.policy.tls_ca_file, { publicFile: true, limit: 16384 });
        check(hash(ca) === this.policy.tls_ca_sha256, 'tls_trust_changed');
        let caExpires;
        try { caExpires = Math.floor(Date.parse(new X509Certificate(ca).validTo) / 1000); }
        catch { throw new OperatorError('invalid_tls_certificate'); }
        check(caExpires > this.seconds(), 'tls_certificate_expired');
        let state = await this.state(); let session = await this.session(state);
        if (maintenance.state === 'intervention') return this.status(maintenance);
        if (maintenance.retry_at !== null && maintenance.retry_at > this.seconds()) return this.status(maintenance);
        const saved = state.phase === 'refresh_pending' && session.generation === state.session_generation + 1;
        const due = state.phase === 'refresh_pending' || session.access_expires_at <= this.now() + 900000;
        if (due) {
          check(maintenance.attempts < REFRESH_DEFAULTS.max_attempts, 'refresh_retries_exhausted');
          if (!saved) {
            check(session.refresh_expires_at > this.now() + 30000, 'refresh_credential_expired');
            if (state.phase === 'refresh_pending') check(this.now() < state.pending_started_at + 7200000, 'refresh_reconciliation_window_expired');
          }
          const attempts = maintenance.attempts + 1;
          await save({ attempts, state: 'refreshing', reason: null, generation: session.generation,
            // A killed process may have left an in-flight request. A new process
            // waits the bounded transport deadline and backoff before reconciliation.
            retry_at: this.seconds() + 30 + Math.min(600, 10 * 2 ** (attempts - 1)) });
          await this.status(maintenance);
          if (saved) {
            if (session.access_expires_at > this.now() + 30000) {
              const response = await this.rpc('com.atproto.server.getSession', session.access_jwt, ca);
              check(response?.did === this.policy.repository && response.handle === this.policy.handle && response.active === true, 'session_identity_mismatch');
            }
          } else {
            if (state.phase === 'active') {
              state = { ...state, revision: state.revision + 1, phase: 'refresh_pending', resume_phase: 'active',
                pending_started_at: this.now(), resume_pending_started_at: null };
              await this.files.write('journal.json', state);
            }
            const response = await this.rpc('com.atproto.server.refreshSession', session.refresh_jwt, ca);
            check(response?.did === this.policy.repository && response.handle === this.policy.handle && response.active !== false, 'session_identity_mismatch');
            session = { schema_version: 1, identity_sha256: this.policy.enrollment_identity_sha256, generation: state.session_generation + 1,
              did: response.did, handle: response.handle, access_jwt: response.accessJwt, refresh_jwt: response.refreshJwt,
              access_expires_at: this.token(response.accessJwt, 'access'), refresh_expires_at: this.token(response.refreshJwt, 'refresh') };
            check(session.access_expires_at > this.now() + 30000 && session.refresh_expires_at > this.now() + 30000, 'invalid_session_response');
            await this.files.write('session.json', session);
          }
          state = { ...state, revision: state.revision + 1, phase: 'active', session_generation: session.generation,
            resume_phase: null, pending_started_at: null, resume_pending_started_at: null };
          await this.files.write('journal.json', state);
          // Keep this attempt open until access publication succeeds. A local
          // bundle failure must not reset the bounded recovery counter.
          await save({ state: 'refreshing', reason: null, last_success: this.seconds(), generation: session.generation });
        } else if (maintenance.state !== 'healthy') {
          check(maintenance.attempts < REFRESH_DEFAULTS.max_attempts, 'refresh_retries_exhausted');
          await save({ state: 'refreshing', attempts: maintenance.attempts + 1, generation: session.generation,
            reason: null, retry_at: this.seconds() + 30 });
        } else if (maintenance.generation !== session.generation) {
          await save({ generation: session.generation });
        }
        // A persisted refresh can be reconciled after its access token expires.
        // Complete local state first; the next poll performs a new bounded refresh.
        if (session.access_expires_at <= this.now() + 30000) {
          await save({ state: 'retry_wait', reason: 'access_credential_expired', retry_at: this.seconds() + 60 });
        } else {
          await this.bundle(session, maintenance);
          if (maintenance.state !== 'healthy') await save({ state: 'healthy', attempts: 0, reason: null, retry_at: null });
        }
        return await this.status(maintenance);
      } catch (error) {
        const reason = error instanceof OperatorError ? error.code : 'local_persistence_unconfirmed';
        const attempts = Math.max(1, maintenance.attempts);
        const retry = TRANSIENT.has(reason) && attempts < REFRESH_DEFAULTS.max_attempts;
        await save({ state: retry ? 'retry_wait' : 'intervention', attempts, reason: retry ? reason
          : (TRANSIENT.has(reason) ? 'refresh_retries_exhausted' : reason),
        retry_at: retry ? this.seconds() + Math.min(600, 10 * 2 ** Math.max(0, maintenance.attempts - 1)) : null });
        return this.status(maintenance);
      }
    });
  }
}
