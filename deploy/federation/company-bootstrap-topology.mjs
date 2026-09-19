// SPDX-License-Identifier: AGPL-3.0-or-later
// Optional, pure operator prerequisite overlay. Never provisions or starts PDS.
import { createHash } from 'node:crypto';
import { companyTopology } from './company-topology.mjs';

const POST_PATHS = Object.freeze([
  '/xrpc/com.atproto.server.createInviteCode',
  '/xrpc/com.atproto.server.createAccount',
  '/xrpc/com.atproto.server.activateAccount',
  '/xrpc/com.atproto.server.createSession',
  '/xrpc/com.atproto.server.refreshSession',
]);
const GET_PATHS = Object.freeze([
  '/xrpc/com.atproto.identity.getRecommendedDidCredentials',
  '/xrpc/com.atproto.server.getSession',
  '/xrpc/com.atproto.sync.getRepo',
]);
const sha256 = text => createHash('sha256').update(text, 'utf8').digest('hex');
const hexDigest = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
function check(condition, message) { if (!condition) throw Error(message); }
function strict(value, keys, name) {
  check(value && typeof value === 'object' && !Array.isArray(value)
    && [Object.prototype, null].includes(Object.getPrototypeOf(value))
    && Object.keys(value).length === keys.length
    && Object.keys(value).every(key => keys.includes(key)), `Invalid ${name}`);
}
function hostname(value) {
  return typeof value === 'string' && value.length <= 190 && value.includes('.')
    && !/^[0-9.]+$/.test(value) && value.split('.').every(part => part.length <= 63
      && /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(part));
}

// Accept ordinary JSON whitespace, but reject duplicate keys and noncanonical
// string escapes. Otherwise JSON.parse could discard data still served publicly.
function compact(text) {
  let quoted = false, escaped = false, output = '';
  for (const character of text) {
    if (quoted) {
      output += character;
      if (escaped) escaped = false;
      else if (character === '\\') escaped = true;
      else if (character === '"') quoted = false;
    } else if (character === '"') { quoted = true; output += character; }
    else if (!' \t\r\n'.includes(character)) output += character;
  }
  return output;
}
function validateDocument(bootstrap, publisherOrigin) {
  const text = bootstrap.didDocument;
  check(typeof text === 'string' && Buffer.byteLength(text, 'utf8') <= 16 * 1024
    && hexDigest(bootstrap.didDocumentSha256) && sha256(text) === bootstrap.didDocumentSha256,
  'Reviewed DID document bytes and SHA256 must match');
  let doc;
  try { doc = JSON.parse(text); } catch { throw Error('Invalid public DID document JSON'); }
  check(compact(text) === JSON.stringify(doc), 'Ambiguous or noncanonical DID document JSON');
  strict(doc, ['@context', 'id', 'alsoKnownAs', 'verificationMethod', 'service'], 'public DID document');
  check(JSON.stringify(doc['@context']) === '["https://www.w3.org/ns/did/v1"]'
    && doc.id === bootstrap.repositoryDid
    && JSON.stringify(doc.alsoKnownAs) === JSON.stringify([`at://${bootstrap.accountHandle}`]), 'DID identity or handle mismatch');
  check(Array.isArray(doc.verificationMethod) && doc.verificationMethod.length === 1, 'Exactly one reviewed public signing key is required');
  const key = doc.verificationMethod[0];
  strict(key, ['id', 'type', 'controller', 'publicKeyMultibase'], 'public signing key');
  check(key.id === `${doc.id}#atproto` && key.controller === doc.id && key.type === 'Multikey'
    && typeof key.publicKeyMultibase === 'string' && /^z[1-9A-HJ-NP-Za-km-z]{40,128}$/.test(key.publicKeyMultibase), 'Invalid public signing key binding');
  check(Array.isArray(doc.service) && doc.service.length === 1, 'Exactly one PDS service is required');
  strict(doc.service[0], ['id', 'type', 'serviceEndpoint'], 'PDS service');
  check(doc.service[0].id === `${doc.id}#atproto_pds` && doc.service[0].type === 'AtprotoPersonalDataServer'
    && doc.service[0].serviceEndpoint === publisherOrigin, 'DID service must name the normal publisher origin');
  // Whitelisted string values contain no Caddy delimiters or placeholders. This
  // also rejects raw backticks that could end the literal response token.
  check(!text.includes('`') && !text.includes('\\'), 'Unsupported DID document escaping');
  return text;
}

const globalOptions = '{\n admin off\n auto_https off\n servers {\n protocols h1 h2\n }\n}\n';
function operatorProxy(host) {
  return `${globalOptions}https://${host}:8443 {\n tls /tls/cert.pem /tls/key.pem\n @read {\n method GET\n path ${GET_PATHS.join(' ')}\n }\n @write {\n method POST\n path ${POST_PATHS.join(' ')}\n }\n route {\n reverse_proxy @read pds:2583\n reverse_proxy @write pds:2583\n respond 404\n }\n}\n`;
}
function didHost(host, bytes) {
  // Backtick tokens preserve JSON byte-for-byte. Escape opening braces to stop
  // Caddy's runtime placeholder expansion; it removes precisely this backslash.
  // https://caddyserver.com/docs/caddyfile/concepts#placeholders
  const literal = bytes.replaceAll('{', '\\{');
  return `${globalOptions}https://${host}:443 {\n tls /tls/cert.pem /tls/key.pem\n @document {\n method GET\n path /.well-known/did.json\n }\n route {\n header @document Content-Type application/did+json\n header @document Cache-Control no-store\n respond @document \`${literal}\` 200\n respond 404\n }\n}\n`;
}

