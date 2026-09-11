// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs against the actual upstream image, using its independent AT SDK verifier.
const assert = require('node:assert/strict');
const { createRequire } = require('node:module');
const { readFileSync, readdirSync, writeFileSync, existsSync, chownSync } = require('node:fs');
const https = require('node:https');
const { randomBytes, randomUUID } = require('node:crypto');
const appRequire = createRequire('/app/index.ts');
const pdsRequire = createRequire(appRequire.resolve('@atproto/pds'));
const { Secp256k1Keypair } = pdsRequire('@atproto/crypto');
const { createServiceJwt } = pdsRequire('@atproto/xrpc-server');
const { verifyRepoCar } = pdsRequire('@atproto/repo');
const { Lexicons } = pdsRequire('@atproto/lexicon');

const did = 'did:web:space.federation.example.com';
const handle = 'space.federation.example.com';
const serviceDid = 'did:web:pds.federation.example.com';
let document;
const server = https.createServer({
  key: readFileSync('/gate-tls/key.pem'),
  cert: readFileSync('/gate-tls/cert.pem'),
}, (req, res) => {
  if (req.headers.host === handle && req.url === '/.well-known/did.json') {
    res.writeHead(200, { 'content-type': 'application/did+ld+json' });
    res.end(JSON.stringify(document));
  } else {
    res.writeHead(404); res.end();
  }
});

function setDocument(signingDid, endpoint = 'https://pds.federation.example.com') {
  document = {
    '@context': ['https://www.w3.org/ns/did/v1'], id: did,
    alsoKnownAs: [`at://${handle}`],
    verificationMethod: [{ id: `${did}#atproto`, type: 'Multikey', controller: did,
      publicKeyMultibase: signingDid.slice('did:key:'.length) }],
    service: [{ id: `${did}#atproto_pds`, type: 'AtprotoPersonalDataServer',
      serviceEndpoint: endpoint }],
  };
}

async function rpc(nsid, { body, token, query, admin = false, method,
  base = 'http://pds:2583', rawBody, contentType = 'application/vnd.ipld.car' } = {}) {
  const url = new URL(`/xrpc/${nsid}`, base);
  if (query) url.search = new URLSearchParams(query).toString();
  const headers = {};
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (rawBody !== undefined) headers['content-type'] = contentType;
  if (token) headers.authorization = `Bearer ${token}`;
  if (admin) headers.authorization = `Basic ${Buffer.from(`admin:${process.env.GATE_ADMIN_PASSWORD}`).toString('base64')}`;
  const res = await fetch(url, { method: method ?? (body === undefined ? 'GET' : 'POST'),
    headers, body: rawBody ?? (body === undefined ? undefined : JSON.stringify(body)),
    signal: AbortSignal.timeout(15_000), redirect: 'error' });
  const value = res.headers.get('content-type')?.includes('json')
    ? await res.json() : new Uint8Array(await res.arrayBuffer());
  if (!res.ok) throw new Error(`${nsid}: HTTP ${res.status}`);
  return value;
}

