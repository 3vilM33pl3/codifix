// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { chmod, mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { spawn, spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { canonical, digest, hash, OperatorError, StateFiles } from './files.mjs';
import { PDS_IMAGE } from './operator.mjs';
import { AccessFiles, loadRefreshPolicy, RefreshFiles, RefreshMaintainer, validateRefreshPolicy } from './refresh.mjs';

let certificate;
const instant = Math.floor(Date.now() / 1000) * 1000;
function jwt(policy, scope, expires, serial = 0) {
  const encode = value => Buffer.from(JSON.stringify(value)).toString('base64url');
  return `${encode({ typ: 'JWT', alg: 'HS256' })}.${encode({ sub: policy.repository, aud: policy.service_did,
    scope: `com.atproto.${scope}`, exp: Math.floor(expires / 1000), jti: String(serial) })}.syntheticSignature`;
}
async function fixture(t, options = {}) {
  const directory = await mkdtemp('/tmp/memory-pds-refresh-'); await chmod(directory, 0o700);
  t.after(() => rm(directory, { recursive: true, force: true }));
  const state = join(directory, 'state'); const output = join(directory, 'access');
  await mkdir(state, { mode: 0o700 }); await mkdir(output, { mode: 0o700 });
  if (!certificate) {
    const result = spawnSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '30',
      '-subj', '/CN=synthetic-refresh-only-test', '-keyout', join(directory, 'fake.key'), '-out', join(directory, 'generated.pem')], { stdio: 'ignore' });
    assert.equal(result.status, 0); certificate = await readFile(join(directory, 'generated.pem'));
  }
  await writeFile(join(directory, 'ca.pem'), certificate, { mode: 0o600 });
  const did = { id: 'did:web:space.company.test', reviewed: 'synthetic public identity fixture' };
  const policy = { schema_version: 1, purpose: 'memory-private-pds-refresh-only-v1', policy_id: 'fdb89f7c-1bdb-4d70-9722-54fca59262f1', image: PDS_IMAGE,
    enrollment_identity_sha256: 'a'.repeat(64), repository: 'did:web:space.company.test', handle: 'space.company.test',
    pds_origin: 'https://pds.company.test', operator_origin: 'https://operator.company.test:8443', service_did: 'did:web:pds.company.test',
    signing_key: `did:key:z${'4'.repeat(45)}`, did_document_sha256: hash(`${canonical(did)}\n`),
    tls_ca_file: join(directory, 'ca.pem'), tls_ca_sha256: hash(certificate), state_directory: state, output_directory: output,
    not_before: Math.floor(instant / 1000) - 60 };
  const files = new RefreshFiles(state, options.fault); const access = new AccessFiles(output, options.outputFault);
  let clock = instant; let serial = 1; const calls = []; let failure; let mutate = value => value;
  const transport = { async rpc(origin, nsid, input) {
    assert.equal(origin, policy.operator_origin); calls.push({ op: nsid, token: input.token });
    assert.ok(['com.atproto.server.refreshSession', 'com.atproto.server.getSession'].includes(nsid));
    if (failure === 'pause') await options.pause();
    if (nsid.endsWith('getSession')) return mutate({ did: policy.repository, handle: policy.handle, active: true });
    const response = mutate({ did: policy.repository, handle: policy.handle, active: true,
      accessJwt: jwt(policy, 'access', clock + 7200000, ++serial), refreshJwt: jwt(policy, 'refresh', clock + 90 * 86400000, serial) });
    if (failure && failure !== 'pause') throw new OperatorError(failure);
    return response;
  } };
  await files.write('did.json', did);
  await files.write('journal.json', { schema_version: 1, identity_sha256: policy.enrollment_identity_sha256, phase: 'active', revision: 6,
    session_generation: 1, signing_key: policy.signing_key, input_sha256: 'b'.repeat(64), resume_phase: null,
    pending_started_at: null, resume_pending_started_at: null });
  const accessExpires = clock + (options.valid ? 7200000 : -60000);
  await files.write('session.json', { schema_version: 1, identity_sha256: policy.enrollment_identity_sha256, generation: 1,
    did: policy.repository, handle: policy.handle, access_jwt: jwt(policy, 'access', accessExpires), refresh_jwt: jwt(policy, 'refresh', clock + 86400000),
    access_expires_at: accessExpires, refresh_expires_at: clock + 86400000 });
  const path = join(directory, 'policy.json'); await writeFile(path, JSON.stringify(policy), { mode: 0o600 });
  const make = () => new RefreshMaintainer(policy, { transport, files, output: access, now: () => clock });
  const maintainer = make(); const authorize = async () => maintainer.authorize((await maintainer.preview()).review_sha256);
  return { directory, policy, path, files, access, calls, maintainer, make, authorize,
    advance: seconds => { clock += seconds * 1000; }, failure: value => { failure = value; }, mutate: fn => { mutate = fn; } };
}

