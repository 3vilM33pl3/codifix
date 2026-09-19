// SPDX-License-Identifier: AGPL-3.0-or-later
// Pure review artifact: never inspect Docker, write config or attach a container.
import { createHash } from 'node:crypto';
import { companyBootstrapTopology } from './company-bootstrap-topology.mjs';

const dockerName = /^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$/;
const containerId = /^[a-f0-9]{64}$/;
const networksOf = service => Array.isArray(service.networks) ? service.networks : Object.keys(service.networks);
const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
function check(condition, message) { if (!condition) throw Error(message); }
function strict(value, keys, name) {
  check(value && typeof value === 'object' && !Array.isArray(value)
    && [Object.prototype, null].includes(Object.getPrototypeOf(value))
    && Object.keys(value).length === keys.length && Object.keys(value).every(key => keys.includes(key)), `Invalid ${name}`);
}
function names(values, maximum, name) {
  check(Array.isArray(values) && values.length <= maximum && values.every(value => typeof value === 'string' && dockerName.test(value))
    && new Set(values).size === values.length, `Invalid ${name}`);
  return [...values].sort();
}
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
}
const sha256 = value => createHash('sha256').update(canonical(value)).digest('hex');

/** Regenerate the validated bootstrap graph, then replace every generated member
 * with an explicit, ID-pinned existing app. The supplied inventory is a review
 * input, not evidence that Docker currently matches it. Recreate apps for new
 * config first, then inspect and regenerate this plan with their new IDs. */
