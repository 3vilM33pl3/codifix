// SPDX-License-Identifier: AGPL-3.0-or-later
// Pure, default-deny installation blueprint. No file writes, Docker commands,
// credentials, identities, memberships or knowledge consent are created here.
import { tlsProxy } from './topology.mjs';

const IMAGE_KEYS = ['app', 'gateway', 'postgres', 'pds', 'proxy'];
const RESERVED = new Set(['gateway', 'pds', 'postgres', 'proxy', 'edge', 'default', 'host', 'none', ...Object.getOwnPropertyNames(Object.prototype)]);
const pinnedImage = /^(?:[a-z0-9]+(?:[.-][a-z0-9]+)+)(?::[0-9]{1,5})?\/(?:[a-z0-9]+(?:[._-][a-z0-9]+)*\/)*[a-z0-9]+(?:[._-][a-z0-9]+)*(?::[A-Za-z0-9][A-Za-z0-9_.-]{0,127})?@sha256:[a-f0-9]{64}$/;
const label = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;

function strict(value, keys, name) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
    || ![Object.prototype, null].includes(Object.getPrototypeOf(value))
    || Object.keys(value).some(key => !keys.includes(key))) throw Error(`Invalid ${name}`);
}
function check(condition, message) { if (!condition) throw Error(message); }
function boolean(value, name) { check(value === undefined || typeof value === 'boolean', `Invalid ${name}`); return value ?? false; }
function user(value, name) {
  check(typeof value === 'string' && /^[1-9][0-9]{0,4}:[1-9][0-9]{0,4}$/.test(value)
    && value.split(':').every(number => Number(number) <= 65535), `Invalid ${name} user`);
  return value;
}

const lockedAppEnvironment = Object.freeze({
  HOME: '/state',
  XDG_CONFIG_HOME: '/state/config',
  XDG_DATA_HOME: '/state/data',
  XDG_CACHE_HOME: '/state/cache',
  XDG_STATE_HOME: '/state/local-state',
  CODIFIX_PROFILE: 'prod',
  CODIFIX__SERVICE__BIND_ADDR: '0.0.0.0:4040',
  CODIFIX__SERVICE__WEB_ROOT: '/opt/codifix/web',
  CODIFIX__DATABASE__MIGRATIONS: 'verify_only',
  CODIFIX__OFFLINE__ENABLED: 'false',
  CODIFIX__CLUSTER__ENABLED: 'false',
  CODIFIX__BACKGROUND_WORK__ENABLED: 'false',
  CODIFIX__BACKGROUND_WORK__INTERNAL_AUTOMATION: 'false',
  CODIFIX__AUTOMATION__ENABLED: 'false',
  CODIFIX__FEATURES__LLM_CURATION: 'false',
  CODIFIX__EMBEDDINGS__ENABLED: 'false',
  CODIFIX__EMBEDDINGS__CREATE_ENABLED: 'false',
  CODIFIX__CONSOLIDATION__ENABLED: 'false',
  CODIFIX__CONSOLIDATION__AUTO_TRIGGER: 'false',
  CODIFIX__REINFORCEMENT__VALIDATION_ENABLED: 'false',
  CODIFIX__PROVENANCE__REVERIFY_ENABLED: 'false',
  CODIFIX__TELEMETRY__ENABLED: 'false',
  CODIFIX__LLM_AUDIT__ENABLED: 'false',
  CODIFIX__MCP__ENABLED: 'false',
  CODIFIX__MCP__HTTP_ENABLED: 'false',
  // No provider credentials are mounted. Empty model plus a closed loopback
  // endpoint prevents an ordinary query from choosing an external fallback.
  CODIFIX__LLM__MODEL: '',
  CODIFIX__LLM__BASE_URL: 'http://127.0.0.1:9/v1',
  CODIFIX__LLM__API_KEY_ENV: 'CODIFIX_COMPANY_MODEL_DISABLED',
  CODIFIX_COMPANY_MODEL_DISABLED: '',
  OPENAI_API_KEY: '',
  ANTHROPIC_API_KEY: '',
});

/** Return { compose, proxyFiles, requiredFiles, endpoints, posture } for review.
 * Host files must be separately provisioned; short bind syntax is deliberately
 * avoided so Compose cannot silently create an absent secret as a directory. */
