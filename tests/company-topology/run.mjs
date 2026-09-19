// SPDX-License-Identifier: AGPL-3.0-or-later
// Explicit, disposable Linux Docker gate. No existing deployment is an input.
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { createHarness, CADDY_IMAGE, PDS_IMAGE } from './harness.mjs';

if (process.argv.slice(2).some(argument => argument !== '--with-pds')) throw Error('Usage: node tests/company-topology/run.mjs [--with-pds]');
const harness = await createHarness();
let completed = false;
async function liveBoundaryCheck() {
  for (const name of Object.keys(harness.result.compose.services)) {
    const info = await harness.inspect(name);
    assert.equal(info.HostConfig.Privileged, false);
    assert.notEqual(info.HostConfig.NetworkMode, 'host');
    assert.ok(Object.values(info.HostConfig.PortBindings ?? {}).every(binding => !binding));
    assert.ok(Object.values(info.NetworkSettings.Ports ?? {}).every(binding => !binding));
    assert.deepEqual(info.HostConfig.CapDrop, ['ALL']);
    assert.equal(info.HostConfig.ReadonlyRootfs, true);
    assert.equal(info.Config.User, `${process.getuid()}:${process.getgid()}`);
  }
  for (const name of Object.keys(harness.result.compose.networks)) {
    const network = JSON.parse(await harness.run('docker', ['network', 'inspect', `${harness.project}_${name}`]))[0];
    assert.equal(network.Labels['com.docker.compose.project'], harness.project);
    assert.equal(network.Internal, true); assert.equal(network.EnableIPv6, false);
  }
}
async function writeProbes() {
  const infos = {};
  for (const name of Object.keys(harness.result.compose.services)) infos[name] = await harness.inspect(name);
  const targets = (name, port) => [{ host: name, port }, ...Object.values(infos[name].NetworkSettings.Networks).map(n => ({ host: n.IPAddress, port }))];
  const endpoint = (name, network, port) => ({ host: infos[name].NetworkSettings.Networks[`${harness.project}_${network}`].IPAddress, port });
  const config = { gateway: harness.result.endpoints.gateway, publisher: harness.result.endpoints.pds,
    operator: harness.result.endpoints.operator.url, repository: `https://${harness.repositoryHostname}`,
    members: Object.fromEntries(Object.entries(harness.result.endpoints.members).map(([name, details]) => [name, details.url])),
    didDocument: harness.result.proxyFiles['bootstrap/repository/config/Caddyfile'].match(/respond @document `([\s\S]*)` 200\n/)[1].replaceAll('\\{', '{'),
    didDocumentSha256: harness.result.endpoints.repository.documentSha256,
    operatorGet: ['/xrpc/com.atproto.identity.getRecommendedDidCredentials', '/xrpc/com.atproto.server.getSession', '/xrpc/com.atproto.sync.getRepo'],
    operatorPost: ['/xrpc/com.atproto.server.createInviteCode', '/xrpc/com.atproto.server.createAccount', '/xrpc/com.atproto.server.activateAccount', '/xrpc/com.atproto.server.createSession', '/xrpc/com.atproto.server.refreshSession'], roles: {} };
  for (const [name, other] of [['workshop', 'homelab'], ['homelab', 'workshop']]) {
    config.roles[name] = { denied: [
      ...targets('gateway', 4055), ...targets('gateway_db', 5432), ...targets('pds', 2583), ...targets('pds_tls', 443),
      ...targets('pds_operator_tls', 8443), ...targets('repository_did', 443),
      ...targets(`${other}_app`, 4040), ...targets(`${other}_db`, 5432), ...targets(`${other}_edge`, 8443),
    ], allowed: [endpoint(`${name}_db`, `${name}_database`, 5432)],
    tlsResolve: { [`${name}.${harness.domain}`]: endpoint(`${name}_edge`, `${name}_app`, 8443).host } };
  }
  config.roles.gateway = { denied: [...targets('pds', 2583), ...targets('pds_operator_tls', 8443), ...targets('repository_did', 443),
    ...targets('workshop_db', 5432), ...targets('homelab_db', 5432)], allowed: [endpoint('gateway_db', 'gateway_database', 5432)] };
  config.roles.operator = { denied: [...targets('pds', 2583), ...targets('pds_tls', 443), ...targets('gateway', 4055), ...targets('gateway_db', 5432),
    ...targets('workshop_app', 4040), ...targets('homelab_app', 4040), ...targets('workshop_db', 5432), ...targets('homelab_db', 5432)],
  allowed: [endpoint('pds_operator_tls', 'pds_operator_ingress', 8443), endpoint('repository_did', 'pds_operator_ingress', 443)] };
  await harness.save(join(harness.root, 'fixture/probes.json'), config);
  return config;
}
const operatorProbe = () => harness.dc(['run', '--rm', '--no-deps', '--name', `${harness.project}-tls-probe`, 'operator_probe', '/fixture/probe.cjs', 'operator']);
try {
  console.log(`Disposable project: ${harness.project}`);
  await harness.prepare();
  console.log('PASS pinned Caddy accepts default PDS and all generated company/bootstrap TLS configurations');
  const config = await writeProbes();
  for (const [service, role] of [['workshop_app', 'workshop'], ['homelab_app', 'homelab'], ['gateway', 'gateway']]) console.log((await harness.dc(['exec', '-T', service, 'node', '/fixture/probe.cjs', role])).trim());
  console.log((await operatorProbe()).trim());
  await liveBoundaryCheck();
  console.log('PASS live inspection: no published/host listeners, all networks internal IPv4, non-root readonly/capability limits');
  const handoff = JSON.parse(config.didDocument);
  handoff.verificationMethod[0].publicKeyMultibase = `z${'b'.repeat(48)}`;
  await harness.replaceDidDocument(`${JSON.stringify(handoff)}\n`);
  await writeProbes();
  console.log((await operatorProbe()).trim());
  console.log('PASS explicit DID process recreation serves newly pinned compact bytes; prior pretty/newline revision also verified through TLS');
  if (process.argv.includes('--with-pds')) {
    const { rehearsePds } = await import('./pds-lifecycle.mjs');
    console.log(JSON.stringify(await rehearsePds(harness)));
    await liveBoundaryCheck();
  }
  console.log(JSON.stringify({ caddy: CADDY_IMAGE, nodeFixture: PDS_IMAGE, didTls: true, routes: true, networkBoundary: true,
    realPdsLifecycle: process.argv.includes('--with-pds'), customerData: false, paidCalls: false }));
  completed = true;
} finally {
  try {
    await harness.cleanup();
    console.log(`PASS cleanup: removed only ${harness.project} containers/networks, derived fixture image and ephemeral credentials`);
  } catch {
    process.exitCode = 1;
    console.error(`Cleanup incomplete: inspect exact disposable project ${harness.project}; protected temporary root retained at ${harness.root}`);
  }
}
if (!completed) process.exitCode = 1;
