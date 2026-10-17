import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { companyTopology } from './company-topology.mjs';
import { companyBootstrapTopology } from './company-bootstrap-topology.mjs';

const digest = value => createHash('sha256').update(value).digest('hex');
function company() {
  return { schemaVersion: 1, name: 'codifix-company', root: '/srv/codifix-company', domain: 'memory.company.internal',
    images: Object.fromEntries(['app', 'gateway', 'postgres', 'pds', 'proxy'].map((key, index) => [key, `registry.company.internal/${key}@sha256:${String(index + 1).repeat(64)}`])),
    users: { app: '65532:65532', gateway: '65532:65532', proxy: '65532:65532', pds: '1000:1000', postgres: '999:999' } };
}
function bootstrap() {
  const repositoryHostname = 'repository.memory.company.internal';
  const repositoryDid = `did:web:${repositoryHostname}`;
  const doc = { '@context': ['https://www.w3.org/ns/did/v1'], id: repositoryDid,
    alsoKnownAs: [`at://${repositoryHostname}`], verificationMethod: [{ id: `${repositoryDid}#atproto`, type: 'Multikey', controller: repositoryDid,
      publicKeyMultibase: `z${'a'.repeat(48)}` }],
    service: [{ id: `${repositoryDid}#atproto_pds`, type: 'AtprotoPersonalDataServer', serviceEndpoint: 'https://pds.memory.company.internal' }] };
  const didDocument = `${JSON.stringify(doc)}\n`;
  return { schemaVersion: 1, operatorHostname: 'operator.memory.company.internal', repositoryHostname, repositoryDid,
    accountHandle: repositoryHostname, didDocument, didDocumentSha256: digest(didDocument), caCertificateSha256: 'a'.repeat(64) };
}
const networks = service => Array.isArray(service.networks) ? service.networks : Object.keys(service.networks);
const adjacent = (a, b) => networks(a).some(network => networks(b).includes(network));

test('bootstrap overlay leaves normal services/routes and original inputs unchanged', () => {
  const input = company(), options = bootstrap();
  const before = JSON.stringify({ input, options });
  const base = companyTopology(input);
  const result = companyBootstrapTopology(input, options);
  assert.equal(JSON.stringify({ input, options }), before);
  assert.deepEqual(companyTopology(input), base);
  for (const [key, value] of Object.entries(base.compose.services)) if (!['pds', 'pds_tls'].includes(key)) assert.deepEqual(result.compose.services[key], value);
  assert.deepEqual(result.compose.services.pds_tls, { ...base.compose.services.pds_tls, sysctls: { 'net.ipv4.ip_unprivileged_port_start': '443' } });
  for (const [key, value] of Object.entries(base.proxyFiles)) {
    assert.equal(result.proxyFiles[key], key === 'pds/tls/Caddyfile' ? value.replace('https://pds.memory.company.internal:8443 {', 'https://pds.memory.company.internal:443 {') : value);
  }
  assert.equal(base.endpoints.pds, 'https://pds.memory.company.internal:8443');
  assert.equal(result.endpoints.pds, 'https://pds.memory.company.internal');
  assert.equal(result.posture.accountProvisioned, false);
  assert.equal(result.posture.installed, false);
  assert.equal(result.posture.externalProviders, false);
  assert.equal(result.posture.autonomousAgents, false);
  assert.deepEqual(result.posture.approvedCollections, []);
});

