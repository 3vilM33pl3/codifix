// SPDX-License-Identifier: AGPL-3.0-or-later
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { randomBytes, createHash } = require('node:crypto');
const { seal, open } = require('./backup.cjs');

function fixture() {
  const archive = Buffer.from('disposable archive bytes');
  return { version: 1, archive: archive.toString('base64'),
    archiveSha256: createHash('sha256').update(archive).digest('hex'),
    configuration: { GATE_ADMIN_PASSWORD: 'test-only-not-a-real-secret' },
    fixture: { did: 'did:web:space.federation.example.com' } };
}

test('round-trip retains archive and credentials without plaintext in envelope', () => {
  const key = randomBytes(32);
  const payload = fixture();
  const envelope = seal(payload, key);
  assert.deepEqual(open(envelope, key).payload, payload);
  assert.ok(!JSON.stringify(envelope).includes(payload.configuration.GATE_ADMIN_PASSWORD));
  assert.notEqual(seal(payload, key).nonce, envelope.nonce);
});

test('wrong key, ciphertext, nonce, authentication tag and format all fail closed', () => {
  const key = randomBytes(32);
  const envelope = seal(fixture(), key);
  assert.throws(() => open(envelope, randomBytes(32)));
  for (const name of ['ciphertext', 'nonce', 'tag']) {
    const bytes = Buffer.from(envelope[name], 'base64');
    bytes[0] ^= 1;
    assert.throws(() => open({ ...envelope, [name]: bytes.toString('base64') }, key));
  }
  assert.throws(() => open({ ...envelope, format: 'another-format' }, key));
});

test('authenticated but inconsistent archive metadata is rejected', () => {
  const key = randomBytes(32);
  assert.throws(() => open(seal({ ...fixture(), archiveSha256: '0'.repeat(64) }, key), key));
  assert.throws(() => open(seal({ ...fixture(), version: 2 }, key), key));
});
