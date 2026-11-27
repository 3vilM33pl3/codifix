// Private DID documents and the host-local account provisioning tool.
const { createRequire } = require('node:module');
const { readFileSync, writeFileSync, renameSync, chownSync } = require('node:fs');
const https = require('node:https');
const http = require('node:http');
const { randomBytes } = require('node:crypto');

const appRequire = createRequire('/app/index.ts');
const pdsRequire = createRequire(appRequire.resolve('@atproto/pds'));
const { Secp256k1Keypair } = pdsRequire('@atproto/crypto');
const { createServiceJwt } = pdsRequire('@atproto/xrpc-server');
const domain = process.env.PRIVATE_AT_DOMAIN;
const pdsName = 'pds.' + domain;
const pdsUrl = 'https://' + pdsName;
const roles = [process.env.PRIVATE_AT_FIRST, process.env.PRIVATE_AT_SECOND];
const filename = role => '/state/' + role + '.json';
const load = role => {
  try { return JSON.parse(readFileSync(filename(role), 'utf8')); }
  catch (err) { if (err.code === 'ENOENT') return null; throw err; }
};
const save = (role, value) => {
  const temp = filename(role) + '.new';
  writeFileSync(temp, JSON.stringify(value, null, 2) + '\n', { mode: 0o600 });
  chownSync(temp, Number(process.env.PRIVATE_AT_UID), Number(process.env.PRIVATE_AT_GID));
  renameSync(temp, filename(role));
};

function sendDocument(req, res) {
  const role = roles.find(item => req.headers.host === item + '.' + domain);
  const account = role && load(role);
  if (req.method !== 'GET' || req.url !== '/.well-known/did.json' || !account) {
    res.writeHead(404).end();
    return;
  }
  const body = JSON.stringify({
    '@context': ['https://www.w3.org/ns/did/v1'],
    id: account.did,
    alsoKnownAs: ['at://' + account.handle],
    verificationMethod: [{
      id: account.did + '#atproto', type: 'Multikey', controller: account.did,
      publicKeyMultibase: account.signingKey.slice('did:key:'.length),
    }],
    service: [{
      id: account.did + '#atproto_pds', type: 'AtprotoPersonalDataServer', serviceEndpoint: pdsUrl,
    }],
  });
  res.writeHead(200, { 'content-type': 'application/did+ld+json', 'content-length': Buffer.byteLength(body) });
  res.end(body);
}

async function rpc(name, { body, bearer, admin, method = 'POST', query } = {}) {
  const url = new URL('/xrpc/com.atproto.' + name, 'http://pds:2583');
  if (query) url.search = new URLSearchParams(query).toString();
  const headers = {};
  if (body) headers['content-type'] = 'application/json';
  if (bearer) headers.authorization = 'Bearer ' + bearer;
  if (admin) headers.authorization = 'Basic ' + Buffer.from('admin:' + process.env.PRIVATE_AT_ADMIN_PASSWORD).toString('base64');
  const result = await fetch(url, { method, headers, body: body ? JSON.stringify(body) : undefined, redirect: 'error', signal: AbortSignal.timeout(10000) });
  if (!result.ok) { const failure = await result.json(); throw Error(name + ' returned HTTP ' + result.status + ' ' + failure.error + ': ' + failure.message); }
  const data = await result.text();
  return data ? JSON.parse(data) : {};
}

async function seed(role) {
  let account = load(role);
  if (!account) {
    const key = await Secp256k1Keypair.create({ exportable: true });
    account = {
      did: 'did:web:' + role + '.' + domain, handle: role + '.' + domain,
      signingKey: key.did(), password: randomBytes(32).toString('hex'),
      bootstrap: Buffer.from(await key.export()).toString('hex'),
    };
    save(role, account);
  }
  if (account.active) {
    const session = await rpc('server.createSession', { body: { identifier: account.did, password: account.password } });
    if (session.did !== account.did) throw Error('Unexpected account identity');
    save(role, { ...account, accessJwt: session.accessJwt, refreshJwt: session.refreshJwt });
    return;
  }
  if (!account.refreshJwt) {
    const key = await Secp256k1Keypair.import(account.bootstrap);
    const invite = await rpc('server.createInviteCode', { admin: true, body: { useCount: 1 } });
    const jwt = await createServiceJwt({ iss: account.did, aud: 'did:web:' + pdsName,
      lxm: 'com.atproto.server.createAccount', keypair: key });
    const session = await rpc('server.createAccount', { bearer: jwt, body: {
      did: account.did, handle: account.handle, email: role + '@' + domain,
      password: account.password, inviteCode: invite.code,
    } });
    if (session.did !== account.did) throw Error('Unexpected account identity');
    Object.assign(account, { accessJwt: session.accessJwt, refreshJwt: session.refreshJwt });
    save(role, account);
  }
  const credentials = await rpc('identity.getRecommendedDidCredentials', { bearer: account.accessJwt, method: 'GET' });
  account.signingKey = credentials.verificationMethods.atproto;
  save(role, account);
  await rpc('server.activateAccount', { bearer: account.accessJwt });
  account.active = true;
  delete account.bootstrap;
  save(role, account);
}

// The host-facing listener forwards only companion login and service-token requests.
const publicRoutes = new Map([
  ['/xrpc/_health', 'GET'],
  ['/xrpc/com.atproto.server.createSession', 'POST'],
  ['/xrpc/com.atproto.server.refreshSession', 'POST'],
  ['/xrpc/com.atproto.server.getServiceAuth', 'GET'],
]);
function forwardLocalRequest(req, res) {
  if (!req.url.startsWith('/') || publicRoutes.get(req.url.split('?')[0]) !== req.method) {
    res.writeHead(404).end();
    return;
  }
  const headers = {};
  for (const name of ['authorization', 'content-type']) {
    if (req.headers[name]) headers[name] = req.headers[name];
  }
  const upstream = http.request({ hostname: 'pds', port: 2583, path: req.url, method: req.method, headers }, response => {
    res.writeHead(response.statusCode, { 'content-type': 'application/json', 'cache-control': 'no-store' });
    response.pipe(res);
  });
  upstream.on('error', () => { if (!res.headersSent) res.writeHead(502); res.end(); });
  req.pipe(upstream);
}

async function main() {
  if (process.argv[2] !== 'enroll') {
    http.createServer(forwardLocalRequest).listen(2584, '0.0.0.0');
    https.createServer({ key: readFileSync('/tls/key.pem'), cert: readFileSync('/tls/cert.pem') }, sendDocument).listen(443, '0.0.0.0');
    return;
  }
  for (const role of roles) await seed(role);
  for (const role of roles) {
    const other = roles.find(item => item !== role);
    const account = load(role);
    const remote = load(other);
    const issued = await rpc('server.getServiceAuth', {
      method: 'GET', bearer: account.accessJwt,
      query: { aud: remote.did + '#codifix_agency', lxm: 'dev.codifix.agency.sendMessage' },
    });
    const payload = JSON.parse(Buffer.from(issued.token.split('.')[1], 'base64url'));
    if (payload.iss !== account.did || payload.aud !== remote.did + '#codifix_agency' ||
        payload.lxm !== 'dev.codifix.agency.sendMessage') throw Error('Unexpected service token binding');
    console.log(role + ' enrolled and method-bound service token issued');
  }
}
main().catch(err => { console.error(err.message); process.exitCode = 1; });
