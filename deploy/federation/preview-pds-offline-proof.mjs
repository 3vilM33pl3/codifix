// SPDX-License-Identifier: AGPL-3.0-or-later
// Executed only with the pinned PDS image, network none, read-only archived files.
import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const require = createRequire('/app/index.ts');
const pds = createRequire(require.resolve('@atproto/pds'));
const Database = pds('better-sqlite3');
const { BlockMap, blocksToCarFile, verifyRepoCar } = pds('@atproto/repo');
const { parseCid } = pds('@atproto/lex-data');
const { Secp256k1Keypair, verifySignature } = pds('@atproto/crypto');
const { cborBytesToRecord } = pds('@atproto/common');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const canonical = value => value === null || typeof value !== 'object' ? JSON.stringify(value)
  : Array.isArray(value) ? `[${value.map(canonical).join(',')}]`
    : `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
function publicationKey(envelope) {
  const namespace = Buffer.from(envelope.space.replaceAll('-', ''), 'hex'); assert.equal(namespace.length, 16);
  const bytes = createHash('sha1').update(namespace).update(`${envelope.publisher}:${envelope.record_id}`).digest().subarray(0, 16);
  bytes[6] = (bytes[6] & 15) | 80; bytes[8] = (bytes[8] & 63) | 128;
  const hex = bytes.toString('hex'); return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

async function proof() {
  const expected = JSON.parse(await readFile('/proof/expectation.json', 'utf8'));
  const journal = JSON.parse(await readFile('/enrollment/journal.json', 'utf8'));
  const config = JSON.parse(await readFile('/proof/pds-input-config.json', 'utf8'));
  assert.equal(expected.repository, config.repository); assert.equal(journal.signing_key, config.signing_key);
  // WAL-mode SQLite may need private sidecars even for reads. Work on a byte
  // copy in this network-none container's bounded tmpfs, never the archive.
  await writeFile('/tmp/actor.sqlite', await readFile(`/pds/${expected.actor_path}/store.sqlite`), { mode: 0o600, flag: 'wx' });
  const actor = new Database('/tmp/actor.sqlite', { fileMustExist: true });
  const blocks = new BlockMap();
  try {
    assert.equal(actor.pragma('integrity_check', { simple: true }), 'ok');
    const roots = actor.prepare('SELECT did,cid,rev FROM repo_root').all(); assert.equal(roots.length, 1);
    assert.equal(roots[0].did, expected.repository);
    for (const row of actor.prepare('SELECT cid,content,size FROM repo_block').all()) {
      assert.equal(row.content.length, row.size); blocks.set(parseCid(row.cid), row.content);
    }
    const privateKey = await readFile(`/pds/${expected.actor_path}/key`);
    const signer = await Secp256k1Keypair.import(privateKey); assert.equal(signer.did(), config.signing_key);
    assert.notEqual(signer.did(), config.bootstrap_public_key);
    const car = await blocksToCarFile(parseCid(roots[0].cid), blocks);
    const verified = await verifyRepoCar(car, expected.repository, config.signing_key);
    await assert.rejects(verifyRepoCar(car, expected.repository, config.bootstrap_public_key));
    const records = actor.prepare('SELECT rkey,cid,collection FROM record ORDER BY rkey').all();
    const keys = records.map(row => row.rkey);
    assert.deepEqual(keys, expected.deliveries.map(delivery => publicationKey(delivery.envelope)).sort());
    for (const record of records) {
      const decoded = cborBytesToRecord(blocks.get(parseCid(record.cid)));
      assert.equal(record.collection, 'dev.codifix.unspecced.publication');
      assert.equal(decoded.$type, record.collection);
      const delivery = expected.deliveries.find(value => publicationKey(value.envelope) === record.rkey);
      assert.ok(delivery); assert.equal(canonical(decoded.delivery), canonical(delivery));
    }
    assert.equal(verified.creates.length, keys.length);
    const receiptKeyBytes = Buffer.from((await readFile('/audit-key', 'utf8')).trim(), 'hex');
    assert.equal(receiptKeyBytes.length, 32);
    const receiptSigner = await Secp256k1Keypair.import(receiptKeyBytes);
    assert.equal(receiptSigner.did(), expected.receipt_keys.at(-1).public_key);
    let previous = null;
    for (const [index, receipt] of expected.audit_receipts.entries()) {
      assert.equal(receipt.claims.sequence, index + 1);
      assert.equal(receipt.claims.space, expected.space); assert.equal(receipt.claims.authority, expected.authority);
      assert.deepEqual(receipt.claims.previous, previous);
      const trusted = expected.receipt_keys[receipt.claims.key_generation - 1]; assert.ok(trusted);
      const message = Buffer.concat([Buffer.from('codifix-private-gateway-attestation-v1\0'), Buffer.from(canonical(receipt.claims))]);
      assert.equal(await verifySignature(trusted.public_key, message, Uint8Array.from(receipt.signature)), true);
      assert.equal(await verifySignature(trusted.public_key, Buffer.from(canonical(receipt.claims)), Uint8Array.from(receipt.signature)), false);
      previous = { algorithm: 'sha256', hex: hash(canonical(receipt)) };
    }
    return { signed_repository_verified: true, wrong_bootstrap_key_rejected: true,
      actor_private_key_matches_enrolled_key: true, audit_private_key_matches_trust: true,
      audit_receipts_verified: expected.audit_receipts.length, audit_domain_separation_verified: true,
      published_record_ids_match: true, published_record_contents_match: true, records: keys.length, blocks: blocks.size, car_sha256: hash(car),
      car_bytes: car.length, repository_commit: roots[0].cid, pds_started: false, network: 'none' };
  } finally { actor.close(); }
}
proof().then(result => process.stdout.write(`${JSON.stringify(result)}\n`)).catch(error => {
  // Only the fixed assertion location is emitted, never database rows/key bytes.
  const line = error.stack?.match(/(?:preview-pds-offline-proof|operator)\.mjs:(\d+):/)?.[1] ?? 'unknown';
  process.stderr.write(`offline_pds_proof_failed_at_${line}\n`); process.exitCode = 1;
});
