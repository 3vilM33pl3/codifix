// SPDX-License-Identifier: AGPL-3.0-or-later
// Explicit dependency-overlay checks; install upstream/ with the pinned pnpm first.
import assert from 'node:assert/strict';
import { createECDH, createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { createRequire } from 'node:module';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { Readable } from 'node:stream';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { dispatch } from './http-stdio.mjs';

const require = createRequire(process.env.CODIFIX_PDS_TEST_ROOT
  ? path.join(path.resolve(process.env.CODIFIX_PDS_TEST_ROOT), 'package.json')
  : new URL('./upstream/package.json', import.meta.url));
const importDependency = name => import(pathToFileURL(require.resolve(name)));
const pdsRoot = path.dirname(path.dirname(require.resolve('@atproto/pds')));
const importPds = name => import(pathToFileURL(path.join(pdsRoot, 'dist', name)));
const KeyEncoderModule = require('key-encoder');
const KeyEncoder = KeyEncoderModule.default ?? KeyEncoderModule;

test('pinned PDS and vulnerable dependency removal are real runtime properties', () => {
  assert.equal(require(path.join(pdsRoot, 'package.json')).version, '0.5.27');
  const fromEncoder = createRequire(require.resolve('key-encoder'));
  assert.throws(() => fromEncoder.resolve('elliptic'), { code: 'MODULE_NOT_FOUND' });
  assert.match(require('node:fs').readFileSync(path.join(path.dirname(require.resolve('key-encoder')), 'key-encoder.js'), 'utf8'), /createECDH/);
});

test('key encoding preserves secp256k1 public/private bytes and signed verification', async () => {
  const encoder = new KeyEncoder('secp256k1');
  const { createPublicKeyObject } = await importPds('auth-verifier.js');
  const { SignJWT, jwtVerify } = await importDependency('jose');
  for (const privateHex of ['1'.padStart(64, '0'), 'a7'.repeat(32)]) {
    const ecdh = createECDH('secp256k1');
    ecdh.setPrivateKey(Buffer.from(privateHex, 'hex'));
    const privatePem = encoder.encodePrivate(privateHex, 'raw', 'pem');
    const privateKey = createPrivateKey(privatePem);
    assert.equal(encoder.encodePrivate(privatePem, 'pem', 'raw'), privateHex);
    const expected = createPublicKey(privateKey).export({ format: 'jwk' });
    for (const format of ['compressed', 'uncompressed']) {
      const raw = ecdh.getPublicKey('hex', format);
      const pem = encoder.encodePublic(raw, 'raw', 'pem');
      const publicKey = createPublicKeyObject(raw);
      assert.deepEqual(publicKey.export({ format: 'jwk' }), expected);
      assert.equal(encoder.encodePublic(pem, 'pem', 'raw'), raw);
      assert.equal(encoder.encodePublic(Buffer.from(encoder.encodePublic(raw, 'raw', 'der'), 'hex'), 'der', 'raw'), raw);
      const message = Buffer.from('synthetic PDS key compatibility');
      assert.equal(verify('sha256', message, publicKey, sign('sha256', message, privateKey)), true);
      const jwt = await new SignJWT({ sub: 'did:example:synthetic' })
        .setProtectedHeader({ alg: 'ES256K' }).sign(privateKey);
      assert.equal((await jwtVerify(jwt, publicKey, { algorithms: ['ES256K'] })).payload.sub, 'did:example:synthetic');
      const tampered = `${jwt.slice(0, jwt.lastIndexOf('.') + 1)}${Buffer.alloc(64).toString('base64url')}`;
      await assert.rejects(jwtVerify(tampered, publicKey, { algorithms: ['ES256K'] }));
    }
  }
  for (const invalid of ['0'.repeat(64), 'f'.repeat(64), 'not-hex', '1', 'aa'.repeat(33)]) {
    assert.throws(() => encoder.encodePrivate(invalid, 'raw', 'pem'));
  }
  const jwt = await new SignJWT({ sub: 'synthetic' }).setProtectedHeader({ alg: 'ES256K' })
    .sign(createPrivateKey(encoder.encodePrivate('1'.padStart(64, '0'), 'raw', 'pem')));
  for (const invalid of ['', '00', '04' + '00'.repeat(64), 'not-hex']) {
    await assert.rejects(async () => jwtVerify(jwt, createPublicKeyObject(invalid), { algorithms: ['ES256K'] }));
  }
});

test('patched file-type API accepts Node streams and PDS blob module loads', async () => {
  await importPds('actor-store/blob/transactor.js');
  const { fileTypeFromStream } = await importDependency('file-type');
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j8qkAAAAASUVORK5CYII=', 'base64');
  assert.equal((await fileTypeFromStream(Readable.from([png], { objectMode: false }))).mime, 'image/png');
  assert.equal(await fileTypeFromStream(Readable.from([Buffer.from('ordinary text')], { objectMode: false })), undefined);
});

test('nodemailer upgrade retains PDS template and html-to-text delivery without network', async () => {
  const nodemailer = require('nodemailer');
  const { ServerMailer } = await importPds('mailer/index.js');
  const transport = nodemailer.createTransport({ streamTransport: true, buffer: true, newline: 'unix' });
  const mailer = new ServerMailer(transport, { fromAddress: 'test@example.invalid' }, { name: 'Synthetic' });
  const result = await mailer.sendTemplate('resetPassword', { token: 'synthetic-only' }, {
    to: 'recipient@example.invalid', subject: 'Synthetic reset',
  });
  assert.match(result.message.toString(), /recipient@example.invalid/);
  assert.match(result.message.toString(), /text\/plain/);
  assert.match(result.message.toString(), /text\/html/);
});

test('security overlay initializes real PDS databases and serves private HTTP without a listener', async () => {
  const { PDS } = await importDependency('@atproto/pds');
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'codifix-pds-security-test-'));
  let pds;
  try {
    pds = await PDS.fromEnv({ hostname: 'pds.example.invalid',
      dataDirectory: root, blobstoreDiskLocation: path.join(root, 'blobs'),
      jwtSecret: 'synthetic-jwt-secret-not-for-production', adminPassword: 'synthetic-admin-only',
      plcRotationKeyK256PrivateKeyHex: '1'.padStart(64, '0'),
      inviteRequired: true, devMode: false, disableSsrfProtection: false,
      serviceHandleDomains: ['.example.invalid'], crawlers: [],
      didPlcUrl: 'https://plc.example.invalid' });
    const response = await dispatch(pds.app, Buffer.from('GET /xrpc/com.atproto.server.describeServer HTTP/1.1\r\nHost: pds.example.invalid\r\nConnection: close\r\n\r\n'));
    assert.match(response.toString(), /^HTTP\/1\.1 200 /);
    assert.match(response.toString(), /did:web:pds.example.invalid/);
    const denied = await dispatch(pds.app, Buffer.from('POST /xrpc/com.atproto.repo.createRecord HTTP/1.1\r\nHost: pds.example.invalid\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}'));
    assert.match(denied.toString(), /^HTTP\/1\.1 401 /);
  } finally {
    await pds?.destroy();
    await fs.rm(root, { recursive: true, force: true });
  }
});
