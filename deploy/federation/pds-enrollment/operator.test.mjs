// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, chmod, readFile, writeFile, rm, symlink, link, mkdir, rename, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { canonical, hash, OperatorError, parseJson, privateRead, safeFile, StateFiles } from './files.mjs';
import { EnrollmentOperator, loadConfig, PDS_IMAGE, validateConfig } from './operator.mjs';
import { HttpsTransport, jsonMediaType } from './transport.mjs';

const bootstrapKey = `did:key:z${'3'.repeat(45)}`;
const signingKey = `did:key:z${'4'.repeat(45)}`;
const instant = Date.parse('2026-09-19T14:00:00Z');
function jwt(config, scope, expires, serial = 0) {
  const encode = value => Buffer.from(JSON.stringify(value)).toString('base64url');
  return `${encode({ typ: 'JWT', alg: 'HS256' })}.${encode({ sub: config.repository, aud: config.service_did,
    scope: `com.atproto.${scope}`, exp: Math.floor(expires / 1000), jti: String(serial) })}.syntheticSignature`;
}

async function fixture(t, options = {}) {
  // The Linux operator deliberately rejects permissive non-root-owned TMPDIR
  // ancestors. Tests use the root-owned sticky /tmp, never chmod shared paths.
  const directory = await mkdtemp('/tmp/codifix-pds-operator-');
  await chmod(directory, 0o700);
  t.after(async () => { await rm(directory, { recursive: true, force: true }); });
  const write = (name, contents) => writeFile(join(directory, name), contents, { mode: 0o600 });
  const ca = Buffer.from('FAKE CA ONLY, no TLS connection is ever made');
  await write('ca.pem', ca); await write('bootstrap.key', 'a'.repeat(64));
  await write('password', 'synthetic-private-account-password'.repeat(2)); await write('invite', 'synthetic-once-invite');
  const config = { schema_version: 1, image: PDS_IMAGE, operation_id: 'fdb89f7c-1bdb-4d70-9722-54fca59262f1',
    repository: 'did:web:space.company.test', handle: 'space.company.test', email: 'private-owner@company.test',
    pds_origin: 'https://pds.company.test', operator_origin: 'https://operator.company.test:8443', service_did: 'did:web:pds.company.test',
    tls_ca_file: join(directory, 'ca.pem'), tls_ca_sha256: hash(ca), bootstrap_public_key: bootstrapKey,
    bootstrap_key_file: join(directory, 'bootstrap.key'), password_file: join(directory, 'password'), invite_file: join(directory, 'invite'),
    state_directory: directory, review_not_before: '2026-09-19T13:59:00Z', review_expires_at: '2026-09-20T13:00:00Z' };
  await write('config.json', JSON.stringify(config));
  let clock = instant;
  const calls = []; const fake = { doc: null, account: false, active: false, serial: 0, fail: null, refreshTokens: [], responseMutator: value => value };
  function response() {
    fake.serial++;
    const result = fake.responseMutator({ did: config.repository, handle: config.handle,
      accessJwt: jwt(config, 'access', clock + 7200000, fake.serial), refreshJwt: jwt(config, 'refresh', clock + 90 * 86400000, fake.serial) });
    fake.refreshTokens.push(result.refreshJwt); fake.refreshTokens = fake.refreshTokens.slice(-2); return result;
  }
  const transport = {
    async document(url) { calls.push({ op: 'document', origin: new URL(url).origin }); return fake.doc; },
    async rpc(origin, nsid, input) {
      calls.push({ op: nsid, origin });
      assert.equal(origin, config.operator_origin);
      const operation = nsid.split('.').at(-1);
      if (operation === 'createAccount') {
        assert.equal(input.body.did, config.repository); assert.equal(input.body.handle, config.handle);
        assert.equal(fake.account, false); fake.account = true;
        if (fake.fail === operation) { fake.fail = null; throw new Error('SENSITIVE remote response body'); }
        return response();
      }
      if (operation === 'createSession') {
        assert.equal(input.body.identifier, config.repository); assert.equal(fake.account, true);
        assert.equal(input.body.password, 'synthetic-private-account-password'.repeat(2));
        return response();
      }
      if (operation === 'getSession') return fake.responseMutator({ did: config.repository, handle: config.handle, active: fake.active });
      if (operation === 'getRecommendedDidCredentials') return fake.responseMutator({ verificationMethods: { atproto: signingKey },
        alsoKnownAs: [`at://${config.handle}`], services: { atproto_pds: { type: 'AtprotoPersonalDataServer', endpoint: config.pds_origin } } });
      if (operation === 'activateAccount') {
        fake.active = true;
        if (fake.fail === operation) { fake.fail = null; throw new Error('SENSITIVE activation failure'); }
        return {};
      }
      if (operation === 'refreshSession') {
        assert.ok(fake.refreshTokens.includes(input.token));
        const result = response();
        if (fake.fail === operation) { fake.fail = null; throw new Error('SENSITIVE ambiguous refresh'); }
        return result;
      }
      assert.fail('Unknown synthetic operation');
    },
  };
  const files = new StateFiles(directory, options.fault);
  const operator = new EnrollmentOperator(config, { files, transport, now: () => clock, signer: {
    publicKey: async key => { assert.equal(key, 'a'.repeat(64)); return bootstrapKey; },
    validPublicKey: async key => key === signingKey,
    serviceJwt: async () => 'synthetic-bound-bootstrap-service-jwt',
  } });
  const run = async phase => operator.execute(phase, (await operator.preview(phase)).review_sha256);
  const hostDocument = async () => { fake.doc = await files.read('did.json'); };
  async function ready(until = 'active') {
    await run('bootstrap'); await hostDocument();
    if (until === 'bootstrap') return;
    await run('create'); if (until === 'created') return;
    await run('discover-key'); if (until === 'key_review') return;
    await run('handoff'); await hostDocument(); if (until === 'handoff') return;
    await run('activate');
  }
  return { directory, config, operator, files, calls, fake, write, run, ready, hostDocument, clock: value => { clock = value; } };
}
const count = (f, suffix) => f.calls.filter(c => c.op.endsWith(suffix)).length;

