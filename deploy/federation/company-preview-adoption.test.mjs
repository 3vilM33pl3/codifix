// SPDX-License-Identifier: AGPL-3.0-or-later
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { companyBootstrapTopology } from './company-bootstrap-topology.mjs';
import { companyPreviewAdoption } from './company-preview-adoption.mjs';

function fixture(extraMembers = []) {
  const company = { schemaVersion: 1, name: 'codifix-company', root: '/srv/codifix-company', domain: 'company.example.com',
    images: Object.fromEntries(['app', 'gateway', 'postgres', 'pds', 'proxy'].map((key, index) => [key, `registry.example.com/codifix/${key}@sha256:${String(index + 1).repeat(64)}`])),
    users: { app: '1000:1000', gateway: '1000:1000', proxy: '1000:1000', pds: '1000:1000', postgres: '999:999' },
    members: ['workshop', 'homelab', ...extraMembers], federationEnabled: true, expertiseRoutes: true };
  const did = 'did:web:space.company.example.com';
  const document = { '@context': ['https://www.w3.org/ns/did/v1'], id: did, alsoKnownAs: ['at://space.company.example.com'],
    verificationMethod: [{ id: `${did}#atproto`, type: 'Multikey', controller: did, publicKeyMultibase: `z${'a'.repeat(48)}` }],
    service: [{ id: `${did}#atproto_pds`, type: 'AtprotoPersonalDataServer', serviceEndpoint: 'https://pds.company.example.com' }] };
  const bytes = JSON.stringify(document);
  const bootstrap = { schemaVersion: 1, repositoryHostname: 'space.company.example.com', repositoryDid: did,
    accountHandle: 'space.company.example.com', operatorHostname: 'operator.company.example.com',
    didDocument: bytes, didDocumentSha256: createHash('sha256').update(bytes).digest('hex'), caCertificateSha256: 'f'.repeat(64) };
  const members = company.members.map((member, index) => ({ member, containerName: `codifix-${member}-preview-app`, containerId: String(index + 1).repeat(64) }));
  const adoption = { schemaVersion: 1, members, inventory: {
    containers: members.map(member => ({ name: member.containerName, id: member.containerId, networks: [`codifix-${member.member}-preview`] })),
    networks: members.map(member => `codifix-${member.member}-preview`), volumes: ['unrelated-existing-volume'] } };
  return { company, bootstrap, adoption };
}
const plan = ({ company, bootstrap, adoption }) => companyPreviewAdoption(company, bootstrap, adoption);
const networksOf = service => Array.isArray(service.networks) ? service.networks : Object.keys(service.networks);
function withPreparedDatabase() {
  const input = fixture();
  const database = { containerName: 'codifix-company-gateway_db-1', containerId: 'd'.repeat(64), networkName: 'codifix-company_gateway_database', alias: 'gateway_db' };
  input.adoption.preparedGatewayDatabase = database;
  input.adoption.inventory.containers.push({ name: database.containerName, id: database.containerId, networks: [database.networkName],
    networkAliases: { [database.networkName]: ['gateway_db', database.containerName] } });
  input.adoption.inventory.networks.push(database.networkName);
  input.adoption.inventory.volumes.push('codifix-company_gateway_db_data');
  return input;
}

test('adoption removes every duplicate member service, mount, volume, network and UI claim', () => {
  const input = fixture(); const original = structuredClone(input); const result = plan(input);
  assert.deepEqual(input, original);
  assert.deepEqual(Object.keys(result.compose.services).sort(), ['gateway', 'gateway_db', 'gateway_edge', 'pds', 'pds_operator_tls', 'pds_tls', 'repository_did']);
  assert.deepEqual(Object.keys(result.compose.volumes).sort(), ['gateway_db_data', 'pds_data']);
  assert.deepEqual(result.endpoints.members, {});
  assert.ok(result.requiredFiles.every(file => Object.hasOwn(result.compose.services, file.service) && !file.path.includes('/members/')));
  assert.ok(Object.keys(result.proxyFiles).every(file => !file.startsWith('members/')));
  for (const member of input.company.members) {
    assert.deepEqual(Object.keys(result.compose.networks).filter(name => name.startsWith(`${member}_`)), [`${member}_federation`]);
    assert.ok(result.requiredFiles.some(file => file.path.endsWith(`/gateway/member-tokens/${member}`)));
  }
  assert.equal(result.posture.installed, false);
  assert.equal(result.adoption.liveInventoryVerified, false);
  assert.equal(result.adoption.attachmentsExecuted, false);
});

