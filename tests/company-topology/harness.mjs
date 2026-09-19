// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile, copyFile, rename, realpath, rm, chmod } from 'node:fs/promises';
import { randomBytes, createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { companyTopology } from '../../deploy/federation/company-topology.mjs';
import { companyBootstrapTopology } from '../../deploy/federation/company-bootstrap-topology.mjs';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const PDS_IMAGE = 'ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12';
export const CADDY_IMAGE = 'docker.io/library/caddy@sha256:226d1f059b75399fe19182893c7184591c07b97afc8dfcf44eeb80c9a77a530f';
const hash = value => createHash('sha256').update(value).digest('hex');
const networks = service => Array.isArray(service.networks) ? service.networks : Object.keys(service.networks);

export async function createHarness() {
  assert.ok(process.platform === 'linux' && process.getuid() > 0, 'Non-root Linux host user required');
  const root = await mkdtemp(join(tmpdir(), 'memory-company-rehearsal-'));
  const project = `memory-company-rehearsal-${randomBytes(6).toString('hex')}`;
  const proxyImage = `${project}-caddy:fixture`;
  const composeArgs = ['compose', '-p', project, '-f', join(root, 'compose.json')];
  // "repository" is reserved by the pinned PDS handle policy; use the same
  // non-reserved synthetic label as the existing private-PDS prerequisite gate.
  const domain = 'company.example.com', repositoryHostname = `space.${domain}`, operatorHostname = `operator.${domain}`;
  const user = `${process.getuid()}:${process.getgid()}`;
  const signal = new AbortController();
  const interrupt = () => signal.abort();
  process.on('SIGTERM', interrupt); process.on('SIGINT', interrupt);
  let built = false, touchedDocker = false, composeWritten = false, result, compose, company, bootstrap;
  const stateDir = join(root, 'rehearsal'), caFile = join(root, 'tls/ca.pem');
  async function run(command, args, { cleanup = false, timeout = 45000 } = {}) {
    return await new Promise((resolvePromise, reject) => {
      const child = spawn(command, args, { cwd: repository, stdio: ['ignore', 'pipe', 'pipe'], ...(cleanup ? {} : { signal: signal.signal }) });
      const chunks = []; let size = 0, diagnostics = '';
      const timer = setTimeout(() => child.kill('SIGKILL'), timeout);
      child.stdout.on('data', chunk => { size += chunk.length; if (size > 2 * 1024 * 1024) child.kill('SIGKILL'); else chunks.push(chunk); });
      // Do not print provider bodies, credentials, Compose-expanded environment or
      // arbitrary container logs, even in this synthetic harness.
      child.stderr.on('data', chunk => { if (diagnostics.length < 8192) diagnostics += chunk.toString('utf8'); });
      child.once('error', () => { clearTimeout(timer); reject(Error(`Disposable ${command} ${args[0]} could not run`)); });
      child.once('close', code => {
        clearTimeout(timer);
        if (code === 0) resolvePromise(Buffer.concat(chunks).toString('utf8'));
        else {
          let stage = diagnostics.match(/^FAIL (?:operator|workshop|homelab|gateway): [a-z_]+$/m)?.[0] ?? 'no public diagnostic';
          for (const line of diagnostics.split('\n')) {
            try {
              const item = JSON.parse(line);
              if (item.credentials_logged === false && /^[a-z_]{1,64}$/.test(item.error ?? '')
                && /^(?:prepare_(?:pinned_sdk|key_derivation|private_files|configuration|pds_environment)|(?:preview|execute)_(?:bootstrap|create|discover-key|handoff|activate|refresh)|invite_(?:readiness|issue|persist)|verify_(?:publish|readback|download|independent_sdk|wrong_key_rejection))$/.test(item.diagnostic ?? '')) {
                stage = `${item.diagnostic}: ${item.error}`;
                const xrpcErrors = ['InvalidRequest', 'InvalidToken', 'ExpiredToken', 'BadJwtSignature',
                  'InvalidInviteCode', 'AccountTakedown', 'HandleNotAvailable', 'UnsupportedDomain', 'IncompatibleDidDoc',
                  'InternalServerError', 'InvalidSignature', 'AuthRequired', 'InvalidEmail', 'InvalidHandle', 'RateLimitExceeded', 'unclassified'];
                if (Number.isSafeInteger(item.http_status) && item.http_status >= 400 && item.http_status <= 599
                  && xrpcErrors.includes(item.xrpc_error)
                  && ['unsupported_email', 'missing_invite', 'missing_email', 'unclassified'].includes(item.upstream_condition)) {
                  stage += ` HTTP${item.http_status} ${item.xrpc_error} ${item.upstream_condition}`;
                  if (['did_document', 'operator'].includes(item.upstream)) stage += ` ${item.upstream}`;
                }
              }
            } catch { /* Never print unclassified stderr. */ }
          }
          reject(Error(`Disposable ${command} ${args[0]} failed (${code}); ${stage}; project ${project}`));
        }
      });
    });
  }
  async function save(path, value) {
    assert.ok(path.startsWith(`${root}/`), 'Write must belong to this exact fixture root');
    await mkdir(dirname(path), { recursive: true, mode: 0o700 });
    await writeFile(path, typeof value === 'string' ? value : JSON.stringify(value), { mode: 0o600 });
  }
  const dc = args => run('docker', [...composeArgs, ...args]);
  const bind = (source, target, readOnly = true) => ({ type: 'bind', source, target, read_only: readOnly, bind: { create_host_path: false } });
  async function inspect(service) {
    assert.ok(compose.services[service], 'Unknown fixture service');
    const id = (await dc(['ps', '-aq', service])).trim();
    assert.match(id, /^[a-f0-9]{64}$/);
    const info = JSON.parse(await run('docker', ['inspect', id]))[0];
    assert.equal(info.Config.Labels['com.docker.compose.project'], project);
    assert.equal(info.Config.Labels['com.docker.compose.service'], service);
    assert.equal(info.State.Running, true, `${service} must be running`);
    return info;
  }
  async function certificate(directory, hostname) {
    await mkdir(join(root, directory), { recursive: true, mode: 0o700 });
    const prefix = join(root, directory);
    await run('openssl', ['req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', `${prefix}/key.pem`, '-out', `${prefix}/request.csr`, '-subj', `/CN=${hostname}`]);
    await save(`${prefix}/leaf.ext`, `basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:${hostname}\n`);
    await run('openssl', ['x509', '-req', '-in', `${prefix}/request.csr`, '-CA', caFile, '-CAkey', join(root, 'tls/private-key.pem'), '-CAserial', join(root, 'tls/serial'), '-CAcreateserial', '-out', `${prefix}/cert.pem`, '-days', '1', '-extfile', `${prefix}/leaf.ext`]);
  }
  async function prepare() {
    const context = (await run('docker', ['context', 'show'])).trim();
    const connection = JSON.parse(await run('docker', ['context', 'inspect', context]))[0];
    assert.ok(connection.Endpoints.docker.Host.startsWith('unix://') && !process.env.DOCKER_HOST, 'Local Docker socket context without host override required');
    assert.equal((await run('docker', ['ps', '-aq', '--filter', `label=com.docker.compose.project=${project}`])).trim(), '');
    for (const image of [PDS_IMAGE, CADDY_IMAGE]) await run('docker', ['image', 'inspect', image]); // No implicit pulls.
    await save(join(root, 'run-id'), project);
    await mkdir(join(root, 'tls'), { mode: 0o700 }); await mkdir(stateDir, { mode: 0o700 });
    await run('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-keyout', join(root, 'tls/private-key.pem'), '-out', caFile, '-subj', '/CN=Disposable company rehearsal CA', '-addext', 'basicConstraints=critical,CA:TRUE']);
    // OpenSSL honors the host umask (often 0002). The public trust root must not
    // be group writable; the real operator correctly rejects that condition.
    await chmod(caFile, 0o644);
    const did = `did:web:${repositoryHostname}`;
    const document = { '@context': ['https://www.w3.org/ns/did/v1'], id: did, alsoKnownAs: [`at://${repositoryHostname}`],
      verificationMethod: [{ id: `${did}#atproto`, type: 'Multikey', controller: did, publicKeyMultibase: `z${'a'.repeat(48)}` }],
      service: [{ id: `${did}#atproto_pds`, type: 'AtprotoPersonalDataServer', serviceEndpoint: `https://pds.${domain}` }] };
    const didDocument = `${JSON.stringify(document, null, 2)}\n`;
    company = { schemaVersion: 1, name: project, root, domain,
      images: { app: PDS_IMAGE, gateway: PDS_IMAGE, postgres: PDS_IMAGE, pds: PDS_IMAGE, proxy: CADDY_IMAGE },
      users: Object.fromEntries(['app', 'gateway', 'postgres', 'pds', 'proxy'].map(key => [key, user])) };
    bootstrap = { schemaVersion: 1, repositoryHostname, repositoryDid: did, accountHandle: repositoryHostname,
      operatorHostname, didDocument, didDocumentSha256: hash(didDocument), caCertificateSha256: hash(await readFile(caFile)) };
    result = companyBootstrapTopology(company, bootstrap);
    compose = structuredClone(result.compose);
    await mkdir(join(root, 'build'), { mode: 0o700 });
    await copyFile(join(repository, 'deploy/federation/Proxy.Dockerfile'), join(root, 'build/Dockerfile'));
    touchedDocker = true;
    await run('docker', ['build', '--network', 'none', '--pull=false', '-q', '-t', proxyImage, '--build-arg', `PROXY_IMAGE=${CADDY_IMAGE}`, join(root, 'build')]); built = true;
    for (const [path, content] of Object.entries(result.proxyFiles)) await save(join(root, path), content);
    await save(join(root, 'fixture/base-pds.Caddyfile'), companyTopology(company).proxyFiles['pds/tls/Caddyfile']);
    await copyFile(caFile, join(root, 'fixture/ca.pem'));
    for (const name of ['backend.cjs', 'probe.cjs']) await copyFile(join(repository, 'tests/company-topology', name), join(root, 'fixture', name));
    for (const [directory, hostname] of [
      ['members/workshop/tls', `workshop.${domain}`], ['members/homelab/tls', `homelab.${domain}`],
      ['gateway/tls', `gateway.${domain}`], ['pds/tls', `pds.${domain}`],
      ['bootstrap/operator/tls', operatorHostname], ['bootstrap/repository/tls', repositoryHostname],
    ]) await certificate(directory, hostname);
    const ports = { workshop_app: 4040, homelab_app: 4040, workshop_db: 5432, homelab_db: 5432, gateway: 4055, gateway_db: 5432, pds: 2583 };
    for (const [name, service] of Object.entries(compose.services)) {
      if (ports[name]) {
        service.image = PDS_IMAGE; service.entrypoint = ['node', '/fixture/backend.cjs']; service.command = [String(ports[name]), name];
        service.volumes = [bind(join(root, 'fixture'), '/fixture')]; service.environment = {}; delete service.env_file;
      } else service.image = proxyImage;
    }
    compose.services.pds_tls.volumes.push(bind(join(root, 'fixture/base-pds.Caddyfile'), '/fixture/base.Caddyfile'));
    // Trusted disposable maintenance runners; never part of the generated deployment.
    for (const [name, network] of [['operator_probe', 'pds_operator_ingress'], ['publisher_probe', 'pds_client']]) {
      compose.services[name] = { image: PDS_IMAGE, user, entrypoint: ['node'], profiles: ['maintenance'], networks: [network],
        read_only: true, cap_drop: ['ALL'], security_opt: ['no-new-privileges:true'], pids_limit: 128, restart: 'no',
        environment: { NODE_EXTRA_CA_CERTS: '/trust/ca.pem', HOME: '/tmp' }, tmpfs: ['/tmp:rw,noexec,nosuid,nodev,size=32m'],
        volumes: [bind(join(root, 'fixture'), '/fixture'), bind(join(repository, 'tests/company-topology'), '/tests'),
          bind(join(repository, 'deploy/federation/pds-enrollment'), '/operator'), bind(stateDir, '/rehearsal', false), bind(caFile, '/trust/ca.pem')] };
    }
    compose.volumes = {}; // Fake upstreams use no real application/DB/PDS volumes.
    await save(join(root, 'compose.json'), compose);
    composeWritten = true;
    const rendered = JSON.parse(await dc(['--profile', '*', 'config', '--format', 'json']));
    for (const service of Object.values(rendered.services)) assert.ok(!service.ports?.length && !service.privileged && !service.network_mode);
    for (const network of Object.values(rendered.networks)) assert.equal(network.internal, true);
    await dc(['up', '-d', '--no-build', '--pull', 'never']);
    await dc(['run', '--rm', '--no-deps', '--name', `${project}-base-config`, 'pds_tls', 'adapt', '--config', '/fixture/base.Caddyfile', '--adapter', 'caddyfile']);
    return result;
  }
  async function replaceDidDocument(bytes) {
    bootstrap = { ...bootstrap, didDocument: bytes, didDocumentSha256: hash(bytes) };
    const next = companyBootstrapTopology(company, bootstrap);
    const relativePath = 'bootstrap/repository/config/Caddyfile', destination = join(root, relativePath);
    await save(`${destination}.next`, next.proxyFiles[relativePath]); await rename(`${destination}.next`, destination);
    await dc(['up', '-d', '--no-build', '--pull', 'never', '--force-recreate', 'repository_did']);
    result = next;
  }
  async function ownedPath(path) {
    const actual = await realpath(path), suffix = relative(root, actual);
    assert.ok(suffix && !suffix.startsWith('..') && !suffix.startsWith('/'), 'Maintenance path must resolve inside this fixture root');
    return actual;
  }
  async function startRealPds(envFile, directory) {
    const template = structuredClone(result.compose.services.pds);
    template.env_file = [await ownedPath(envFile)];
    template.volumes = [bind(await ownedPath(directory), '/pds', false), ...template.volumes.filter(mount => mount.type === 'bind')];
    compose.services.pds = template;
    await save(join(root, 'compose.json'), compose);
    await dc(['up', '-d', '--no-build', '--pull', 'never', '--force-recreate', 'pds']);
  }
  async function cleanup() {
    process.off('SIGTERM', interrupt); process.off('SIGINT', interrupt);
    if (touchedDocker) {
      // The exact random project name and protected marker are checked before removal.
      assert.equal(await readFile(join(root, 'run-id'), 'utf8'), project);
      if (composeWritten) await run('docker', [...composeArgs, '--profile', '*', 'down', '--volumes', '--remove-orphans', '--timeout', '3'], { cleanup: true });
      assert.equal((await run('docker', ['ps', '-aq', '--filter', `label=com.docker.compose.project=${project}`], { cleanup: true })).trim(), '');
      assert.equal((await run('docker', ['network', 'ls', '-q', '--filter', `label=com.docker.compose.project=${project}`], { cleanup: true })).trim(), '');
      if (built) await run('docker', ['image', 'rm', proxyImage], { cleanup: true });
    }
    assert.ok(root.startsWith(join(tmpdir(), 'memory-company-rehearsal-')) && await realpath(root) === root);
    await rm(root, { recursive: true, force: false });
  }
  const maintenance = async (name, args) => {
    assert.ok(['prepare', 'phase', 'invite', 'publish-verify'].includes(args[0]), 'Known bounded maintenance operation required');
    let output;
    try { output = await dc(['run', '--rm', '--no-deps', '--name', `${project}-${name}`, name, '/tests/pds-lifecycle-helper.mjs', ...args]); }
    catch (error) { throw Error(`Disposable ${name} ${args[0]} failed; ${error.message}; no credentials logged`); }
    try { return JSON.parse(output); } catch { throw Error('Malformed disposable maintenance receipt'); }
  };
  return { root, project, domain, repositoryHostname, operatorHostname, stateDir, caFile, composeArgs,
    prepare, inspect, dc, save, run, cleanup, replaceDidDocument, startRealPds,
    runOperator: args => maintenance('operator_probe', args), runPublisher: args => maintenance('publisher_probe', args),
    get result() { return result; }, get compose() { return compose; }, networks };
}