test('review is read-only, independent of old enrollment windows, credentials and SDK', async t => {
  const f = await fixture(t); await rm(f.files.path('session.json'));
  const before = await readdir(f.policy.state_directory);
  const review = await f.maintainer.preview();
  assert.equal(review.executed, false); assert.equal(f.calls.length, 0); assert.equal(review.password_fallback, false);
  assert.deepEqual(await readdir(f.policy.state_directory), before);
  for (const forbidden of [f.directory, 'access_jwt', 'refresh_jwt', 'password_file', 'bootstrap_key_file']) assert.ok(!JSON.stringify(review).includes(forbidden));
  assert.deepEqual(await loadRefreshPolicy(f.path), f.policy);
  assert.deepEqual(review.operations, ['com.atproto.server.refreshSession', 'com.atproto.server.getSession']);
});

test('strict policy refuses new privileges, insecure endpoints and private/output aliasing', async t => {
  const f = await fixture(t);
  for (const changes of [{ password_file: '/private/password' }, { image: 'latest' }, { pds_origin: 'http://pds.company.test' },
    { operator_origin: 'https://operator.company.test/path' }, { repository: 'did:web:other.company.test' },
    { service_did: 'did:web:other.company.test' }, { output_directory: f.policy.state_directory },
    { output_directory: join(f.policy.state_directory, 'gateway') }, { not_before: 0 }]) {
    assert.throws(() => validateRefreshPolicy({ ...f.policy, ...changes }));
  }
  await chmod(f.path, 0o644); await assert.rejects(loadRefreshPolicy(f.path), /unsafe_private_file/u);
});

test('exact approval adopts only this enrolled identity without network or session read', async t => {
  const f = await fixture(t);
  await assert.rejects(f.maintainer.tick(), /refresh_policy_not_authorized/u);
  await assert.rejects(f.maintainer.authorize('0'.repeat(64)), /exact_review_digest_required/u);
  const result = await f.authorize(); assert.equal(result.network_calls, 0); assert.equal(f.calls.length, 0);
  assert.equal((await f.files.read('refresh-owner.json')).policy_sha256, digest(f.policy));
  assert.equal(await f.files.read('operator.lock'), null);
  const changed = new RefreshMaintainer({ ...f.policy, operator_origin: 'https://other.company.test:8443' });
  await assert.rejects(changed.tick(), /refresh_policy_not_authorized/u);
});

test('expired access refreshes exactly once, publishes access-only bundle, and restart reuses it', async t => {
  const f = await fixture(t); await f.authorize();
  const status = await f.maintainer.tick(); assert.equal(status.state, 'healthy'); assert.equal(status.generation, 2);
  assert.equal(f.calls.length, 1); assert.equal((await f.files.read('journal.json')).phase, 'active');
  const session = await f.files.read('session.json'); const bundle = await f.access.read('session.json');
  assert.equal(bundle.access_token, session.access_jwt); assert.equal(bundle.generation, session.generation);
  assert.equal(bundle.expires_at * 1000, session.access_expires_at);
  assert.deepEqual(Object.keys(bundle).sort(), ['schema_version', 'repository', 'pds', 'generation', 'access_token', 'issued_at', 'expires_at'].sort());
  assert.ok(!JSON.stringify(bundle).includes(session.refresh_jwt)); assert.ok(!JSON.stringify(status).includes('Jwt'));
  assert.equal((await f.make().tick()).state, 'healthy'); assert.equal(f.calls.length, 1);
  f.advance(105 * 60); assert.equal((await f.make().tick()).generation, 3); assert.equal(f.calls.length, 2);
});

