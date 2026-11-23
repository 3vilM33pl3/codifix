import { test } from 'node:test';
import assert from 'node:assert/strict';
import { companyTopology } from './company-topology.mjs';

function settings() {
  return { schemaVersion: 1, name: 'codifix-company', root: '/srv/codifix-company', domain: 'codifix.company.internal',
    images: Object.fromEntries(['app', 'gateway', 'postgres', 'pds', 'proxy'].map((key, index) => [key, `registry.company.internal/codifix/${key}@sha256:${String(index + 1).repeat(64)}`])),
    users: { app: '65532:65532', gateway: '65532:65532', proxy: '65532:65532', pds: '1000:1000', postgres: '999:999' } };
}
const networks = service => Array.isArray(service.networks) ? service.networks : Object.keys(service.networks);
const adjacent = (a, b) => networks(a).some(network => networks(b).includes(network));

test('default blueprint is offline, uninstalled, bounded and has no publication/model authority', () => {
  const result = companyTopology(settings());
  const doc = result.compose;
  assert.equal(Object.keys(doc.services).length, 11);
  for (const service of Object.values(doc.services)) {
    assert.match(service.image, /@sha256:[a-f0-9]{64}$/);
    assert.equal(service.ports, undefined);
    assert.equal(service.network_mode, undefined);
    assert.equal(service.privileged, undefined);
    assert.deepEqual(service.cap_drop, ['ALL']);
    assert.equal(service.read_only, true);
    assert.equal(service.restart, 'no');
    assert.equal(service.pids_limit, 128);
    assert.ok(!service.user.startsWith('0:'));
    for (const mount of service.volumes) {
      assert.ok(!mount.source.includes('docker.sock'));
      if (mount.type === 'bind') { assert.equal(mount.read_only, true); assert.equal(mount.bind.create_host_path, false); }
    }
  }
  for (const network of Object.values(doc.networks)) assert.deepEqual(network, { internal: true, enable_ipv6: false });
  assert.deepEqual(result.posture.enrolledMembers, []);
  assert.deepEqual(result.posture.approvedCollections, []);
  assert.equal(result.posture.installed, false);
  for (const id of ['workshop', 'homelab']) {
    const app = doc.services[`${id}_app`];
    assert.equal(app.environment.CODIFIX_FEDERATION_CONFIG, undefined);
    assert.equal(app.environment.CODIFIX__BACKGROUND_WORK__ENABLED, 'false');
    assert.equal(app.environment.CODIFIX__AUTOMATION__ENABLED, 'false');
    assert.equal(app.environment.CODIFIX__DATABASE__MIGRATIONS, 'verify_only');
    assert.equal(app.environment.CODIFIX__OFFLINE__ENABLED, 'false');
    assert.equal(app.environment.CODIFIX__LLM__MODEL, '');
    assert.equal(app.environment.OPENAI_API_KEY, '');
    assert.equal(app.env_file, undefined);
    assert.equal(app.environment.HOME, '/state');
    for (const key of ['XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME']) assert.ok(app.environment[key].startsWith('/state/'));
  }
  assert.ok(!result.proxyFiles['gateway/tls/Caddyfile'].includes('/v1/expertise'));
});

test('each member reaches only its own DB/UI and the TLS gateway, not peers or internal authority services', () => {
  const { compose: { services: s } } = companyTopology(settings());
  for (const [id, other] of [['workshop', 'homelab'], ['homelab', 'workshop']]) {
    const app = s[`${id}_app`];
    for (const allowed of [`${id}_db`, `${id}_edge`, 'gateway_edge']) assert.ok(adjacent(app, s[allowed]), allowed);
    for (const forbidden of [`${other}_app`, `${other}_db`, `${other}_edge`, 'gateway', 'gateway_db', 'pds_tls', 'pds']) assert.equal(adjacent(app, s[forbidden]), false, forbidden);
    assert.equal(adjacent(s[`${id}_db`], s.gateway_edge), false);
  }
  assert.ok(adjacent(s.gateway, s.gateway_db));
  assert.ok(adjacent(s.gateway, s.pds_tls));
  assert.equal(adjacent(s.gateway, s.pds), false);
  assert.ok(adjacent(s.pds_tls, s.pds));
  for (const forbidden of ['gateway_db', 'pds', 'pds_tls']) assert.equal(adjacent(s.gateway_edge, s[forbidden]), false);
});

