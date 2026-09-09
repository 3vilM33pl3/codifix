// SPDX-License-Identifier: AGPL-3.0-or-later
// Generates only ephemeral fixture secrets/certificates; never uses user data.
import { mkdtemp, mkdir, readFile, writeFile, rm, unlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomBytes, createECDH } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';

const directory = dirname(fileURLToPath(import.meta.url));
const runtime = await mkdtemp(join(tmpdir(), 'memory-private-pds-'));
const tlsDirectory = join(runtime, 'tls');
const stateDirectory = join(runtime, 'state');
const keyDirectory = join(runtime, 'key');
const project = `memory-pds-gate-${randomBytes(6).toString('hex')}`;
const key = createECDH('secp256k1');
key.generateKeys();
const env = {
  ...process.env,
  GATE_RUNTIME_DIR: runtime,
  GATE_HOST_UID: String(process.getuid()),
  GATE_HOST_GID: String(process.getgid()),
  GATE_ADMIN_PASSWORD: randomBytes(32).toString('hex'),
  GATE_JWT_SECRET: randomBytes(32).toString('hex'),
  GATE_ROTATION_KEY: key.getPrivateKey('hex'),
};
const compose = ['compose', '-p', project, '-f', join(directory, 'compose.yaml')];
const runtimeMode = process.argv.includes('--runtime');
function composeRun(args, environment = env) {
  const result = spawnSync('docker', [...compose, ...args],
    { env: environment, stdio: 'inherit', timeout: 60_000 });
  if (result.status !== 0) throw new Error(`Disposable Compose ${args[0]} failed`);
}

async function probe(services, environment = env) {
  const child = spawn('docker', [...compose, 'up', '--abort-on-container-exit',
    '--exit-code-from', 'probe', ...services, 'probe'], { env: environment, stdio: 'inherit' });
  let interrupted = false;
  const stop = () => { interrupted = true; child.kill('SIGTERM'); };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
  const timeout = setTimeout(stop, 180_000);
  try {
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', resolve);
    });
    if (interrupted || code !== 0) throw new Error('Disposable PDS probe failed or timed out');
  } finally {
    clearTimeout(timeout);
    process.off('SIGINT', stop);
    process.off('SIGTERM', stop);
  }
}