export function companyPreviewAdoption(company, bootstrap, adoption) {
  const result = companyBootstrapTopology(company, bootstrap);
  const preparedDatabase = adoption && Object.hasOwn(adoption, 'preparedGatewayDatabase');
  strict(adoption, ['schemaVersion', 'members', 'inventory', ...(preparedDatabase ? ['preparedGatewayDatabase'] : [])], 'adoption input');
  strict(adoption.inventory, ['containers', 'networks', 'volumes'], 'reviewed inventory');
  check(adoption.schemaVersion === 1, 'Unsupported adoption schema');
  const occupiedNetworks = names(adoption.inventory.networks, 512, 'existing network names');
  const occupiedVolumes = names(adoption.inventory.volumes, 512, 'existing volume names');
  check(Array.isArray(adoption.inventory.containers) && adoption.inventory.containers.length <= 256, 'Bounded container inventory required');
  const inventory = adoption.inventory.containers.map(item => {
    const suppliedAliases = item && Object.hasOwn(item, 'networkAliases');
    strict(item, ['name', 'id', 'networks', ...(suppliedAliases ? ['networkAliases'] : [])], 'existing container');
    check(typeof item.name === 'string' && dockerName.test(item.name) && typeof item.id === 'string' && containerId.test(item.id), 'Exact existing container name and ID required');
    const networks = names(item.networks, 32, 'existing container networks');
    check(networks.every(name => occupiedNetworks.includes(name)), 'Container network absent from reviewed inventory');
    const networkAliases = {};
    if (suppliedAliases) {
      check(item.networkAliases && typeof item.networkAliases === 'object' && !Array.isArray(item.networkAliases)
        && [Object.prototype, null].includes(Object.getPrototypeOf(item.networkAliases))
        && Object.keys(item.networkAliases).every(name => networks.includes(name)), 'Aliases require an existing container network');
      for (const name of Object.keys(item.networkAliases).sort()) networkAliases[name] = names(item.networkAliases[name], 32, 'network aliases');
    }
    return { name: item.name, id: item.id, networks, ...(suppliedAliases ? { networkAliases } : {}) };
  }).sort((a, b) => compare(a.name, b.name));
  check(new Set(inventory.map(item => item.name)).size === inventory.length
    && new Set(inventory.map(item => item.id)).size === inventory.length, 'Duplicate container name or ID');
  const ids = Object.keys(result.endpoints.members).sort();
  check(Array.isArray(adoption.members) && adoption.members.length === ids.length, 'Every generated member requires one explicit existing app');
  const members = adoption.members.map(item => {
    strict(item, ['member', 'containerName', 'containerId'], 'member adoption');
    check(ids.includes(item.member), 'Unknown member');
    const existing = inventory.find(entry => entry.name === item.containerName && entry.id === item.containerId);
    check(existing && existing.networks.length > 0, 'Existing app must match the exact reviewed container name and ID');
    check(!existing.networks.some(name => ['host', 'none', 'bridge'].includes(name)), 'Shared default or host networking is unsupported for adopted apps');
    return { member: item.member, containerName: existing.name, containerId: existing.id, currentNetworks: existing.networks };
  }).sort((a, b) => compare(a.member, b.member));
  check(new Set(members.map(item => item.member)).size === ids.length, 'Duplicate member');
  check(new Set(members.map(item => item.containerId)).size === ids.length, 'An existing app cannot represent multiple members');
  const currentNetworks = members.flatMap(item => item.currentNetworks);
  check(new Set(currentNetworks).size === currentNetworks.length, 'Adopted members must not already share a network');

  const { compose } = result;
  for (const member of ids) {
    for (const suffix of ['app', 'db', 'edge']) delete compose.services[`${member}_${suffix}`];
  }
  let preparedGatewayDatabase;
  if (preparedDatabase) {
    const requested = adoption.preparedGatewayDatabase;
    strict(requested, ['containerName', 'containerId', 'networkName', 'alias'], 'prepared gateway database');
    check(requested.containerName === `${compose.name}-gateway_db-1`
      && requested.networkName === `${compose.name}_gateway_database` && requested.alias === 'gateway_db', 'Only this stack\'s exact staged gateway database may be adopted');
    const existing = inventory.find(item => item.name === requested.containerName && item.id === requested.containerId);
    check(existing && existing.networks.length === 1 && existing.networks[0] === requested.networkName
      && existing.networkAliases?.[requested.networkName]?.includes(requested.alias), 'Staged database identity, sole network and alias must match inventory');
    check(!members.some(member => member.containerId === existing.id), 'A member cannot be the staged database');
    const occupants = inventory.filter(item => item.networks.includes(requested.networkName));
    check(occupants.length === 1 && occupants[0].id === existing.id, 'Only the staged database may occupy its existing network');
    delete compose.services.gateway_db;
    preparedGatewayDatabase = { ...requested, expectedComposeProject: compose.name, expectedComposeService: 'gateway_db',
      expectedComposeNetwork: 'gateway_database', expectedInternal: true, expectedIpv6: false,
      expectedCurrentContainerIds: [existing.id], expectedNewServices: ['gateway'], expectedPublishedPorts: [],
      expectedHostNetwork: false, managedByThisPlan: false, removeOrphansAllowed: false, liveVerified: false };
  }
  const usedNetworks = new Set(Object.values(compose.services).flatMap(networksOf));
  for (const name of Object.keys(compose.networks)) {
    if (!usedNetworks.has(name)) delete compose.networks[name];
    else {
      const explicit = `${compose.name}_${name}`;
      if (preparedGatewayDatabase && name === 'gateway_database') {
        compose.networks[name] = { external: true, name: explicit };
        check(Object.entries(compose.services).filter(([, service]) => networksOf(service).includes(name))
          .every(([service]) => service === 'gateway'), 'Only gateway may join the staged database network');
      } else {
        check(!occupiedNetworks.includes(explicit), 'New-stack network already exists; reuse requires a different reviewed project');
        compose.networks[name].name = explicit;
      }
    }
  }
  const existingNames = new Set(inventory.map(item => item.name));
  for (const service of Object.keys(compose.services)) {
    check(!existingNames.has(`${compose.name}-${service}-1`) && !existingNames.has(`${compose.name}_${service}_1`), 'New-stack container name collides with inventory');
    check(!members.some(item => item.containerName === service), 'Existing app name collides with an authority service');
  }
  const usedVolumes = new Set(Object.values(compose.services).flatMap(service => service.volumes ?? [])
    .filter(mount => mount.type === 'volume').map(mount => mount.source));
  for (const name of Object.keys(compose.volumes)) {
    if (!usedVolumes.has(name)) delete compose.volumes[name];
    else {
      const explicit = `${compose.name}_${name}`;
      check(!occupiedVolumes.includes(explicit), 'New-stack volume already exists; existing storage must not be reused');
      compose.volumes[name].name = explicit;
    }
  }
  result.requiredFiles = result.requiredFiles.filter(file => Object.hasOwn(compose.services, file.service));
  const binds = Object.values(compose.services).flatMap(service => service.volumes ?? []).filter(mount => mount.type === 'bind');
  for (const relative of Object.keys(result.proxyFiles)) {
    const absolute = `${company.root}/${relative}`;
    if (!binds.some(mount => absolute === mount.source || absolute.startsWith(`${mount.source}/`))) delete result.proxyFiles[relative];
  }
  // Existing UI URLs and databases are not supplied, inspected or replaced.
  result.endpoints.members = {};
  const attachments = members.map(member => {
    const key = `${member.member}_federation`;
    const adjacent = Object.entries(compose.services).filter(([, service]) => networksOf(service).includes(key)).map(([name]) => name);
    check(adjacent.length === 1 && adjacent[0] === 'gateway_edge', 'Member network may expose only gateway TLS');
    const network = compose.networks[key];
    check(network.internal === true && network.enable_ipv6 === false && !network.external, 'A fresh internal IPv4 member network is required');
    return { ...member, network: network.name, expectedComposeProject: compose.name, expectedComposeNetwork: key,
      expectedPeers: ['gateway_edge'], gateway: result.endpoints.gateway,
      command: ['docker', 'network', 'connect', network.name, member.containerId] };
  });
  result.posture.plannedAdoptedMembers = ids;
  result.posture.existingMemberModelPolicy = 'unchanged and not inspected';
  result.posture.installed = false;
  result.adoption = { schemaVersion: 1, inventorySha256: sha256({ containers: inventory, networks: occupiedNetworks, volumes: occupiedVolumes }),
    attachments, backupRequired: true, configRecreationRequired: true, postRecreationInventoryRequired: true,
    liveInventoryVerified: false, attachmentsExecuted: false, modelEgressChanged: false,
    ...(preparedGatewayDatabase ? { preparedGatewayDatabase } : {}) };
  return { ...result, reviewSha256: sha256(result) };
}