test('valid existing access is installed without a refresh call or fabricated refresh success', async t => {
  const f = await fixture(t, { valid: true }); await f.authorize();
  const status = await f.maintainer.tick(); assert.equal(status.state, 'healthy'); assert.equal(status.last_success, null);
  assert.equal(status.generation, 1); assert.equal(f.calls.length, 0);
  assert.equal((await f.access.read('session.json')).generation, 1);
});

test('ambiguous refresh preserves intent, waits, then reconciles retained token only within grace', async t => {
  const f = await fixture(t); await f.authorize(); const old = await f.files.read('session.json');
  f.failure('network_outcome_unconfirmed'); const first = await f.maintainer.tick();
  assert.equal(first.state, 'retry_wait'); assert.equal((await f.files.read('journal.json')).phase, 'refresh_pending');
  assert.equal((await f.files.read('session.json')).refresh_jwt, old.refresh_jwt);
  await f.make().tick(); assert.equal(f.calls.length, 1);
  f.failure(null); f.advance(10); const result = await f.make().tick();
  assert.equal(result.state, 'healthy'); assert.equal(f.calls[1].token, old.refresh_jwt); assert.equal(f.calls.length, 2);
});

test('ambiguous refresh older than two hours never repeats the provider operation', async t => {
  const f = await fixture(t); await f.authorize(); f.failure('network_outcome_unconfirmed'); await f.maintainer.tick();
  f.advance(7201); f.failure(null); const result = await f.make().tick();
  assert.equal(result.state, 'intervention'); assert.equal(result.reason, 'refresh_reconciliation_window_expired'); assert.equal(f.calls.length, 1);
});

test('five transient attempts exhaust durably; restart cannot reset attempts or expand budget', async t => {
  const f = await fixture(t); await f.authorize(); f.failure('remote_rate_limited');
  for (let n = 1; n <= 5; n++) {
    const status = await f.make().tick(); assert.equal(f.calls.length, n);
    assert.equal(status.state, n === 5 ? 'intervention' : 'retry_wait'); f.advance(600);
  }
  const result = await f.make().tick(); assert.equal(result.reason, 'refresh_retries_exhausted'); assert.equal(f.calls.length, 5);
  assert.equal((await f.files.read('maintenance.json')).attempts, 5);
});

test('explicit intervention recovery retains the failed generation and cannot extend remote grace', async t => {
  const f = await fixture(t); await f.authorize(); f.failure('credential_rejected'); await f.maintainer.tick();
  const review = await f.maintainer.recoveryPreview(); assert.equal(review.prior_reason, 'credential_rejected');
  await assert.rejects(f.maintainer.recover('0'.repeat(64)), /exact_review_digest_required/u);
  const result = await f.maintainer.recover(review.review_sha256); assert.equal(result.network_calls, 0);
  assert.equal((await f.files.read('recovery-1.json')).prior_reason, 'credential_rejected');
  await assert.rejects(f.maintainer.recover(review.review_sha256), /intervention_recovery_required/u);
  f.failure(null); f.advance(7201);
  assert.equal((await f.make().tick()).reason, 'refresh_reconciliation_window_expired'); assert.equal(f.calls.length, 1);
  assert.equal((await f.maintainer.recoveryPreview()).recovery_generation, 2);
});