/** Return a new company blueprint plus isolated operator/DID prerequisites.
 * Exact public document bytes, identities and CA digest must already be reviewed.
 * This function performs no filesystem, key-generation, Docker or network I/O. */
export function companyBootstrapTopology(company, bootstrap) {
  const result = companyTopology(company);
  strict(bootstrap, ['schemaVersion', 'operatorHostname', 'repositoryHostname', 'repositoryDid', 'accountHandle',
    'didDocument', 'didDocumentSha256', 'caCertificateSha256'], 'bootstrap input');
  check(bootstrap.schemaVersion === 1, 'Unsupported bootstrap schema');
  check(hostname(bootstrap.operatorHostname) && hostname(bootstrap.repositoryHostname) && hostname(bootstrap.accountHandle), 'Invalid explicit operator/repository hostname or account handle');
  const reservedHosts = [`gateway.${company.domain}`, `pds.${company.domain}`,
    ...Object.keys(result.endpoints.members).map(id => `${id}.${company.domain}`)];
  check(bootstrap.operatorHostname !== bootstrap.repositoryHostname
    && !reservedHosts.includes(bootstrap.operatorHostname) && !reservedHosts.includes(bootstrap.repositoryHostname), 'Operator and repository origins must be distinct from existing edges');
  check(bootstrap.repositoryDid === `did:web:${bootstrap.repositoryHostname}`, 'Explicit repository DID must match the HTTPS443 hostname');
  check(bootstrap.accountHandle === bootstrap.repositoryHostname && bootstrap.accountHandle.endsWith(`.${company.domain}`), 'The reviewed repository handle must belong to the company namespace');
  check(hexDigest(bootstrap.caCertificateSha256), 'Reviewed CA SHA256 is required');
  // Pinned upstream @atproto/pds 0.5.27 derives publicUrl=https://${hostname}.
  // There is no PDS_PUBLIC_URL override. Keep this adjustment in the opt-in
  // overlay and preserve the publisher's exact route allowlist and networks.
  const previousOrigin = result.endpoints.pds;
  result.endpoints.pds = `https://pds.${company.domain}`;
  check(result.proxyFiles['pds/tls/Caddyfile'].includes(`${previousOrigin} {`), 'Unsupported publisher proxy template');
  result.proxyFiles['pds/tls/Caddyfile'] = result.proxyFiles['pds/tls/Caddyfile'].replace(`${previousOrigin} {`, `${result.endpoints.pds}:443 {`);
  result.compose.services.pds_tls.sysctls = { 'net.ipv4.ip_unprivileged_port_start': '443' };
  const bytes = validateDocument(bootstrap, result.endpoints.pds);
  const { compose, requiredFiles, proxyFiles } = result;
  for (const name of ['pds_operator_ingress', 'pds_operator_upstream', 'repository_identity']) {
    compose.networks[name] = { internal: true, enable_ipv6: false };
  }
  function bind(relative, target, service, secret = false, directory = false) {
    const source = `${company.root}/${relative}`;
    requiredFiles.push({ path: source, target, service, secret, mode: secret ? '0600' : directory ? '0755' : '0644', ...(directory ? { kind: 'directory' } : {}) });
    return { type: 'bind', source, target, read_only: true, bind: { create_host_path: false } };
  }
  function edge(service, directory, config, networks) {
    const template = structuredClone(compose.services.pds_tls);
    delete template.sysctls;
    const configPath = `${directory}/config/Caddyfile`;
    proxyFiles[configPath] = config;
    // Mount only this generated-config directory, never the operator's private
    // state or parent root. Atomic Caddyfile replacement cannot leave a file bind
    // pinned to an old inode; a deliberate process recreation loads the revision.
    template.volumes = [bind(`${directory}/config`, '/config', service, false, true),
      bind(`${directory}/tls/cert.pem`, '/tls/cert.pem', service),
      bind(`${directory}/tls/key.pem`, '/tls/key.pem', service, true)];
    template.networks = networks;
    compose.services[service] = template;
  }
  edge('pds_operator_tls', 'bootstrap/operator', operatorProxy(bootstrap.operatorHostname), {
    pds_operator_ingress: { aliases: [bootstrap.operatorHostname] }, pds_operator_upstream: {},
  });
  edge('repository_did', 'bootstrap/repository', didHost(bootstrap.repositoryHostname, bytes), {
    repository_identity: { aliases: [bootstrap.repositoryHostname] },
    pds_operator_ingress: { aliases: [bootstrap.repositoryHostname] },
  });
  // Per-container network namespace only; never grant root or NET_BIND_SERVICE.
  compose.services.repository_did.sysctls = { 'net.ipv4.ip_unprivileged_port_start': '443' };
  compose.services.pds.networks.push('pds_operator_upstream', 'repository_identity');
  compose.services.pds.environment.NODE_EXTRA_CA_CERTS = '/run/trust/repository-ca.pem';
  compose.services.pds.environment.PDS_SERVICE_HANDLE_DOMAINS = `.${company.domain}`;
  compose.services.pds.volumes.push(bind('tls/ca.pem', '/run/trust/repository-ca.pem', 'pds'));
  requiredFiles.at(-1).sha256 = bootstrap.caCertificateSha256;
  result.endpoints.operator = { url: `https://${bootstrap.operatorHostname}:8443`, network: `${company.name}_pds_operator_ingress` };
  result.endpoints.repository = { did: bootstrap.repositoryDid, url: `https://${bootstrap.repositoryHostname}/.well-known/did.json`, documentSha256: bootstrap.didDocumentSha256 };
  result.posture.operatorPrerequisites = true;
  result.posture.accountProvisioned = false;
  return result;
}
