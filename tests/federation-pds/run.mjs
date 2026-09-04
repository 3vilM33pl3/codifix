// SPDX-License-Identifier: AGPL-3.0-or-later
// Generates only ephemeral fixture secrets/certificates; never uses user data.
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomBytes, createECDH } from 'node:crypto';
import { spawnSync } from 'node:child_process';

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
try {
  const cert = spawnSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048',
    '-nodes', '-days', '1', '-keyout', join(runtime, 'key.pem'),
    '-out', join(runtime, 'cert.pem'), '-subj', '/CN=Memory isolated PDS gate',
    '-addext', 'subjectAltName=DNS:space.federation.example.com,DNS:pds.federation.example.com'],
  { encoding: 'utf8' });
  if (cert.status !== 0) throw new Error('Cannot create disposable TLS certificate');
  // Marker enables safe identification of this disposable run, not secret storage.
  await writeFile(join(runtime, 'run-id'), project, { mode: 0o600 });
  started = true;
  const result = spawnSync('docker', [...compose, 'up', '--abort-on-container-exit',
    '--exit-code-from', 'probe'], { env, stdio: 'inherit', timeout: 180_000 });
  process.exitCode = result.status ?? 1;
} finally {
  if (started) {
    const cleanup = spawnSync('docker', [...compose, 'down', '--volumes'],
      { env, stdio: 'inherit', timeout: 60_000 });
    if (cleanup.status !== 0) {
      process.exitCode = 1;
      console.error(`Cleanup failed; inspect isolated Compose project ${project}`);
    }
  }
  await rm(runtime, { recursive: true, force: true });
}