test('default review uses no secrets, SDK, writes or transport and emits no private paths/email', async t => {
  const f = await fixture(t);
  await rm(f.config.bootstrap_key_file); await rm(f.config.password_file); await rm(f.config.invite_file);
  const preview = await f.operator.preview();
  assert.equal(preview.phase, 'bootstrap'); assert.equal(preview.executed, false); assert.equal(f.calls.length, 0);
  assert.equal(await f.files.read('journal.json'), null);
  const serialized = JSON.stringify(preview);
  for (const secret of [f.directory, f.config.email, 'password', 'invite', 'accessJwt']) assert.ok(!serialized.includes(secret));
  assert.equal(preview.did_document.id, f.config.repository);
  const config = await loadConfig(join(f.directory, 'config.json'));
  assert.deepEqual(config, f.config);
  assert.equal((await f.operator.preview()).review_sha256, preview.review_sha256);
});

test('strict configuration rejects duplicate keys, unsafe endpoints, path aliases and unknown semantics', async t => {
  const f = await fixture(t);
  assert.throws(() => parseJson('{"a":1,"a":2}'), /duplicate_json_key/u);
  assert.throws(() => parseJson('{"a":{"x":1,"x":2}}'), /duplicate_json_key/u);
  for (const change of [{ operator_origin: 'http://operator.company.test' }, { pds_origin: 'https://pds.company.test/path' },
    { operator_origin: 'https://secret@operator.company.test' }, { image: 'latest' }, { admin_password: 'no' },
    { service_did: 'did:web:other.company.test' }, { password_file: f.config.invite_file }, { password_file: join(f.directory, 'did.json') },
    { review_expires_at: '2026-02-30T00:00:00Z' }]) {
    assert.throws(() => validateConfig({ ...f.config, ...change }));
  }
  await chmod(join(f.directory, 'config.json'), 0o644);
  await assert.rejects(loadConfig(join(f.directory, 'config.json')), /unsafe_private_file/u);
  await symlink(f.config.password_file, join(f.directory, 'linked.json'));
  await assert.rejects(loadConfig(join(f.directory, 'linked.json')));
});

