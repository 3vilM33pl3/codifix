// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs inside the exact disposable network namespace being tested.
const assert = require('node:assert/strict');
const { readFileSync, existsSync } = require('node:fs');
const { createHash } = require('node:crypto');
const https = require('node:https');
const net = require('node:net');
const dns = require('node:dns');
const config = JSON.parse(readFileSync('/fixture/probes.json', 'utf8'));
const ca = readFileSync('/fixture/ca.pem');
const [role] = process.argv.slice(2);
let stage = 'initialization';
function request(origin, path, { method = 'GET', trusted = true, wrongName = false } = {}) {
  const url = new URL(origin);
  return new Promise((resolve, reject) => {
    const request = https.request({ hostname: url.hostname, port: url.port || 443, path, method,
      // An explicit empty root set must override NODE_EXTRA_CA_CERTS on the
      // maintenance runner; omitting ca would still trust that fixture root.
      ca: trusted ? ca : [], servername: wrongName ? 'wrong.company.example.com' : url.hostname,
      lookup: (host, options, callback) => {
        const address = config.roles[role]?.tlsResolve?.[host];
        if (address) callback(null, options?.all ? [{ address, family: 4 }] : address, 4);
        else dns.lookup(host, options, callback);
      },
      timeout: 3000, agent: false }, response => {
      const chunks = []; let count = 0;
      response.on('data', chunk => { count += chunk.length; if (count > 32 * 1024) request.destroy(Error('oversized fixture response')); else chunks.push(chunk); });
      response.on('end', () => resolve({ status: response.statusCode, headers: response.headers, bytes: Buffer.concat(chunks) }));
    });
    request.on('error', reject); request.on('timeout', () => request.destroy(Error('fixture timeout'))); request.end();
  });
}
async function retryRequest(origin, path) {
  for (let attempt = 0; attempt < 40; attempt++) {
    try { return await request(origin, path); } catch { await new Promise(resolve => setTimeout(resolve, 100)); }
  }
  throw Error('fixture TLS readiness timed out');
}
function tcp({ host, port }) {
  return new Promise(resolve => {
    let finished = false;
    const socket = net.connect({ host, port });
    const done = value => { if (!finished) { finished = true; socket.destroy(); resolve(value); } };
    socket.setTimeout(400); socket.on('connect', () => done(true)); socket.on('error', () => done(false)); socket.on('timeout', () => done(false));
  });
}
async function upstream(origin, path, service, method = 'GET') {
  const response = await request(origin, path, { method });
  assert.equal(response.status, 200, 'allowed proxy request');
  assert.deepEqual(JSON.parse(response.bytes), { upstream: service, method, path });
}
async function denied(origin, paths, method = 'GET') {
  for (const path of paths) assert.equal((await request(origin, path, { method })).status, 404, 'denied route');
}
async function deniedNetworks() {
  const failures = await Promise.all(config.roles[role].denied.map(async target => await tcp(target)));
  assert.ok(failures.every(value => !value), 'forbidden direct DNS/IP connection');
  for (const target of config.roles[role].allowed) assert.equal(await tcp(target), true, 'reachable positive control');
}
async function main() {
  assert.ok(config.roles[role], 'known fixture role');
  if (role === 'operator') {
    stage = 'operator_routes';
    await retryRequest(config.operator, '/xrpc/com.atproto.server.getSession');
    for (const path of config.operatorGet) await upstream(config.operator, path, 'pds');
    for (const path of config.operatorPost) await upstream(config.operator, path, 'pds', 'POST');
    await denied(config.operator, config.operatorGet, 'POST');
    await denied(config.operator, config.operatorPost);
    await denied(config.operator, ['/xrpc/com.atproto.repo.createRecord', '/xrpc/com.atproto.repo.deleteRecord', '/xrpc/com.atproto.repo.importRepo', '/xrpc/com.atproto.repo.uploadBlob', '/xrpc/com.atproto.sync.subscribeRepos', '/xrpc/com.atproto.server.deleteAccount', '/xrpc/com.atproto.server.getSession/extra', '/config/key.pem', '/']);
    stage = 'did_bytes_and_headers';
    const response = await retryRequest(config.repository, '/.well-known/did.json');
    assert.equal(response.status, 200);
    assert.equal(response.headers['content-type'], 'application/did+json');
    assert.equal(response.headers['cache-control'], 'no-store');
    assert.equal(response.bytes.toString('utf8'), config.didDocument);
    assert.equal(createHash('sha256').update(response.bytes).digest('hex'), config.didDocumentSha256);
    stage = 'did_route_restrictions';
    await denied(config.repository, ['/', '/.well-known/did.json/extra', '/config/Caddyfile', '/tls/key.pem', '/.well-known/../tls/key.pem']);
    await denied(config.repository, ['/.well-known/did.json'], 'POST');
    await denied(config.repository, ['/.well-known/did.json'], 'HEAD');
    stage = 'did_wrong_trust_rejection';
    await assert.rejects(request(config.repository, '/.well-known/did.json', { trusted: false }));
    await assert.rejects(request(config.repository, '/.well-known/did.json', { wrongName: true }));
  } else if (role === 'gateway') {
    await retryRequest(config.publisher, '/xrpc/com.atproto.repo.getRecord');
    for (const path of ['/xrpc/com.atproto.repo.getRecord', '/xrpc/com.atproto.sync.getRepo']) await upstream(config.publisher, path, 'pds');
    await upstream(config.publisher, '/xrpc/com.atproto.repo.createRecord', 'pds', 'POST');
    await denied(config.publisher, [...config.operatorGet.filter(path => !path.endsWith('getRepo')), ...config.operatorPost, '/xrpc/com.atproto.repo.importRepo', '/xrpc/com.atproto.sync.subscribeRepos']);
    await assert.rejects(request(config.publisher, '/xrpc/com.atproto.repo.getRecord', { trusted: false }));
  } else {
    await retryRequest(config.gateway, '/v1/pull');
    for (const path of ['/v1/accept', '/v1/pull', '/v1/retention']) await upstream(config.gateway, path, 'gateway');
    await denied(config.gateway, ['/v1/expertise', '/xrpc/com.atproto.sync.getRepo', '/xrpc/com.atproto.sync.subscribeRepos', '/v1/pull/extra', '/mcp', '/config/gateway.json']);
    const ui = config.members[role];
    for (const path of ['/', '/assets/fixture.js', '/v1/projects', '/ws']) await upstream(ui, path, `${role}_app`);
    await denied(ui, ['/xrpc/com.atproto.server.createAccount', '/config/app.toml']);
    await assert.rejects(request(config.gateway, '/v1/pull', { trusted: false }));
    for (const forbidden of ['/run/secrets/pds-token', '/run/secrets/audit-key', '/run/secrets/owner-password', '/var/run/docker.sock']) assert.equal(existsSync(forbidden), false);
  }
  stage = 'direct_network_boundary';
  await deniedNetworks();
  console.log(`PASS ${role}: generated TLS routes, trust rejection and direct-network boundaries`);
}
main().catch(() => { console.error(`FAIL ${role}: ${stage}`); process.exitCode = 1; });