test('reviewed TLS-handshake recovery retires only a proven pre-HTTP pending intent', async t => {
  const f = await fixture(t); await f.authorize(); f.failure('tls_peer_verification_failed');
  await f.maintainer.tick(); f.advance(7201);
  const ordinary = await f.maintainer.recoveryPreview();
  await f.maintainer.recover(ordinary.review_sha256); f.failure(null); f.advance(60);
  assert.equal((await f.make().tick()).reason, 'refresh_reconciliation_window_expired');
  assert.equal(f.calls.length, 1);
  const review = await f.maintainer.tlsRecoveryPreview();
  assert.equal(review.evidence.kind, 'reviewed_recovery_receipt');
  assert.equal(review.retires_pre_http_pending_intent, true);
  assert.equal(review.next_attempt_may_call_refresh_session, true);
  await assert.rejects(f.maintainer.recoverTls('0'.repeat(64)), /exact_review_digest_required/u);
  const recovered = await f.maintainer.recoverTls(review.review_sha256);
  assert.equal(recovered.network_calls, 0); assert.equal(recovered.pending_intent_retired, true);
  assert.equal((await f.files.read('journal.json')).phase, 'active');
  assert.equal((await f.files.read('tls-recovery-2.json')).evidence.kind, 'reviewed_recovery_receipt');
  f.advance(60); const status = await f.make().tick();
  assert.equal(status.state, 'healthy'); assert.equal(status.generation, 2);
  assert.equal(f.calls.length, 2);
});

test('TLS-handshake recovery is unavailable for an ambiguous HTTP outcome', async t => {
  const f = await fixture(t); await f.authorize(); f.failure('network_outcome_unconfirmed');
  await f.maintainer.tick(); f.advance(7201); f.failure(null); await f.make().tick();
  await assert.rejects(f.maintainer.tlsRecoveryPreview(), /tls_pre_http_failure_evidence_required/u);
  assert.equal((await f.files.read('journal.json')).phase, 'refresh_pending');
  assert.equal(f.calls.length, 1);
});

test('credential, configuration, TLS and identity failures require immediate intervention', async t => {
  for (const reason of ['credential_rejected', 'remote_request_rejected', 'tls_peer_verification_failed', 'invalid_response']) {
    const f = await fixture(t); await f.authorize(); f.failure(reason);
    const result = await f.maintainer.tick(); assert.equal(result.state, 'intervention'); assert.equal(result.reason, reason);
    f.advance(3600); await f.make().tick(); assert.equal(f.calls.length, 1);
  }
});

test('forged response attribution and audience cannot publish credentials', async t => {
  for (const changed of ['identity', 'audience']) {
    const f = await fixture(t); await f.authorize();
    f.mutate(response => changed === 'identity' ? { ...response, did: 'did:web:other.company.test' }
      : { ...response, accessJwt: jwt({ ...f.policy, service_did: 'did:web:other.company.test' }, 'access', instant + 7200000) });
    const status = await f.maintainer.tick(); assert.equal(status.state, 'intervention'); assert.equal(status.reason, 'session_identity_mismatch');
    assert.equal(await f.access.read('session.json'), null); assert.equal((await f.files.read('session.json')).generation, 1);
  }
});

test('saved session after interrupted journal commit is recovered without another refresh', async t => {
  let interrupted = false;
  const f = await fixture(t, { fault: async (stage, name) => {
    if (interrupted && stage === 'after_atomic_write' && name === 'session.json') { interrupted = false; throw new Error('simulated private persistence interruption'); }
  } });
  await f.authorize(); interrupted = true;
  assert.equal((await f.maintainer.tick()).state, 'retry_wait');
  assert.equal((await f.files.read('session.json')).generation, 2); assert.equal((await f.files.read('journal.json')).phase, 'refresh_pending');
  f.advance(10); const status = await f.make().tick(); assert.equal(status.state, 'healthy');
  assert.equal(f.calls.filter(call => call.op.endsWith('refreshSession')).length, 1);
  assert.equal(f.calls.filter(call => call.op.endsWith('getSession')).length, 1);
});

test('failed bundle persistence retains refreshed session and never invokes provider again', async t => {
  let interrupted = false;
  const f = await fixture(t, { outputFault: async (stage, name) => {
    if (interrupted && stage === 'before_file_sync' && name === 'session.json') { interrupted = false; throw new Error('synthetic bundle failure'); }
  } });
  await f.authorize(); interrupted = true;
  assert.equal((await f.maintainer.tick()).state, 'retry_wait'); assert.equal(f.calls.length, 1);
  f.advance(10); assert.equal((await f.make().tick()).state, 'healthy');
  assert.equal(f.calls.length, 1); assert.equal((await f.access.read('session.json')).generation, 2);
});