test('foreign owners, hard-linked secrets and permissive ancestors are rejected', async t => {
  const f = await fixture(t);
  await link(f.config.password_file, join(f.directory, 'password-hardlink'));
  await assert.rejects(privateRead(f.config.password_file), /unsafe_private_file/u);
  const meta = { isFile: () => true, nlink: 1, size: 32, mode: 0o100600, uid: process.geteuid() + 1 };
  assert.throws(() => safeFile(meta, 65536, false), /unsafe_private_file/u);
  const permissive = join(f.directory, 'permissive'); await mkdir(permissive, { mode: 0o700 }); await chmod(permissive, 0o777);
  const inner = join(permissive, 'state'); await mkdir(inner, { mode: 0o700 });
  await assert.rejects(new StateFiles(inner).validate(), /unsafe_parent_directory/u);
  await symlink(f.directory, join(f.directory, 'parent-alias'));
  await assert.rejects(privateRead(join(f.directory, 'parent-alias', 'password')));
});

test('a FIFO is rejected promptly before any blocking read', async t => {
  const f = await fixture(t); const fifo = join(f.directory, 'fifo');
  assert.equal(spawnSync('mkfifo', ['-m', '600', fifo]).status, 0);
  const module = new URL('./files.mjs', import.meta.url).href;
  const program = `import {privateRead} from ${JSON.stringify(module)}; try { await privateRead(${JSON.stringify(fifo)}); process.exitCode=2; } catch { process.stdout.write('rejected'); }`;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', program], { encoding: 'utf8', timeout: 2000 });
  assert.equal(result.status, 0); assert.equal(result.stdout, 'rejected');
});

test('oversized or growing-file inputs remain bounded and cannot advance the journal', async t => {
  const f = await fixture(t);
  await f.write('oversized', Buffer.alloc(65537));
  await assert.rejects(privateRead(join(f.directory, 'oversized')), /unsafe_private_file/u);
  await assert.rejects(f.files.write('journal.json', { data: 'x'.repeat(65536) }), /invalid_private_file_bound/u);
  assert.equal(await f.files.read('journal.json'), null);
  // Compete with bounded reads; every outcome is either one complete allowed
  // small snapshot or a refusal, never a returned oversized allocation.
  const path = join(f.directory, 'growing'); await f.write('growing', 'x');
  const reads = Array.from({ length: 40 }, async () => {
    try { const bytes = await privateRead(path, { limit: 32 }); assert.ok(bytes.length <= 32); }
    catch (error) { assert.ok(error instanceof OperatorError); }
  });
  await writeFile(path, Buffer.alloc(1024 * 1024), { mode: 0o600 });
  await Promise.all(reads);
});

test('directory replacement cannot redirect a write or its temporary cleanup', async t => {
  const f = await fixture(t); const moved = `${f.directory}-moved`;
  t.after(async () => rm(moved, { recursive: true, force: true }));
  let swapped = false;
  const files = new StateFiles(f.directory, async (stage, name) => {
    if (!swapped && stage === 'before_file_sync' && name === 'journal.json') {
      swapped = true; await rename(f.directory, moved); await mkdir(f.directory, { mode: 0o700 });
      await writeFile(join(f.directory, 'journal.json'), 'untouched replacement directory', { mode: 0o600 });
    }
  });
  await assert.rejects(files.write('journal.json', { sensitive: 'test-only' }), /state_directory_replaced/u);
  assert.equal(await readFile(join(f.directory, 'journal.json'), 'utf8'), 'untouched replacement directory');
  assert.equal((await readdir(moved)).some(name => name.startsWith('.pending-')), false);
});

test('symlink directory swap fails closed without writing through the alias', async t => {
  const f = await fixture(t); const victim = await fixture(t); const moved = `${f.directory}-moved`;
  t.after(async () => rm(moved, { recursive: true, force: true }));
  await victim.write('journal.json', 'unrelated protected file'); let swapped = false;
  const files = new StateFiles(f.directory, async stage => {
    if (!swapped && stage === 'before_file_sync') { swapped = true; await rename(f.directory, moved); await symlink(victim.directory, f.directory); }
  });
  await assert.rejects(files.write('journal.json', { new: 'do not redirect' }));
  assert.equal(await readFile(join(victim.directory, 'journal.json'), 'utf8'), 'unrelated protected file');
});