let started = false;
let cleaned = true;
async function runtimeTest() {
  if (!process.env.MEMORY_LAYER_TEST_DATABASE_URL) {
    throw new Error('--runtime requires an explicitly disposable test PostgreSQL URL with CREATE DATABASE rights');
  }
  const runtimeEnv = { ...env, GATE_PHASE: 'runtime' };
  composeRun(['up', '-d', 'pds', 'pds-recovery', 'probe'], runtimeEnv);
  for (let attempt = 0; ; attempt++) {
    try { await readFile(join(stateDirectory, 'runtime-ready')); break; }
    catch (error) {
      if (error.code !== 'ENOENT') throw new Error(`Cannot read private readiness marker: ${error.code}`);
      if (attempt % 10 === 0) {
        const status = spawnSync('docker', [...compose, 'ps', '-a', '--format', 'json', 'probe'],
          { env: runtimeEnv, encoding: 'utf8' });
        if (status.status !== 0) throw new Error('Cannot inspect provisioning status');
        const probe = status.stdout.trim() ? JSON.parse(status.stdout) : null;
        if (probe?.State === 'exited') {
          composeRun(['logs', 'probe'], runtimeEnv);
          throw new Error('PDS runtime provisioning probe exited before readiness');
        }
      }
      if (attempt >= 360) {
        composeRun(['logs', 'probe'], runtimeEnv);
        throw new Error('PDS runtime provisioning did not become ready within 180 seconds');
      }
      await new Promise(resolve => setTimeout(resolve, 500));
    }
  }
  const container = spawnSync('docker', [...compose, 'ps', '-q', 'pds-recovery'],
    { env: runtimeEnv, encoding: 'utf8' });
  if (container.status !== 0 || !/^[a-f0-9]+$/.test(container.stdout.trim())) throw new Error('Missing fixture PDS');
  const inspected = spawnSync('docker', ['inspect', container.stdout.trim()], { encoding: 'utf8' });
  if (inspected.status !== 0) throw new Error('Cannot inspect isolated fixture network');
  const network = JSON.parse(inspected.stdout)[0].NetworkSettings.Networks[`${project}_private`];
  if (!network || !/^[0-9.]+$/.test(network.IPAddress)) throw new Error('Missing private fixture address');
  const state = JSON.parse(await readFile(join(stateDirectory, 'recovery-state.json'), 'utf8'));
  const fixture = join(runtime, 'pds-fixture.json');
  await writeFile(fixture, JSON.stringify({ base: `http://${network.IPAddress}:2583`,
    repository: state.did, token: state.accessJwt, signing_key: state.signingKey }), { mode: 0o600 });
  const testEnv = { ...process.env, MEMORY_FEDERATION_PDS_FIXTURE: fixture, MEMORY_LAYER_TEST_REQUIRE_DB: '1' };
  delete testEnv.MEMORY_LAYER_CLIENT_TOKEN;
  delete testEnv.DATABASE_URL;
  delete testEnv.MEMORY_LAYER_FEDERATION_CONFIG;
  const child = spawn('cargo', ['test', '-p', 'mem-federation-runtime', '--test', 'c_workflow', '--locked', '--', '--nocapture'],
    { cwd: join(directory, '../..'), env: testEnv, stdio: 'inherit' });
  const timer = setTimeout(() => child.kill('SIGTERM'), 540_000);
  try {
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject); child.once('exit', resolve);
    });
    if (code !== 0) throw new Error('Rust-to-real-PDS workflow failed');
  } finally { clearTimeout(timer); }
  // Stage E exercises the local application contract over this SAME pinned PDS.
  // Optional client coverage requires a locally built CLI/web and Chromium.
  const app = spawn('cargo', ['test', '-p', 'mem-service', 'e_two_instance_http_workflow', '--locked', '--', '--test-threads=1', '--nocapture'],
    { cwd: join(directory, '../..'), env: testEnv, stdio: 'inherit' });
  const appTimer = setTimeout(() => app.kill('SIGTERM'), 540_000);
  try {
    const code = await new Promise((resolve, reject) => { app.once('error', reject); app.once('exit', resolve); });
    if (code !== 0) throw new Error('Two-instance Memory application workflow failed');
  } finally { clearTimeout(appTimer); }
  await writeFile(join(stateDirectory, 'runtime-finished'), 'complete', { mode: 0o600 });
  const probeId = spawnSync('docker', [...compose, 'ps', '-aq', 'probe'], { env: runtimeEnv, encoding: 'utf8' });
  const completed = spawnSync('docker', ['wait', probeId.stdout.trim()], { encoding: 'utf8', timeout: 60_000 });
  composeRun(['logs', 'probe'], runtimeEnv);
  if (completed.status !== 0 || completed.stdout.trim() !== '0') throw new Error('Independent PDS verification failed');
}
try {
  for (const path of [tlsDirectory, stateDirectory, keyDirectory]) await mkdir(path, { mode: 0o700 });
  await writeFile(join(keyDirectory, 'backup-key'), randomBytes(32), { mode: 0o600 });
  const cert = spawnSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048',
    '-nodes', '-days', '1', '-keyout', join(tlsDirectory, 'key.pem'),
    '-out', join(tlsDirectory, 'cert.pem'), '-subj', '/CN=Memory isolated PDS gate',
    '-addext', 'subjectAltName=DNS:space.federation.example.com,DNS:pds.federation.example.com,DNS:recovery.federation.example.com'],
  { encoding: 'utf8' });
  if (cert.status !== 0) throw new Error('Cannot create disposable TLS certificate');
  // Marker enables safe identification of this disposable run, not secret storage.
  await writeFile(join(runtime, 'run-id'), project, { mode: 0o600 });
  const configResult = spawnSync('docker', [...compose, '--profile', '*', 'config', '--format', 'json'],
    { env, encoding: 'utf8' });
  if (configResult.status !== 0) throw new Error('Cannot validate isolated Compose configuration');
  const config = JSON.parse(configResult.stdout);
  const upstream = JSON.parse(await readFile(join(directory, '../../fixtures/federation/upstream-lock.json'), 'utf8'));
  if (Object.values(config.services).some(service => service.image !== upstream.pds.image)) {
    throw new Error('Every PDS fixture image must match the reviewed upstream lock');
  }
  if (Object.values(config.networks).some(network => network.internal !== true)
      || Object.values(config.services).some(service => service.ports?.length
        || service.network_mode || service.privileged)) {
    throw new Error('Gate must have only internal networks and no host ports or privileged services');
  }
  for (const name of ['pds', 'pds-recovery', 'pds-restored']) {
    const settings = config.services[name].environment;
    if (settings.PDS_DEV_MODE !== 'false' || settings.PDS_DISABLE_SSRF_PROTECTION !== 'false'
        || settings.PDS_INVITE_REQUIRED !== 'true' || settings.PDS_CRAWLERS !== '') {
      throw new Error('Unsafe upstream PDS fixture settings');
    }
  }
  const sourceVolume = config.volumes.recovery.name;
  if (sourceVolume !== `${project}_recovery` || Object.values(config.volumes).some(v => v.external)) {
    throw new Error('Gate volume names must belong exclusively to this disposable run');
  }
  console.log('PASS internal-only network, no host ports, production security flags and crawling disabled');
  started = true;
  if (runtimeMode) {
    await runtimeTest();
  } else {
  await probe(['pds', 'pds-recovery']);
  // `up --abort-on-container-exit` has stopped both PDS writers before backup.
  composeRun(['run', '--rm', '--no-deps', 'backup', 'backup']);
  composeRun(['rm', '--force', 'pds-recovery']);
  const remove = spawnSync('docker', ['volume', 'rm', sourceVolume],
    { stdio: 'inherit', timeout: 30_000 });
  if (remove.status !== 0) throw new Error('Cannot remove the stopped disposable source volume');
  await unlink(join(stateDirectory, 'recovery-state.json'));
  console.log('PASS original stopped PDS volume and unencrypted seed state removed before recovery');
  composeRun(['run', '--rm', '--no-deps', 'restore', 'restore']);
  const restoredSecrets = JSON.parse(await readFile(join(stateDirectory, 'restored-secrets.json'), 'utf8'));
  const restoredEnvironment = { ...process.env, GATE_RUNTIME_DIR: runtime, GATE_PHASE: 'restore',
    GATE_HOST_UID: env.GATE_HOST_UID, GATE_HOST_GID: env.GATE_HOST_GID,
    ...restoredSecrets };
  await probe(['pds-restored'], restoredEnvironment);
  }
} finally {
  if (started) {
    const cleanup = spawnSync('docker', [...compose, '--profile', '*', 'down', '--volumes'],
      { env, stdio: 'inherit', timeout: 60_000 });
    if (cleanup.status !== 0) {
      cleaned = false;
      process.exitCode = 1;
      console.error(`Cleanup failed; inspect isolated Compose project ${project}; fixture credentials remain in ${runtime}`);
    }
  }
  if (cleaned) await rm(runtime, { recursive: true, force: true });
}