async function waitReady(base) {
  for (let i = 0; i < 60; i++) {
    try {
      await rpc('com.atproto.server.describeServer', { base });
      return;
    } catch (error) {
      if (i === 59) throw error;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
  }
}

async function main() {
  const initialKey = await Secp256k1Keypair.create({ exportable: true });
  setDocument(initialKey.did());
  await new Promise(resolve => server.listen(443, '0.0.0.0', resolve));
  await waitReady('http://pds:2583');
  console.log('PASS upstream production-mode PDS readiness');
  const invite = await rpc('com.atproto.server.createInviteCode', { admin: true, body: { useCount: 1 } });
  const jwt = await createServiceJwt({ iss: did, aud: serviceDid,
    lxm: 'com.atproto.server.createAccount', keypair: initialKey });
  const account = await rpc('com.atproto.server.createAccount', { token: jwt, body: {
    did, handle, email: 'gate@federation.example.com',
    password: randomBytes(24).toString('hex'), inviteCode: invite.code,
  } });
  assert.equal(account.did, did);
  console.log('PASS private did:web BYOD provisioning without PLC');
  const token = account.accessJwt;
  const credentials = await rpc('com.atproto.identity.getRecommendedDidCredentials', { token });
  const signingKey = credentials.verificationMethods.atproto;
  assert.notEqual(signingKey, initialKey.did());
  setDocument(signingKey);
  await rpc('com.atproto.server.activateAccount', { token, method: 'POST' });
  console.log('PASS DID signing-key handoff and account activation');
  const schemas = new Lexicons(readdirSync('/gate/lexicons').map(file =>
    JSON.parse(readFileSync(`/gate/lexicons/${file}`, 'utf8'))));
  const golden = JSON.parse(readFileSync('/gate/create_record_request.json', 'utf8'));
  const record = golden.record;
  schemas.assertValidRecord(record.$type, record);
  assert.throws(() => schemas.assertValidRecord(record.$type, { $type: record.$type }));
  console.log('PASS all six spike Lexicon definitions and actual Rust adapter request schema');
  const created = await rpc('com.atproto.repo.createRecord', { token, body: {
    repo: did, collection: record.$type, rkey: golden.rkey, record, validate: false,
  } });
  const bytes = await rpc('com.atproto.sync.getRepo', { query: { did } });
  const verified = await verifyRepoCar(bytes, did, signingKey);
  assert.ok(verified);
  await assert.rejects(verifyRepoCar(bytes, did, initialKey.did()));
  const read = await rpc('com.atproto.repo.getRecord', { query: {
    repo: did, collection: record.$type, rkey: created.uri.split('/').at(-1),
  } });
  assert.deepEqual(read.value, record);
  console.log('PASS signed CAR verification, wrong-key rejection and record readback');
  const recovery = 'http://pds-recovery:2583';
  const recoveryDid = 'did:web:recovery.federation.example.com';
  const recoveryInvite = await rpc('com.atproto.server.createInviteCode', {
    base: recovery, admin: true, body: { useCount: 1 },
  });
  const migrationAuth = await rpc('com.atproto.server.getServiceAuth', { token,
    query: { aud: recoveryDid, lxm: 'com.atproto.server.createAccount' },
  });
  const recoveryPassword = randomBytes(24).toString('hex');
  const migrated = await rpc('com.atproto.server.createAccount', { base: recovery,
    token: migrationAuth.token, body: { did, handle, email: 'gate@federation.example.com',
      password: recoveryPassword, inviteCode: recoveryInvite.code },
  });
  const recoveredCredentials = await rpc('com.atproto.identity.getRecommendedDidCredentials', {
    base: recovery, token: migrated.accessJwt,
  });
  const recoveredKey = recoveredCredentials.verificationMethods.atproto;
  assert.notEqual(recoveredKey, signingKey);
  await rpc('com.atproto.repo.importRepo', { base: recovery,
    token: migrated.accessJwt, method: 'POST', rawBody: bytes,
  });
  setDocument(recoveredKey, 'https://recovery.federation.example.com');
  await rpc('com.atproto.server.activateAccount', {
    base: recovery, token: migrated.accessJwt, method: 'POST',
  });
  await rpc('com.atproto.server.deactivateAccount', { token, body: {} });
  const blobBytes = Buffer.from('Synthetic Memory PDS disaster-recovery evidence only.');
  const uploaded = await rpc('com.atproto.repo.uploadBlob', { base: recovery,
    token: migrated.accessJwt, method: 'POST', rawBody: blobBytes,
    contentType: 'application/octet-stream' });
  const recoveryRecord = { ...record, recordId: randomUUID(), fixtureBlob: uploaded.blob };
  const recoveryRkey = randomUUID();
  await rpc('com.atproto.repo.createRecord', { base: recovery, token: migrated.accessJwt,
    body: { repo: did, collection: record.$type, rkey: recoveryRkey,
      record: recoveryRecord, validate: false },
  });
  const recoveredBytes = await rpc('com.atproto.sync.getRepo', { base: recovery, query: { did } });
  await verifyRepoCar(recoveredBytes, did, recoveredKey);
  await assert.rejects(verifyRepoCar(recoveredBytes, did, signingKey));
  const retained = await rpc('com.atproto.repo.getRecord', { base: recovery, query: {
    repo: did, collection: record.$type, rkey: created.uri.split('/').at(-1),
  } });
  assert.deepEqual(retained.value, record);
  console.log('PASS supported PDS migration rotates signing key and preserves signed repository records');
  const commit = await rpc('com.atproto.sync.getLatestCommit', { base: recovery, query: { did } });
  writeFileSync('/gate-state/recovery-state.json', JSON.stringify({ did, handle, document,
    signingKey: recoveredKey, wrongKey: signingKey, password: recoveryPassword,
    accessJwt: migrated.accessJwt, refreshJwt: migrated.refreshJwt, commit,
    records: [{ rkey: golden.rkey, value: record }, { rkey: recoveryRkey, value: recoveryRecord }],
    blobCid: uploaded.blob.ref.$link, blobBytes: blobBytes.toString('base64'),
  }), { mode: 0o600 });
  console.log('NOTE raw upstream reads are public inside the private network; gateway isolation is mandatory');
}

async function restore() {
  const state = JSON.parse(readFileSync('/gate-state/restored-state.json', 'utf8'));
  document = state.document;
  await new Promise(resolve => server.listen(443, '0.0.0.0', resolve));
  const base = 'http://pds-restored:2583';
  await waitReady(base);
  // No original PDS is running and this process only uses decrypted backup state.
  const account = await rpc('com.atproto.server.getSession', { base, token: state.accessJwt });
  assert.equal(account.did, state.did);
  const login = await rpc('com.atproto.server.createSession', { base,
    body: { identifier: state.handle, password: state.password } });
  assert.equal(login.did, state.did);
  const refreshed = await rpc('com.atproto.server.refreshSession', {
    base, token: state.refreshJwt, method: 'POST' });
  assert.equal(refreshed.did, state.did);
  const commit = await rpc('com.atproto.sync.getLatestCommit', { base, query: { did: state.did } });
  assert.deepEqual(commit, state.commit);
  const credentials = await rpc('com.atproto.identity.getRecommendedDidCredentials', {
    base, token: login.accessJwt });
  assert.equal(credentials.verificationMethods.atproto, state.signingKey);
  for (const record of state.records) {
    const restored = await rpc('com.atproto.repo.getRecord', { base,
      query: { repo: state.did, collection: record.value.$type, rkey: record.rkey } });
    assert.deepEqual(restored.value, record.value);
  }
  const blob = await rpc('com.atproto.sync.getBlob', { base,
    query: { did: state.did, cid: state.blobCid } });
  assert.deepEqual(Buffer.from(blob), Buffer.from(state.blobBytes, 'base64'));
  console.log('PASS cold restore preserves account/password, access/refresh credentials, signing key, commit, records and blob bytes');
  const next = { ...state.records[0].value, recordId: randomUUID() };
  await rpc('com.atproto.repo.createRecord', { base, token: refreshed.accessJwt,
    body: { repo: state.did, collection: next.$type, rkey: randomUUID(), record: next, validate: false } });
  const bytes = await rpc('com.atproto.sync.getRepo', { base, query: { did: state.did } });
  await verifyRepoCar(bytes, state.did, state.signingKey);
  await assert.rejects(verifyRepoCar(bytes, state.did, state.wrongKey));
  await rpc('com.atproto.server.createInviteCode', { base, admin: true, body: { useCount: 1 } });
  console.log('PASS restored PDS can authenticate, administer and publish new verifiable records');
  console.log('PARTIAL PRODUCT GATE: Memory gateway access, outbox/inbox recovery, two Memory instances and writer fencing remain unproven');
}

async function runtime() {
  await main();
  chownSync('/gate-state/recovery-state.json', Number(process.env.GATE_HOST_UID), Number(process.env.GATE_HOST_GID));
  writeFileSync('/gate-state/runtime-ready', 'ready', { mode: 0o600 });
  chownSync('/gate-state/runtime-ready', Number(process.env.GATE_HOST_UID), Number(process.env.GATE_HOST_GID));
  const deadline = Date.now() + 600_000;
  while (!existsSync('/gate-state/runtime-finished')) {
    assert.ok(Date.now() < deadline, 'Rust runtime verification timed out');
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  const state = JSON.parse(readFileSync('/gate-state/recovery-state.json', 'utf8'));
  const base = 'http://pds-recovery:2583';
  const collection = 'dev.memory-layer.unspecced.publication';
  const schemas = new Lexicons(readdirSync('/gate/publication-lexicons').map(file =>
    JSON.parse(readFileSync(`/gate/publication-lexicons/${file}`, 'utf8'))));
  const records = []; let cursor;
  do {
    const page = await rpc('com.atproto.repo.listRecords', { base,
      query: { repo: state.did, collection, limit: 100, ...(cursor ? { cursor } : {}) } });
    records.push(...page.records); cursor = page.cursor;
    assert.ok(records.length <= 1000, 'bounded synthetic publication history');
  } while (cursor);
  assert.ok(records.length > 32, 'Rust workflow must publish a multi-page history');
  const versions = new Set();
  for (const record of records) {
    schemas.assertValidRecord(collection, record.value);
    const version = record.value.delivery.envelope.schema_version;
    assert.ok(version === 2 || version === 3); versions.add(version);
    for (const privateText of ['NEVER_SHARE', 'Reviewed synthetic evidence only', 'PRIVATE_PROVIDER_ERROR_SENTINEL', 'What is the reviewed incident diagnosis?', 'Synthetic reviewed incident answer.']) {
      assert.ok(!JSON.stringify(record.value).includes(privateText), 'private expertise payload entered PDS');
    }
  }
  assert.ok(versions.has(2) && versions.has(3), 'legacy and expertise schema histories both present');
  const bytes = await rpc('com.atproto.sync.getRepo', { base, query: { did: state.did } });
  await verifyRepoCar(bytes, state.did, state.signingKey);
  await assert.rejects(verifyRepoCar(bytes, state.did, state.wrongKey));
  console.log('PASS Rust v2/v3 publications in real PDS, no private expertise payloads, independent Lexicon and signed CAR verification, wrong-key rejection');
}

(process.env.GATE_PHASE === 'restore' ? restore()
  : process.env.GATE_PHASE === 'runtime' ? runtime() : main()).catch(error => {
  // Never log auth tokens, private keys, or full request bodies.
  console.error(error.message);
  process.exitCode = 1;
}).finally(() => server.close());