test('replaced temporary and lock inodes are never deleted as ours', async t => {
  const f = await fixture(t); let substituted = false;
  const files = new StateFiles(f.directory, async stage => {
    if (!substituted && stage === 'before_file_sync') {
      substituted = true; const temporary = (await readdir(f.directory)).find(name => name.startsWith('.pending-'));
      await rename(join(f.directory, temporary), join(f.directory, 'retained-own-temporary'));
      await f.write(temporary, 'unrelated replacement');
    }
  });
  await assert.rejects(files.write('journal.json', { private: 'synthetic' }), /owned_state_file_replaced/u);
  const temporary = (await readdir(f.directory)).find(name => name.startsWith('.pending-'));
  assert.equal(await readFile(join(f.directory, temporary), 'utf8'), 'unrelated replacement');
  await assert.rejects(f.files.locked(async () => {
    await rename(join(f.directory, 'operator.lock'), join(f.directory, 'original-lock'));
    await f.write('operator.lock', 'replacement lock');
  }), /owned_state_file_replaced/u);
  assert.equal(await readFile(join(f.directory, 'operator.lock'), 'utf8'), 'replacement lock');
});

test('wrong digest and expired review fail before effects', async t => {
  const f = await fixture(t);
  await assert.rejects(f.operator.execute('bootstrap', '0'.repeat(64)), /exact_review_digest_required/u);
  f.clock(instant + 86400000);
  await assert.rejects(f.run('bootstrap'), /review_expired/u);
  assert.equal(f.calls.length, 0); assert.equal(await f.files.read('did.json'), null);
});

test('bootstrap never overwrites unowned existing document or session files', async t => {
  const f = await fixture(t); await f.files.write('did.json', { preserve: 'not this operator document' });
  await assert.rejects(f.run('bootstrap'), /unowned_output_present/u);
  assert.deepEqual(await f.files.read('did.json'), { preserve: 'not this operator document' });
  assert.equal(await f.files.read('journal.json'), null);
});

test('separate signing-key review and exact hosted bytes gate activation; active never means gateway ready', async t => {
  const f = await fixture(t); await f.ready('key_review');
  const review = await f.operator.preview('handoff');
  assert.equal(review.did_document.verificationMethod[0].publicKeyMultibase, signingKey.slice(8));
  assert.equal((await f.files.read('did.json')).verificationMethod[0].publicKeyMultibase, bootstrapKey.slice(8));
  await f.run('handoff');
  assert.equal(review.did_document_sha256, hash(await readFile(join(f.directory, 'did.json'))));
  await assert.rejects(f.run('activate'), /served_did_document_mismatch/u);
  assert.equal(count(f, 'activateAccount'), 0);
  await f.hostDocument(); const result = await f.run('activate');
  assert.equal(result.state, 'active'); assert.equal(result.gateway_ready, false); assert.equal(result.repository_verified, false);
  assert.equal(count(f, 'createAccount'), 1); assert.equal(count(f, 'activateAccount'), 1);
  const serialized = JSON.stringify(result);
  for (const forbidden of ['accessJwt', 'refreshJwt', 'synthetic-private', f.directory, f.config.email]) assert.ok(!serialized.includes(forbidden));
});

test('lost create response keeps intent and reconciles exact identity; changed retry is denied', async t => {
  const f = await fixture(t); await f.ready('bootstrap'); f.fake.fail = 'createAccount';
  await assert.rejects(f.run('create')); assert.equal((await f.files.read('journal.json')).phase, 'create_pending');
  await assert.rejects(f.operator.preview('create'), /phase_requires_reconciliation/u);
  await f.write('password', 'changed-password-must-not-be-replayed'.repeat(2));
  await assert.rejects(f.run('reconcile'), /original_inputs_changed/u);
  assert.equal(count(f, 'createSession'), 0);
  await f.write('password', 'synthetic-private-account-password'.repeat(2));
  assert.equal((await f.run('reconcile')).state, 'created');
  assert.equal(count(f, 'createAccount'), 1); assert.equal(count(f, 'createSession'), 1);
  f.config.repository = 'did:web:wrong.company.test';
  await assert.rejects(f.operator.preview(), /journal_binding_mismatch/u);
});

test('wrong returned identity or JWT audience leaves creation unconfirmed and credentials unstored', async t => {
  for (const mutation of [value => ({ ...value, did: 'did:web:wrong.company.test' }), value => ({ ...value,
    accessJwt: jwt({ ...f.config, service_did: 'did:web:wrong.company.test' }, 'access', instant + 7200000) })]) {
    // Each test subfixture is independent; mutation executes only after binding f.
    var f = await fixture(t); await f.ready('bootstrap'); f.fake.responseMutator = mutation;
    await assert.rejects(f.run('create'), /session_identity_mismatch/u);
    assert.equal((await f.files.read('journal.json')).phase, 'create_pending'); assert.equal(await f.files.read('session.json'), null);
  }
});