test('each existing app gets its own new internal network with only TLS gateway, never existing DB networks', () => {
  const input = fixture(); const result = plan(input);
  for (const attachment of result.adoption.attachments) {
    assert.deepEqual(attachment.command, ['docker', 'network', 'connect', attachment.network, attachment.containerId]);
    assert.deepEqual(attachment.currentNetworks, [`codifix-${attachment.member}-preview`]);
    const local = `${attachment.member}_federation`;
    assert.deepEqual(result.compose.networks[local], { internal: true, enable_ipv6: false, name: `codifix-company_${local}` });
    const peers = Object.entries(result.compose.services).filter(([, service]) => networksOf(service).includes(local)).map(([name]) => name);
    assert.deepEqual(peers, ['gateway_edge']);
  }
  for (const network of Object.values(result.compose.networks)) assert.ok(!input.adoption.inventory.networks.includes(network.name));
  for (const service of Object.values(result.compose.services)) assert.ok(networksOf(service).every(name => !input.adoption.inventory.networks.includes(name)));
  assert.deepEqual(result.compose.services.gateway.networks, ['gateway_api', 'gateway_database', 'pds_client']);
});

test('authority hardening, mounted custody and TLS routes remain byte-for-byte unchanged', () => {
  const input = fixture(); const before = companyBootstrapTopology(input.company, input.bootstrap); const after = plan(input);
  for (const [name, service] of Object.entries(after.compose.services)) assert.deepEqual(service, before.compose.services[name]);
  for (const [name, bytes] of Object.entries(after.proxyFiles)) assert.equal(bytes, before.proxyFiles[name]);
  assert.equal(after.adoption.modelEgressChanged, false);
  assert.equal(after.posture.existingMemberModelPolicy, 'unchanged and not inspected');
  assert.deepEqual(after.posture.enrolledMembers, []);
  assert.deepEqual(after.posture.approvedCollections, []);
  assert.equal(after.adoption.backupRequired, true);
  assert.equal(after.adoption.configRecreationRequired, true);
  assert.equal(after.adoption.postRecreationInventoryRequired, true);
});

test('ordering is deterministic; changed container IDs require a new digest after explicit recreation', () => {
  const input = fixture(); const first = plan(input);
  input.company.members.reverse(); input.adoption.members.reverse(); input.adoption.inventory.containers.reverse(); input.adoption.inventory.networks.reverse();
  assert.deepEqual(plan(input), first);
  input.adoption.members[0].containerId = 'a'.repeat(64);
  assert.throws(() => plan(input), /exact reviewed container/);
  input.adoption.inventory.containers.find(item => item.name === input.adoption.members[0].containerName).id = 'a'.repeat(64);
  assert.notEqual(plan(input).reviewSha256, first.reviewSha256);
});

test('a later explicitly named game app uses the same data-driven isolated adoption path', () => {
  const result = plan(fixture(['game']));
  assert.equal(result.adoption.attachments.length, 3);
  assert.ok(result.adoption.attachments.some(member => member.member === 'game'));
  assert.equal(result.compose.services.game_app, undefined);
  assert.equal(result.compose.networks.game_database, undefined);
  assert.deepEqual(result.posture.enrolledMembers, []);
});