test('operator and DID networks cannot bypass participant, publisher or database edges', () => {
  const { compose: { services: s, networks: graph } } = companyBootstrapTopology(company(), bootstrap());
  for (const proxy of ['pds_operator_tls', 'repository_did']) {
    assert.ok(adjacent(s[proxy], s.pds));
    for (const denied of ['gateway', 'gateway_edge', 'gateway_db', 'pds_tls', 'workshop_app', 'workshop_db', 'workshop_edge', 'homelab_app', 'homelab_db', 'homelab_edge']) {
      assert.equal(adjacent(s[proxy], s[denied]), false, `${proxy} -> ${denied}`);
    }
    assert.equal(s[proxy].ports, undefined);
    assert.equal(s[proxy].privileged, undefined);
    assert.deepEqual(s[proxy].cap_drop, ['ALL']);
    assert.equal(s[proxy].user, company().users.proxy);
    assert.ok(!s[proxy].volumes.some(m => /pds-token|audit-key|authority-token|owner-password|member-tokens|docker.sock/.test(m.source)));
  }
  const operator = { networks: ['pds_operator_ingress'] };
  assert.ok(adjacent(operator, s.pds_operator_tls));
  assert.ok(adjacent(operator, s.repository_did));
  assert.equal(adjacent(operator, s.pds), false);
  assert.equal(adjacent(operator, s.pds_tls), false);
  assert.equal(adjacent(operator, s.gateway_db), false);
  assert.equal(s.pds_operator_tls.sysctls, undefined);
  for (const n of Object.values(graph)) assert.deepEqual(n, { internal: true, enable_ipv6: false });
});

test('operator allowlist is exact and method-bound, with no proxy/upload/delete/firehose routes', () => {
  const { proxyFiles } = companyBootstrapTopology(company(), bootstrap());
  const config = proxyFiles['bootstrap/operator/config/Caddyfile'];
  const read = config.match(/@read \{\n method GET\n path ([^\n]+)\n \}/)?.[1].split(' ');
  const write = config.match(/@write \{\n method POST\n path ([^\n]+)\n \}/)?.[1].split(' ');
  assert.deepEqual(read, ['/xrpc/com.atproto.identity.getRecommendedDidCredentials', '/xrpc/com.atproto.server.getSession', '/xrpc/com.atproto.sync.getRepo']);
  assert.deepEqual(write, ['/xrpc/com.atproto.server.createInviteCode', '/xrpc/com.atproto.server.createAccount', '/xrpc/com.atproto.server.activateAccount', '/xrpc/com.atproto.server.createSession', '/xrpc/com.atproto.server.refreshSession']);
  assert.equal((config.match(/reverse_proxy/g) ?? []).length, 2);
  assert.ok(config.includes('reverse_proxy @read pds:2583'));
  assert.ok(config.includes('reverse_proxy @write pds:2583'));
  assert.ok(config.includes('respond 404'));
  for (const forbidden of ['createRecord', 'deleteRecord', 'importRepo', 'uploadBlob', 'subscribeRepos', 'getServiceAuth', '/*', '{uri}', 'tls_insecure_skip_verify']) assert.ok(!config.includes(forbidden), forbidden);
});

