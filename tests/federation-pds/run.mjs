// SPDX-License-Identifier: AGPL-3.0-or-later
// Generates only ephemeral fixture secrets/certificates; never uses user data.
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomBytes, createECDH } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';

const directory = dirname(fileURLToPath(import.meta.url));
const runtime = await mkdtemp(join(tmpdir(), 'memory-private-pds-'));
const project = `memory-pds-gate-${randomBytes(6).toString('hex')}`;
const key = createECDH('secp256k1');
key.generateKeys();
const env = {
  ...process.env,
  GATE_RUNTIME_DIR: runtime,
  GATE_ADMIN_PASSWORD: randomBytes(32).toString('hex'),
  GATE_JWT_SECRET: randomBytes(32).toString('hex'),
  GATE_ROTATION_KEY: key.getPrivateKey('hex'),
};
const compose = ['compose', '-p', project, '-f', join(directory, 'compose.yaml')];
let started = false;
let cleaned = true;
try {
  const cert = spawnSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048',
    '-nodes', '-days', '1', '-keyout', join(runtime, 'key.pem'),
    '-out', join(runtime, 'cert.pem'), '-subj', '/CN=Memory isolated PDS gate',
    '-addext', 'subjectAltName=DNS:space.federation.example.com,DNS:pds.federation.example.com,DNS:recovery.federation.example.com'],
  { encoding: 'utf8' });
  if (cert.status !== 0) throw new Error('Cannot create disposable TLS certificate');
  // Marker enables safe identification of this disposable run, not secret storage.
  await writeFile(join(runtime, 'run-id'), project, { mode: 0o600 });
  const configResult = spawnSync('docker', [...compose, 'config', '--format', 'json'],
    { env, encoding: 'utf8' });
  if (configResult.status !== 0) throw new Error('Cannot validate isolated Compose configuration');
  const config = JSON.parse(configResult.stdout);
  if (Object.values(config.networks).some(network => network.internal !== true)
      || Object.values(config.services).some(service => service.ports?.length
        || service.network_mode || service.privileged)) {
    throw new Error('Gate must have only internal networks and no host ports or privileged services');
  }
  for (const name of ['pds', 'pds-recovery']) {
    const settings = config.services[name].environment;
    if (settings.PDS_DEV_MODE !== 'false' || settings.PDS_DISABLE_SSRF_PROTECTION !== 'false'
        || settings.PDS_INVITE_REQUIRED !== 'true' || settings.PDS_CRAWLERS !== '') {
      throw new Error('Unsafe upstream PDS fixture settings');
    }
  }
  console.log('PASS internal-only network, no host ports, production security flags and crawling disabled');
  started = true;
  const child = spawn('docker', [...compose, 'up', '--abort-on-container-exit',
    '--exit-code-from', 'probe'], { env, stdio: 'inherit' });
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
    process.exitCode = interrupted ? 1 : (code ?? 1);
  } finally {
    clearTimeout(timeout);
    process.off('SIGINT', stop);
    process.off('SIGTERM', stop);
  }
} finally {
  if (started) {
    const cleanup = spawnSync('docker', [...compose, 'down', '--volumes'],
      { env, stdio: 'inherit', timeout: 60_000 });
    if (cleanup.status !== 0) {
      cleaned = false;
      process.exitCode = 1;
      console.error(`Cleanup failed; inspect isolated Compose project ${project}; fixture credentials remain in ${runtime}`);
    }
  }
  if (cleaned) await rm(runtime, { recursive: true, force: true });
}