test('missing, duplicate, invented, ambiguous and stale identity/network claims fail closed', () => {
  const changes = [
    input => { input.adoption.members.pop(); },
    input => { input.adoption.members[1] = { ...input.adoption.members[0] }; },
    input => { input.adoption.members[0].member = 'gateway'; },
    input => { input.adoption.members[0].containerId = 'bad'; },
    input => { input.adoption.members[0].containerName = 'not-in-inventory'; },
    input => { input.adoption.members[0].containerName = '--privileged'; },
    input => { input.adoption.inventory.containers[0].networks = []; },
    input => { input.adoption.inventory.containers[0].networks = ['absent']; },
    input => { input.adoption.inventory.containers[1].networks = [...input.adoption.inventory.containers[0].networks]; },
    input => { input.adoption.inventory.containers[1].id = input.adoption.inventory.containers[0].id; },
    input => { input.adoption.inventory.networks.push('codifix-company_workshop_federation'); },
    input => { input.adoption.inventory.networks.push('codifix-company_gateway_database'); },
    input => { input.adoption.inventory.volumes.push('codifix-company_gateway_db_data'); },
    input => { input.adoption.inventory.volumes.push('codifix-company_pds_data'); },
    input => { input.adoption.inventory.containers[0].id = [input.adoption.inventory.containers[0].id]; },
    input => { input.adoption.inventory.containers.push({ name: 'codifix-company-gateway-1', id: 'a'.repeat(64), networks: [] }); },
    input => { input.adoption.inventory.containers.push({ name: 'codifix-company_pds_1', id: 'a'.repeat(64), networks: [] }); },
    input => { input.adoption.members[0].containerName = 'gateway'; input.adoption.inventory.containers[0].name = 'gateway'; },
    input => { input.adoption.members[1].containerName = input.adoption.members[0].containerName; input.adoption.members[1].containerId = input.adoption.members[0].containerId; },
    input => { input.adoption.inventory.networks.push('bridge'); input.adoption.inventory.containers[0].networks = ['bridge']; },
    input => { input.adoption.execute = true; },
    input => { input.adoption.schemaVersion = 2; },
    input => { input.adoption.inventory.networks.push(input.adoption.inventory.networks[0]); },
    input => { input.adoption.members[0].network = 'existing-database-network'; },
    input => { input.adoption.members[0].providerKey = 'not-accepted'; },
  ];
  for (const change of changes) { const input = fixture(); change(input); assert.throws(() => plan(input)); }
});

test('explicit staged gateway DB stays outside Compose ownership and is reachable only by gateway', () => {
  const input = withPreparedDatabase(); const original = structuredClone(input); const result = plan(input);
  assert.deepEqual(input, original);
  assert.equal(result.compose.services.gateway_db, undefined);
  assert.deepEqual(Object.keys(result.compose.volumes), ['pds_data']);
  assert.deepEqual(result.compose.networks.gateway_database, { external: true, name: 'codifix-company_gateway_database' });
  assert.ok(result.requiredFiles.every(file => file.service !== 'gateway_db' && !file.path.endsWith('/gateway/database/owner-password')));
  assert.deepEqual(Object.entries(result.compose.services).filter(([, service]) => networksOf(service).includes('gateway_database')).map(([name]) => name), ['gateway']);
  for (const [name, network] of Object.entries(result.compose.networks)) if (name !== 'gateway_database') {
    assert.equal(network.external, undefined); assert.equal(network.internal, true); assert.equal(network.enable_ipv6, false);
  }
  const prepared = result.adoption.preparedGatewayDatabase;
  assert.equal(prepared.liveVerified, false); assert.equal(prepared.expectedInternal, true); assert.equal(prepared.expectedIpv6, false);
  assert.equal(prepared.expectedComposeProject, input.company.name);
  assert.deepEqual(prepared.expectedCurrentContainerIds, ['d'.repeat(64)]);
  assert.deepEqual(prepared.expectedNewServices, ['gateway']);
  assert.deepEqual(prepared.expectedPublishedPorts, []);
  assert.equal(prepared.expectedHostNetwork, false);
  assert.equal(prepared.managedByThisPlan, false);
  assert.equal(prepared.removeOrphansAllowed, false);
  assert.ok(result.adoption.attachments.every(item => item.network !== prepared.networkName));
  assert.ok(result.adoption.attachments.every(item => item.containerId !== prepared.containerId));
  assert.equal(result.posture.installed, false);
});

