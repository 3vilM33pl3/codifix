// SPDX-License-Identifier: AGPL-3.0-or-later
// Disposable-fixture cold backup. Not a production backup service or key store.
const assert = require('node:assert/strict');
const { createCipheriv, createDecipheriv, createHash, randomBytes } = require('node:crypto');
const { readFileSync, writeFileSync, readdirSync, chownSync } = require('node:fs');
const { spawnSync } = require('node:child_process');

const FORMAT = 'memory-pds-prerequisite-backup-v1';
const MAX_BYTES = 64 * 1024 * 1024;

function seal(payload, key) {
  assert.equal(key.length, 32);
  const nonce = randomBytes(12);
  const cipher = createCipheriv('aes-256-gcm', key, nonce);
  cipher.setAAD(Buffer.from(FORMAT));
  const ciphertext = Buffer.concat([cipher.update(JSON.stringify(payload)), cipher.final()]);
  return { format: FORMAT, nonce: nonce.toString('base64'),
    tag: cipher.getAuthTag().toString('base64'), ciphertext: ciphertext.toString('base64') };
}

function open(envelope, key) {
  assert.equal(envelope.format, FORMAT);
  assert.equal(key.length, 32);
  const nonce = Buffer.from(envelope.nonce, 'base64');
  const tag = Buffer.from(envelope.tag, 'base64');
  assert.equal(nonce.length, 12);
  assert.equal(tag.length, 16);
  const ciphertext = Buffer.from(envelope.ciphertext, 'base64');
  assert.ok(ciphertext.length <= MAX_BYTES, 'Fixture archive exceeds size limit');
  const decipher = createDecipheriv('aes-256-gcm', key, nonce);
  decipher.setAAD(Buffer.from(FORMAT));
  decipher.setAuthTag(tag);
  // Authenticate the entire envelope BEFORE parsing or extracting any bytes.
  const plaintext = Buffer.concat([decipher.update(ciphertext), decipher.final()]);
  const payload = JSON.parse(plaintext.toString('utf8'));
  assert.equal(payload.version, 1);
  const archive = Buffer.from(payload.archive, 'base64');
  assert.equal(createHash('sha256').update(archive).digest('hex'), payload.archiveSha256);
  return { payload, archive };
}

function tar(args, input) {
  const result = spawnSync('tar', args, { input, maxBuffer: MAX_BYTES });
  if (result.status !== 0) throw new Error('Disposable backup tar operation failed');
  return result.stdout;
}

function run(mode) {
  const key = readFileSync('/gate-key/backup-key');
  if (mode === 'backup') {
    const archive = tar(['-C', '/source', '-cf', '-', '.']);
    const payload = { version: 1, archive: archive.toString('base64'),
      archiveSha256: createHash('sha256').update(archive).digest('hex'),
      fixture: JSON.parse(readFileSync('/gate-state/recovery-state.json', 'utf8')),
      configuration: Object.fromEntries(['GATE_JWT_SECRET', 'GATE_ADMIN_PASSWORD', 'GATE_ROTATION_KEY']
        .map(name => { assert.ok(process.env[name]); return [name, process.env[name]]; })),
    };
    writeFileSync('/gate-state/backup.json', JSON.stringify(seal(payload, key)), { mode: 0o600 });
    console.log('PASS encrypted cold backup includes PDS volume, account fixture and runtime credentials');
  } else if (mode === 'restore') {
    assert.deepEqual(readdirSync('/restore'), [], 'Restore target must be a fresh disposable volume');
    const envelope = JSON.parse(readFileSync('/gate-state/backup.json', 'utf8'));
    const corrupted = { ...envelope, tag: Buffer.alloc(16).toString('base64') };
    assert.throws(() => open(corrupted, key));
    assert.throws(() => open(envelope, randomBytes(32)));
    assert.deepEqual(readdirSync('/restore'), []);
    console.log('PASS corrupt archive and wrong key rejected before restore writes');
    const { payload, archive } = open(envelope, key);
    // Only archives produced from this runner's isolated volume are accepted.
    // This helper is deliberately not an untrusted/general-purpose tar importer.
    tar(['-C', '/restore', '-xf', '-'], archive);
    writeFileSync('/gate-state/restored-state.json', JSON.stringify(payload.fixture), { mode: 0o600 });
    writeFileSync('/gate-state/restored-secrets.json', JSON.stringify(payload.configuration), { mode: 0o600 });
    const uid = Number(process.env.GATE_HOST_UID);
    const gid = Number(process.env.GATE_HOST_GID);
    assert.ok(Number.isInteger(uid) && uid >= 0 && Number.isInteger(gid) && gid >= 0);
    // Keep secrets 0600, but let the invoking host process read the recovered
    // configuration. Never make it world-readable to work around Docker ownership.
    chownSync('/gate-state/restored-secrets.json', uid, gid);
    console.log('PASS authenticated archive restored into a new PDS volume');
  } else {
    throw new Error('Expected backup or restore mode');
  }
}

module.exports = { seal, open };
if (require.main === module) {
  try { run(process.argv[2]); }
  catch { console.error('Disposable PDS backup/restore failed; no secret details logged'); process.exitCode = 1; }
}