export function companyTopology(input) {
  strict(input, ['schemaVersion', 'name', 'root', 'domain', 'images', 'users', 'members', 'federationEnabled', 'expertiseRoutes'], 'company topology input');
  check(input.schemaVersion === 1, 'Unsupported company topology schema');
  check(typeof input.name === 'string' && label.test(input.name) && input.name.length <= 48 && !RESERVED.has(input.name), 'Invalid Compose project name');
  check(typeof input.root === 'string' && input.root.length <= 240 && /^\/(?:[A-Za-z0-9_.-]+\/)+[A-Za-z0-9_.-]+$/.test(input.root)
    && !input.root.split('/').some(part => part === '.' || part === '..'), 'An explicit dedicated absolute root is required');
  check(typeof input.domain === 'string' && input.domain.length <= 190 && input.domain.includes('.')
    && input.domain.split('.').every(part => part.length <= 63 && /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(part))
    && !/^[0-9.]+$/.test(input.domain), 'Invalid company DNS domain');
  strict(input.images, IMAGE_KEYS, 'image map');
  for (const key of IMAGE_KEYS) check(typeof input.images[key] === 'string' && pinnedImage.test(input.images[key]), `Digest-pinned fully qualified ${key} image required`);
  strict(input.users, IMAGE_KEYS, 'container user map');
  for (const key of IMAGE_KEYS) user(input.users[key], key);
  const members = input.members ?? ['workshop', 'homelab'];
  check(Array.isArray(members) && members.length >= 2 && members.length <= 8
    && members.every(id => typeof id === 'string' && id.length <= 32 && label.test(id) && !RESERVED.has(id))
    && new Set(members).size === members.length && members.includes('workshop') && members.includes('homelab'), 'Expected Workshop and Homelab plus at most six explicitly named members');
  const federationEnabled = boolean(input.federationEnabled, 'federationEnabled');
  const expertiseRoutes = boolean(input.expertiseRoutes, 'expertiseRoutes');
  check(!expertiseRoutes || federationEnabled, 'Expertise routing requires explicit federation opt-in');
  const ids = [...members].sort();
  const compose = { name: input.name, services: {}, networks: {}, volumes: {} };
  const proxyFiles = {};
  const requiredFiles = [];
  const endpoints = { gateway: `https://gateway.${input.domain}:8443`, pds: `https://pds.${input.domain}:8443`, members: {} };
  const root = input.root;
  const hardening = { read_only: true, cap_drop: ['ALL'], security_opt: ['no-new-privileges:true'], restart: 'no', pids_limit: 128 };
  const base = kind => ({ image: input.images[kind], user: input.users[kind], ...structuredClone(hardening), tmpfs: ['/tmp:rw,noexec,nosuid,nodev,mode=1777,size=64m'] });
  const network = name => { compose.networks[name] = { internal: true, enable_ipv6: false }; return name; };
  const volume = name => { compose.volumes[name] = {}; return { type: 'volume', source: name, target: '/var/lib/postgresql/data' }; };
  function bind(relative, target, scope, secret = false) {
    const source = `${root}/${relative}`;
    requiredFiles.push({ path: source, service: scope, target, secret, mode: secret ? '0600' : '0644' });
    return { type: 'bind', source, target, read_only: true, bind: { create_host_path: false } };
  }
  function proxy(name, relative, hostname, upstream, paths, networks) {
    const [uid, gid] = input.users.proxy.split(':');
    const file = `${relative}/Caddyfile`;
    proxyFiles[file] = tlsProxy(hostname, upstream, paths);
    compose.services[name] = { ...base('proxy'), entrypoint: ['/usr/bin/caddy'], command: ['run', '--config', '/config/Caddyfile', '--adapter', 'caddyfile'],
      environment: { XDG_CONFIG_HOME: '/config-data' },
      tmpfs: [`/data:uid=${uid},gid=${gid},mode=0700,size=16m`, `/config-data:uid=${uid},gid=${gid},mode=0700,size=16m`],
      volumes: [bind(file, '/config/Caddyfile', name), bind(`${relative}/cert.pem`, '/config/cert.pem', name), bind(`${relative}/key.pem`, '/config/key.pem', name, true)], networks };
  }
  function database(name, networkName, relative, databaseName) {
    compose.services[name] = { ...base('postgres'),
      environment: { POSTGRES_USER: 'memory_schema_owner', POSTGRES_DB: databaseName, POSTGRES_PASSWORD_FILE: '/run/secrets/owner-password', PGDATA: '/var/lib/postgresql/data/pgdata' },
      tmpfs: ['/tmp:rw,noexec,nosuid,nodev,mode=1777,size=64m', '/var/run/postgresql:rw,nosuid,nodev,mode=1777,size=16m'],
      volumes: [volume(`${name}_data`), bind(`${relative}/owner-password`, '/run/secrets/owner-password', name, true)], networks: [networkName] };
  }
  network('gateway_api'); network('gateway_database'); network('pds_client'); network('pds_repository');
  const gatewayMounts = [bind('gateway/gateway.json', '/config/gateway.json', 'gateway', true),
    bind('gateway/database-url', '/run/secrets/database-url', 'gateway', true),
    bind('gateway/pds-token', '/run/secrets/pds-token', 'gateway', true),
    bind('gateway/audit-key', '/run/secrets/audit-key', 'gateway', true),
    bind('gateway/authority-token', '/run/secrets/authority-token', 'gateway', true),
    bind('tls/ca.pem', '/run/trust/ca.pem', 'gateway')];
  const edgeNetworks = { gateway_api: {} };
  for (const id of ids) {
    const local = network(`${id}_app`);
    const db = network(`${id}_database`);
    const link = network(`${id}_federation`);
    const ingress = network(`${id}_ingress`);
    const app = `${id}_app`;
    const env = { ...lockedAppEnvironment };
    const mounts = [bind(`members/${id}/app.toml`, '/config/app.toml', app, true)];
    if (federationEnabled) {
      env.CODIFIX_FEDERATION_CONFIG = '/config/federation.json';
      mounts.push(bind(`members/${id}/federation.json`, '/config/federation.json', app, true),
        bind(`members/${id}/gateway-token`, '/run/secrets/gateway-token', app, true), bind('tls/ca.pem', '/run/trust/ca.pem', app));
    }
    compose.volumes[`${id}_app_state`] = {};
    mounts.push({ type: 'volume', source: `${id}_app_state`, target: '/state' });
    compose.services[app] = { ...base('app'), entrypoint: ['/usr/local/bin/codifix'], command: ['--config', '/config/app.toml', 'service', 'run'],
      environment: env, volumes: mounts, networks: [local, db, link] };
    database(`${id}_db`, db, `members/${id}/database`, `memory_${id.replaceAll('-', '_')}`);
    proxy(`${id}_edge`, `members/${id}/tls`, `${id}.${input.domain}`, `${app}:4040`, ['/', '/assets/*', '/v1/*', '/ws'], { [local]: {}, [ingress]: { aliases: [`${id}.${input.domain}`] } });
    edgeNetworks[link] = { aliases: [`gateway.${input.domain}`] };
    gatewayMounts.push(bind(`gateway/member-tokens/${id}`, `/run/secrets/member-${id}-token`, 'gateway', true));
    endpoints.members[id] = { url: `https://${id}.${input.domain}:8443`, ingressNetwork: `${input.name}_${ingress}`, database: `memory_${id.replaceAll('-', '_')}` };
  }
  compose.services.gateway = { ...base('gateway'), entrypoint: ['/usr/local/bin/codifix-federation-gateway'], command: ['/config/gateway.json'],
    volumes: gatewayMounts, networks: ['gateway_api', 'gateway_database', 'pds_client'] };
  database('gateway_db', 'gateway_database', 'gateway/database', 'memory_gateway');
  proxy('gateway_edge', 'gateway/tls', `gateway.${input.domain}`, 'gateway:4055', ['/v1/accept', '/v1/pull', '/v1/retention', '/v1/status', ...(expertiseRoutes ? ['/v1/expertise'] : [])], edgeNetworks);
  proxy('pds_tls', 'pds/tls', `pds.${input.domain}`, 'pds:2583', ['/xrpc/com.atproto.repo.createRecord', '/xrpc/com.atproto.repo.getRecord', '/xrpc/com.atproto.repo.deleteRecord', '/xrpc/com.atproto.sync.getRepo'],
    { pds_client: { aliases: [`pds.${input.domain}`] }, pds_repository: {} });
  compose.volumes.pds_data = {};
  const pdsEnv = `${root}/pds/pds.env`;
  requiredFiles.push({ path: pdsEnv, service: 'pds', target: 'PDS process environment only', secret: true, mode: '0600' });
  compose.services.pds = { ...base('pds'), env_file: [pdsEnv],
    environment: { PDS_PORT: '2583', PDS_HOSTNAME: `pds.${input.domain}`, PDS_DATA_DIRECTORY: '/pds', PDS_BLOBSTORE_DISK_LOCATION: '/pds/blobs', PDS_DEV_MODE: 'false', PDS_DISABLE_SSRF_PROTECTION: 'false', PDS_INVITE_REQUIRED: 'true', PDS_CRAWLERS: '' },
    volumes: [{ type: 'volume', source: 'pds_data', target: '/pds' }], networks: ['pds_repository'] };
  return { compose, proxyFiles, requiredFiles, endpoints,
    posture: { federationEnabled, expertiseRoutes, publicPorts: false, externalProviders: false, autonomousAgents: false,
      credentialsGenerated: false, enrolledMembers: [], approvedCollections: [], installed: false } };
}