test('recommended key discovery rejects unrelated handle, endpoint or malformed key', async t => {
  const f = await fixture(t); await f.ready('created');
  f.fake.responseMutator = response => response.verificationMethods ? { ...response,
    services: { atproto_pds: { type: 'AtprotoPersonalDataServer', endpoint: 'https://wrong.company.test' } } } : response;
  await assert.rejects(f.run('discover-key'), /recommended_identity_mismatch/u);
  assert.equal((await f.files.read('journal.json')).phase, 'created');
});

test('interrupted activation reconciles status without a second activation', async t => {
  const f = await fixture(t); await f.ready('handoff'); f.fake.fail = 'activateAccount';
  await assert.rejects(f.run('activate')); assert.equal((await f.files.read('journal.json')).phase, 'activate_pending');
  assert.equal((await f.run('reconcile')).state, 'active'); assert.equal(count(f, 'activateAccount'), 1);
});

test('refresh during pending activation preserves its timestamp through direct and interrupted recovery', async t => {
  for (const interrupted of [false, true]) {
    let interrupt = false;
    const f = await fixture(t, { fault: async (stage, name) => {
      if (interrupt && stage === 'after_atomic_write' && name === 'session.json') { interrupt = false; throw new Error('interrupted'); }
    } });
    await f.ready('handoff'); f.fake.fail = 'activateAccount'; await assert.rejects(f.run('activate'));
    const activationTime = (await f.files.read('journal.json')).pending_started_at;
    f.clock(instant + 121 * 60000); interrupt = interrupted;
    if (interrupted) { await assert.rejects(f.run('refresh')); await f.run('reconcile'); }
    else await f.run('refresh');
    const state = await f.files.read('journal.json');
    assert.equal(state.phase, 'activate_pending'); assert.equal(state.pending_started_at, activationTime);
    assert.equal((await f.operator.preview()).phase, 'reconcile');
    assert.equal((await f.run('reconcile')).state, 'active'); assert.equal(count(f, 'activateAccount'), 1);
  }
});

test('slow preflight cannot carry review authority past expiry into account creation or activation', async t => {
  const f = await fixture(t); await f.ready('bootstrap');
  const getDocument = f.operator.transport.document;
  f.operator.transport.document = async (...args) => { const doc = await getDocument(...args); f.clock(instant + 86400000); return doc; };
  await assert.rejects(f.run('create'), /review_expired/u); assert.equal(count(f, 'createAccount'), 0);
  const g = await fixture(t); await g.ready('handoff');
  const rpc = g.operator.transport.rpc;
  g.operator.transport.rpc = async (...args) => { const response = await rpc(...args); if (args[1].endsWith('getSession')) g.clock(instant + 86400000); return response; };
  await assert.rejects(g.run('activate'), /review_expired/u); assert.equal(count(g, 'activateAccount'), 0);
});

test('successful refresh persists both rotated tokens and journal generation together by recovery', async t => {
  let interrupt = false;
  const f = await fixture(t, { fault: async (stage, name) => {
    if (interrupt && stage === 'after_atomic_write' && name === 'session.json') { interrupt = false; throw new Error('simulated crash after session commit'); }
  } });
  await f.ready(); const old = await f.files.read('session.json'); interrupt = true;
  await assert.rejects(f.run('refresh'));
  assert.equal((await f.files.read('journal.json')).phase, 'refresh_pending');
  const rotated = await f.files.read('session.json'); assert.equal(rotated.generation, 2);
  assert.notEqual(rotated.access_jwt, old.access_jwt); assert.notEqual(rotated.refresh_jwt, old.refresh_jwt);
  const result = await f.run('reconcile'); assert.equal(result.state, 'active'); assert.equal(result.session_generation, 2);
  assert.equal(count(f, 'refreshSession'), 1);
});

test('partial session write preserves old credentials; explicit reconciliation retries retained refresh', async t => {
  let interrupt = false;
  const f = await fixture(t, { fault: async (stage, name) => {
    if (interrupt && stage === 'before_file_sync' && name === 'session.json') { interrupt = false; throw new Error('simulated incomplete file'); }
  } });
  await f.ready(); const old = await readFile(join(f.directory, 'session.json')); interrupt = true;
  await assert.rejects(f.run('refresh')); assert.deepEqual(await readFile(join(f.directory, 'session.json')), old);
  assert.equal((await f.run('reconcile')).session_generation, 2); assert.equal(count(f, 'refreshSession'), 2);
});