test('static DID response preserves exact reviewed bytes and digest across compact and pretty JSON', () => {
  for (const indent of [undefined, 2]) {
    const input = bootstrap();
    input.didDocument = `${JSON.stringify(JSON.parse(input.didDocument), null, indent)}\n`;
    input.didDocumentSha256 = digest(input.didDocument);
    const result = companyBootstrapTopology(company(), input);
    const config = result.proxyFiles['bootstrap/repository/config/Caddyfile'];
    // Backtick tokens are literal; Caddy's documented placeholder escape removes
    // the slash before an opening brace, not the JSON's quotes or whitespace.
    const literal = config.match(/respond @document `([\s\S]*)` 200\n/)?.[1];
    assert.ok(literal);
    assert.equal(literal.replaceAll('\\{', '{'), input.didDocument);
    assert.equal(digest(literal.replaceAll('\\{', '{')), result.endpoints.repository.documentSha256);
    assert.ok(!/(?<!\\)\{/.test(literal));
    assert.match(config, /https:\/\/repository.memory.company.internal:443/);
    assert.match(config, /@document \{\n method GET\n path \/.well-known\/did.json\n \}/);
    assert.ok(config.includes('Content-Type application/did+json'));
    assert.ok(config.includes('Cache-Control no-store'));
    assert.ok(!config.includes('file_server'));
    assert.ok(!config.includes('reverse_proxy'));
    assert.deepEqual(result.compose.services.repository_did.sysctls, { 'net.ipv4.ip_unprivileged_port_start': '443' });
  }
});

test('DID revision handoff uses a dedicated readonly config directory and a PDS-only pinned CA', () => {
  const input = bootstrap();
  const result = companyBootstrapTopology(company(), input);
  const host = result.compose.services.repository_did;
  assert.deepEqual(host.volumes[0], { type: 'bind', source: '/srv/codifix-company/bootstrap/repository/config', target: '/config', read_only: true, bind: { create_host_path: false } });
  assert.equal(host.volumes.filter(m => m.source.endsWith('/key.pem')).length, 1);
  assert.equal(result.requiredFiles.filter(f => f.path.endsWith('/bootstrap/repository/tls/key.pem')).length, 1);
  const pds = result.compose.services.pds;
  assert.equal(pds.environment.NODE_EXTRA_CA_CERTS, '/run/trust/repository-ca.pem');
  assert.equal(pds.environment.PDS_DISABLE_SSRF_PROTECTION, 'false');
  assert.equal(pds.environment.PDS_DEV_MODE, 'false');
  assert.equal(pds.environment.PDS_SERVICE_HANDLE_DOMAINS, '.memory.company.internal');
  const ca = result.requiredFiles.find(f => f.service === 'pds' && f.target === '/run/trust/repository-ca.pem');
  assert.equal(ca.sha256, input.caCertificateSha256);
  assert.equal(ca.secret, false);
});

test('reject malformed, ambiguous, wrong-identity, private-key and configuration-injection inputs', () => {
  const changedDocument = change => {
    const input = bootstrap(), doc = JSON.parse(input.didDocument);
    change(doc);
    input.didDocument = JSON.stringify(doc);
    input.didDocumentSha256 = digest(input.didDocument);
    return input;
  };
  const bad = [
    { ...bootstrap(), schemaVersion: 2 }, { ...bootstrap(), accountPassword: 'not permitted' },
    { ...bootstrap(), operatorHostname: 'workshop.memory.company.internal' },
    { ...bootstrap(), operatorHostname: bootstrap().repositoryHostname },
    { ...bootstrap(), repositoryHostname: 'pds.memory.company.internal' },
    { ...bootstrap(), repositoryHostname: 'repository.example\nreverse_proxy host' },
    { ...bootstrap(), repositoryDid: 'did:web:wrong.example.com' },
    { ...bootstrap(), didDocumentSha256: 'b'.repeat(64) }, { ...bootstrap(), caCertificateSha256: 'bad' },
    changedDocument(doc => { doc.privateKey = 'never serve this'; }),
    changedDocument(doc => { doc.id = 'did:web:other.company.internal'; }),
    changedDocument(doc => { doc.verificationMethod[0].controller = 'did:web:other.company.internal'; }),
    changedDocument(doc => { doc.verificationMethod[0].publicKeyMultibase = '`\nreverse_proxy attacker\n`'; }),
    changedDocument(doc => { doc.alsoKnownAs = ['at://{env.SECRET}']; }),
    changedDocument(doc => { doc.service[0].serviceEndpoint = `https://${bootstrap().operatorHostname}:8443`; }),
    changedDocument(doc => { doc.service.push(doc.service[0]); }),
    changedDocument(doc => { doc.verificationMethod[0].privateKeyJwk = {}; }),
  ];
  const duplicate = bootstrap();
  duplicate.didDocument = duplicate.didDocument.replace('"id":', '"id":"hidden ignored bytes","id":');
  duplicate.didDocumentSha256 = digest(duplicate.didDocument);
  bad.push(duplicate);
  for (const input of bad) assert.throws(() => companyBootstrapTopology(company(), input));
});