test('prepared DB exception rejects wrong identity, aliases, extra peers and broad resource reuse', () => {
  const changes = [
    input => { delete input.adoption.preparedGatewayDatabase; },
    input => { input.adoption.preparedGatewayDatabase = null; },
    input => { input.adoption.preparedGatewayDatabase.containerId = 'e'.repeat(64); },
    input => { input.adoption.preparedGatewayDatabase.containerName = input.adoption.members[0].containerName; },
    input => { input.adoption.preparedGatewayDatabase.networkName = 'codifix-workshop-preview'; },
    input => { input.adoption.preparedGatewayDatabase.alias = 'postgres'; },
    input => { input.adoption.preparedGatewayDatabase.allowAnyExistingNetwork = true; },
    input => { delete input.adoption.inventory.containers.at(-1).networkAliases; },
    input => { input.adoption.inventory.containers.at(-1).networkAliases.memory_company_other = ['gateway_db']; },
    input => { input.adoption.inventory.containers.at(-1).networkAliases['codifix-company_gateway_database'] = 'gateway_db'; },
    input => { input.adoption.inventory.containers.at(-1).networkAliases = null; },
    input => { input.adoption.inventory.containers.at(-1).networkAliases['codifix-company_gateway_database'].push('gateway_db'); },
    input => { input.adoption.inventory.containers.at(-1).networkAliases['codifix-company_gateway_database'] = ['other']; },
    input => { input.adoption.inventory.containers.at(-1).networks.push(input.adoption.inventory.networks[0]); },
    input => { input.adoption.inventory.containers[0].networks.push('codifix-company_gateway_database'); },
    input => { input.adoption.inventory.containers.push({ name: 'unexpected', id: 'e'.repeat(64), networks: ['codifix-company_gateway_database'] }); },
    input => { input.adoption.inventory.networks.push('codifix-company_pds_client'); },
    input => { input.adoption.inventory.networks.push('codifix-company_workshop_federation'); },
    input => { input.adoption.inventory.volumes.push('codifix-company_pds_data'); },
    input => { input.adoption.inventory.containers.push({ name: 'codifix-company-gateway-1', id: 'e'.repeat(64), networks: [] }); },
  ];
  for (const change of changes) { const input = withPreparedDatabase(); change(input); assert.throws(() => plan(input)); }
});

test('staged DB ID and alias inventory changes are review-bound, not silently reconciled', () => {
  const input = withPreparedDatabase(); const before = plan(input);
  input.adoption.preparedGatewayDatabase.containerId = 'e'.repeat(64);
  assert.throws(() => plan(input));
  input.adoption.inventory.containers.at(-1).id = 'e'.repeat(64);
  assert.notEqual(plan(input).reviewSha256, before.reviewSha256);
  const changed = plan(input);
  input.adoption.inventory.containers.at(-1).networkAliases['codifix-company_gateway_database'].reverse();
  assert.deepEqual(plan(input), changed);
  input.adoption.inventory.containers.at(-1).networkAliases['codifix-company_gateway_database'].push('another-reviewed-alias');
  assert.notEqual(plan(input).reviewSha256, changed.reviewSha256);
});

test('staged DB bind storage requires no named-volume adoption or storage declaration', () => {
  const input = withPreparedDatabase();
  input.adoption.inventory.volumes = [];
  const result = plan(input);
  assert.deepEqual(Object.keys(result.compose.volumes), ['pds_data']);
  assert.equal(result.compose.services.gateway_db, undefined);
  assert.ok(result.requiredFiles.every(file => !file.path.startsWith(`${input.company.root}/gateway/database/`)));
  assert.equal(result.adoption.preparedGatewayDatabase.managedByThisPlan, false);
});