test('persistent local access publication failure also has five bounded attempts', async t => {
  let failing = false;
  const f = await fixture(t, { outputFault: async (stage, name) => {
    if (failing && stage === 'before_file_sync' && name === 'session.json') throw new Error('synthetic full filesystem');
  } });
  await f.authorize(); failing = true;
  for (let n = 1; n <= 5; n++) {
    const status = await f.make().tick(); assert.equal(status.state, n === 5 ? 'intervention' : 'retry_wait');
    assert.equal((await f.files.read('maintenance.json')).attempts, n); f.advance(600);
  }
  assert.equal(f.calls.length, 1); assert.equal((await f.make().tick()).reason, 'refresh_retries_exhausted');
});

test('same-generation token replacement and output symlinks fail closed', async t => {
  const f = await fixture(t, { valid: true }); await f.authorize(); await f.maintainer.tick();
  const bundle = await f.access.read('session.json'); await f.access.write('session.json', { ...bundle, access_token: 'substituted' });
  assert.equal((await f.maintainer.tick()).reason, 'access_bundle_binding_mismatch');
  const g = await fixture(t, { valid: true }); await g.authorize();
  await symlink(g.files.path('session.json'), g.access.path('session.json'));
  assert.equal((await g.maintainer.tick()).state, 'intervention'); assert.equal(g.calls.length, 0);
});

test('kernel lock excludes competing maintainers and a legacy lock is never silently deleted', async t => {
  const f = await fixture(t); await f.authorize();
  await f.files.kernelLocked(async () => {
    const other = new RefreshFiles(f.policy.state_directory);
    await assert.rejects(other.kernelLocked(async () => assert.fail('second lock entered')), /refresh_maintainer_busy/u);
  });
  await f.files.kernelLocked(async () => {});
  await new StateFiles(f.policy.state_directory).write('operator.lock', { process_id: 1 });
  await assert.rejects(f.maintainer.tick(), /enrollment_lock_requires_manual_recovery/u);
  assert.equal(f.calls.length, 0); assert.deepEqual(await f.files.read('operator.lock'), { process_id: 1 });
});

test('killing the lock owner releases the kernel lock without deleting a stale inode', async t => {
  const f = await fixture(t);
  const module = new URL('./refresh.mjs', import.meta.url).href;
  const program = `import {RefreshFiles} from ${JSON.stringify(module)}; await new RefreshFiles(${JSON.stringify(f.policy.state_directory)}).kernelLocked(async()=>{process.stdout.write('locked\\n');await new Promise(()=>{});});`;
  // Keep a timer alive so Node does not exit for unsettled top-level await.
  const child = spawn(process.execPath, ['--input-type=module', '-e', `setInterval(()=>{},1000);${program}`], { stdio: ['ignore', 'pipe', 'pipe'] });
  t.after(() => child.kill('SIGKILL'));
  await new Promise((resolve, reject) => { child.stdout.once('data', resolve); child.once('error', reject); child.once('exit', code => reject(new Error(`premature child exit ${code}`))); });
  await assert.rejects(f.files.kernelLocked(async () => {}), /refresh_maintainer_busy/u);
  const exited = new Promise(resolve => child.once('exit', resolve)); child.kill('SIGKILL'); await exited;
  await f.files.kernelLocked(async () => {});
});

test('refresh-only CLI reviews offline and sanitizes denied execution', async t => {
  const f = await fixture(t); const cli = fileURLToPath(new URL('./refresh-cli.mjs', import.meta.url));
  const preview = spawnSync(process.execPath, [cli, 'review', f.path], { encoding: 'utf8' });
  assert.equal(preview.status, 0, preview.stderr); assert.equal(JSON.parse(preview.stdout).executed, false);
  const denied = spawnSync(process.execPath, [cli, 'once', f.path], { encoding: 'utf8' });
  assert.equal(denied.status, 78); assert.equal(JSON.parse(denied.stderr).error, 'refresh_policy_not_authorized');
  assert.ok(!denied.stderr.includes(f.directory)); assert.equal(denied.stdout, '');
  const enrollment = spawnSync(process.execPath, [cli, 'create', f.path], { encoding: 'utf8' });
  assert.equal(enrollment.status, 78); assert.equal(JSON.parse(enrollment.stderr).error, 'invalid_arguments');
});