test('explicit federation mounts only member-specific credentials; a third member is data not a code change', () => {
  const input = { ...settings(), members: ['workshop', 'homelab', 'game'], federationEnabled: true, expertiseRoutes: true };
  const result = companyTopology(input);
  const s = result.compose.services;
  assert.ok(s.game_app && s.game_db && s.game_edge);
  assert.ok(!adjacent(s.game_app, s.workshop_app));
  assert.ok(!adjacent(s.game_app, s.homelab_db));
  for (const id of input.members) {
    const mounts = s[`${id}_app`].volumes.filter(m => m.type === 'bind');
    assert.equal(s[`${id}_app`].environment.CODIFIX_FEDERATION_CONFIG, '/config/federation.json');
    assert.ok(mounts.some(m => m.source === `/srv/codifix-company/members/${id}/gateway-token`));
    assert.ok(mounts.every(m => m.source.startsWith(`/srv/codifix-company/members/${id}/`) || m.source === '/srv/codifix-company/tls/ca.pem'));
    assert.ok(!mounts.some(m => /audit-key|pds-token|owner-password|authority-token/.test(m.source)));
  }
  assert.ok(result.proxyFiles['gateway/tls/Caddyfile'].includes('/v1/expertise'));
  assert.ok(!result.proxyFiles['gateway/tls/Caddyfile'].includes('/xrpc'));
  assert.deepEqual(result.posture.approvedCollections, []);
  assert.deepEqual(result.posture.enrolledMembers, []);
  assert.deepEqual(companyTopology({ ...input, members: [...input.members].reverse() }), result);
});

test('TLS edges expose fixed paths only and private keys/passwords stay with their owning process', () => {
  const result = companyTopology(settings());
  const { services } = result.compose;
  assert.match(result.proxyFiles['gateway/tls/Caddyfile'], /@allowed path \/v1\/accept \/v1\/pull \/v1\/retention \/v1\/status\n/);
  assert.match(result.proxyFiles['gateway/tls/Caddyfile'], /reverse_proxy @allowed gateway:4055/);
  assert.match(result.proxyFiles['pds/tls/Caddyfile'], /@allowed path \/xrpc\/com.atproto.repo.createRecord \/xrpc\/com.atproto.repo.getRecord \/xrpc\/com.atproto.repo.deleteRecord \/xrpc\/com.atproto.sync.getRepo\n/);
  assert.match(result.proxyFiles['pds/tls/Caddyfile'], /reverse_proxy @allowed pds:2583/);
  for (const config of Object.values(result.proxyFiles)) {
    assert.ok(config.includes('admin off'));
    assert.ok(config.includes('respond 404'));
    assert.ok(!config.includes('tls_insecure_skip_verify'));
  }
  for (const id of ['workshop', 'homelab', 'gateway']) {
    const owned = result.requiredFiles.filter(file => file.path.endsWith(`/database/owner-password`) && file.service === `${id}_db`);
    assert.equal(owned.length, 1);
    assert.equal(owned[0].mode, '0600');
    assert.equal(result.requiredFiles.filter(file => file.path === owned[0].path).length, 1);
  }
  for (const file of result.requiredFiles.filter(file => file.path.endsWith('/key.pem'))) {
    assert.equal(file.secret, true);
    assert.equal(file.mode, '0600');
    assert.equal(result.requiredFiles.filter(other => other.path === file.path).length, 1);
  }
  assert.deepEqual(services.pds.env_file, ['/srv/codifix-company/pds/pds.env']);
  for (const [name, service] of Object.entries(services)) if (name !== 'pds') assert.equal(service.env_file, undefined);
  assert.equal(services.pds.environment.PDS_DEV_MODE, 'false');
  assert.equal(services.pds.environment.PDS_DISABLE_SSRF_PROTECTION, 'false');
  assert.equal(services.pds.environment.PDS_INVITE_REQUIRED, 'true');
});

test('strict inputs reject unpinned images, host escapes, injection, root users and authority shortcuts', () => {
  const bad = [
    { ...settings(), schemaVersion: 2 }, { ...settings(), publishPorts: [443] }, { ...settings(), credentials: {} },
    { ...settings(), root: '/' }, { ...settings(), root: '/srv/../etc' }, { ...settings(), root: '/srv/${HOME}' },
    { ...settings(), domain: 'https://example.com' }, { ...settings(), domain: '127.0.0.1' }, { ...settings(), domain: 'x\n reverse_proxy evil' },
    { ...settings(), name: 'company\nnetworks:' },
    { ...settings(), images: { ...settings().images, pds: 'ghcr.io/bluesky-social/pds:latest' } },
    { ...settings(), images: { ...settings().images, proxy: `registry.company.internal/proxy@sha256:${'g'.repeat(64)}` } },
    { ...settings(), images: { ...settings().images, extra: 'bad' } },
    { ...settings(), users: { ...settings().users, app: '0:0' } },
    { ...settings(), users: { ...settings().users, app: '65536:65536' } },
    { ...settings(), members: ['workshop', 'workshop'] }, { ...settings(), members: ['workshop', 'homelab', 'gateway'] },
    { ...settings(), members: ['workshop', 'homelab', '../pds'] }, { ...settings(), members: ['workshop', 'homelab', 'constructor'] },
    { ...settings(), members: ['workshop', 'homelab', ...Array.from({ length: 7 }, (_, n) => `member-${n}`)] },
    { ...settings(), federationEnabled: 'true' }, { ...settings(), expertiseRoutes: true },
  ];
  // Plain objects are used as service maps; inherited names must not become IDs.
  for (const value of bad) assert.throws(() => companyTopology(value));
});