test('uncertain refresh remains explicit; expired access blocks reads while current refresh can recover', async t => {
  const f = await fixture(t); await f.ready('created'); f.clock(instant + 121 * 60000);
  const before = f.calls.length; await assert.rejects(f.run('discover-key'), /access_credential_expired/u); assert.equal(f.calls.length, before);
  await f.run('refresh'); await f.run('discover-key');
  f.fake.fail = 'refreshSession'; await assert.rejects(f.run('refresh'));
  assert.equal((await f.files.read('journal.json')).phase, 'refresh_pending');
  await f.run('reconcile'); assert.equal((await f.files.read('journal.json')).phase, 'key_review');
  f.clock(instant + 91 * 86400000); f.config.review_not_before = '2026-12-19T13:59:00Z'; f.config.review_expires_at = '2026-12-20T13:00:00Z';
  const prior = f.calls.length; await assert.rejects(f.run('refresh'), /refresh_credential_expired/u); assert.equal(f.calls.length, prior);
});

test('exclusive lock refuses another operator without network or credential read', async t => {
  const f = await fixture(t); const review = await f.operator.preview();
  await f.write('operator.lock', '{}');
  await assert.rejects(f.operator.execute('bootstrap', review.review_sha256), /operator_locked_manual_recovery_required/u);
  assert.equal(f.calls.length, 0);
});

test('adopted refresh-only state refuses every legacy enrollment execution', async t => {
  const f = await fixture(t); await f.ready(); const before = f.calls.length;
  await f.files.write('refresh-owner.json', { schema_version: 1, purpose: 'codifix-private-pds-refresh-only-v1', policy_sha256: 'c'.repeat(64) });
  await assert.rejects(f.run('refresh'), /refresh_only_maintenance_owns_session/u);
  assert.equal(f.calls.length, before);
});

test('ambiguous refresh past upstream grace period fails before another call', async t => {
  const f = await fixture(t); await f.ready(); f.fake.fail = 'refreshSession'; await assert.rejects(f.run('refresh'));
  f.clock(instant + 121 * 60000); const prior = f.calls.length;
  await assert.rejects(f.run('reconcile'), /refresh_reconciliation_window_expired/u);
  assert.equal(f.calls.length, prior); assert.equal((await f.files.read('journal.json')).phase, 'refresh_pending');
});

test('CLI preview and refusal work outside image, without leaking paths or secrets', async t => {
  const f = await fixture(t);
  const cli = fileURLToPath(new URL('./cli.mjs', import.meta.url));
  const preview = spawnSync(process.execPath, [cli, join(f.directory, 'config.json')], { encoding: 'utf8' });
  assert.equal(preview.status, 0, preview.stderr); assert.equal(JSON.parse(preview.stdout).executed, false);
  const denied = spawnSync(process.execPath, [cli, 'execute', join(f.directory, 'config.json'), 'bootstrap', '0'.repeat(64)], { encoding: 'utf8' });
  assert.equal(denied.status, 1); assert.equal(JSON.parse(denied.stderr).error, 'exact_review_digest_required');
  assert.ok(!denied.stderr.includes(f.directory)); assert.equal(denied.stdout, '');
  const insecure = spawnSync(process.execPath, [cli, '--insecure'], { encoding: 'utf8' });
  assert.equal(insecure.status, 1); assert.equal(insecure.stdout, '');
});

test('production transport rejects arbitrary methods and cleartext before sending', () => {
  const transport = new HttpsTransport();
  assert.throws(() => transport.rpc('https://operator.company.test', 'com.atproto.server.createInviteCode', { method: 'POST' }, Buffer.alloc(0)));
  assert.throws(() => transport.document('http://operator.company.test/.well-known/did.json', Buffer.alloc(0)), /unsafe_endpoint/u);
  for (const type of ['application/json', 'application/did+json', 'application/did+ld+json; charset=utf-8']) assert.ok(jsonMediaType(type));
  for (const type of ['text/html', 'application/anything+json', 'application/json-other', '']) assert.equal(jsonMediaType(type), false);
});
