// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs against the actual upstream image, using its independent AT SDK verifier.
const assert = require('node:assert/strict');
const { createRequire } = require('node:module');
const { readFileSync, readdirSync } = require('node:fs');
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
  base = 'http://pds:2583', rawBody } = {}) {
  const url = new URL(`/xrpc/${nsid}`, base);
  if (query) url.search = new URLSearchParams(query).toString();
  const headers = {};
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (rawBody !== undefined) headers['content-type'] = 'application/vnd.ipld.car';
  if (token) headers.authorization = `Bearer ${token}`;
  if (admin) headers.authorization = `Basic ${Buffer.from(`admin:${process.env.GATE_ADMIN_PASSWORD}`).toString('base64')}`;
  const res = await fetch(url, { method: method ?? (body === undefined ? 'GET' : 'POST'),
    headers, body: rawBody ?? (body === undefined ? undefined : JSON.stringify(body)),
    signal: AbortSignal.timeout(15_000), redirect: 'error' });
  const value = res.headers.get('content-type')?.includes('json')
    ? await res.json() : new Uint8Array(await res.arrayBuffer());
  if (!res.ok) throw new Error(`${nsid}: HTTP ${res.status} ${JSON.stringify(value)}`);
  return value;
}

async function main() {
  const initialKey = await Secp256k1Keypair.create({ exportable: true });
  setDocument(initialKey.did());
  await new Promise(resolve => server.listen(443, '0.0.0.0', resolve));
  for (let i = 0; i < 60; i++) {
    try {
      await rpc('com.atproto.server.describeServer');
      break;
    } catch (error) {
      if (i === 59) throw error;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
  }
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
  const migrated = await rpc('com.atproto.server.createAccount', { base: recovery,
    token: migrationAuth.token, body: { did, handle, email: 'gate@federation.example.com',
      password: randomBytes(24).toString('hex'), inviteCode: recoveryInvite.code },
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
  const recoveryRecord = { ...record, recordId: randomUUID() };
  await rpc('com.atproto.repo.createRecord', { base: recovery, token: migrated.accessJwt,
    body: { repo: did, collection: record.$type, rkey: randomUUID(),
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
  console.log('NOTE raw upstream reads are public inside the private network; gateway isolation is mandatory');
  console.log('PARTIAL GATE: disaster restore, exposure controls and Memory integration remain untested');
}

main().catch(error => {
  // Never log auth tokens, private keys, or full request bodies.
  console.error(error.message);
  process.exitCode = 1;
}).finally(() => server.close());
