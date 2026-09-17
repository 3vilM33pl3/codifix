import assert from 'node:assert/strict';
import test from 'node:test';
import { databaseEnvironment, parseArgs } from './snapshot.mjs';

test('database credentials remain in child environment, without inherited overrides', () => {
  const config = '[llm]\nurl = "https://example.invalid"\n[database]\nurl = "postgresql://owner:p%40ss@127.0.0.1:5433/source"\n[next]\nkey="unused"';
  const env = databaseEnvironment(config, { PGSERVICE: 'unsafe', PGOPTIONS: 'unsafe',
    PGHOSTADDR: '192.0.2.1', OPENAI_API_KEY: 'must-not-inherit', PATH: '/usr/bin' });
  assert.equal(env.PGDATABASE, 'source'); assert.equal(env.PGPASSWORD, 'p@ss');
  assert.equal(env.PGPORT, '5433'); assert.equal(env.PGSERVICE, undefined);
  assert.equal(env.PGOPTIONS, '-c default_transaction_read_only=on -c timezone=UTC');
  assert.equal(env.PGHOSTADDR, undefined); assert.equal(env.OPENAI_API_KEY, undefined);
  assert.equal(env.PATH, '/usr/bin');
});
test('final database section without a following section is supported', () => {
  assert.equal(databaseEnvironment('[database]\nurl="postgres://user:pw@localhost/db"\n', {}).PGDATABASE, 'db');
});
test('ambiguous bindings and connection query parameters fail closed', () => {
  for (const value of ['invalid', 'https://user:pw@host/db', 'postgres://user:pw@host/db?options=x', 'postgres://host/db'])
    assert.throws(() => databaseEnvironment(`[database]\nurl="${value}"`, {}));
  const one = '[database]\nurl="postgres://user:pw@localhost/db"\n';
  assert.throws(() => databaseEnvironment(one + one, {}));
  assert.throws(() => databaseEnvironment(one + 'url="postgres://user:pw@localhost/other"', {}));
});
test('backup needs a fresh absolute destination and full recipient fingerprint', () => {
  const args = ['--database-config', '/protected/config.toml', '--out', '/data/recovery/run',
    '--recipient', 'A'.repeat(40), '--file', '/protected/config.toml'];
  assert.equal(parseArgs(args).files.length, 1);
  assert.throws(() => parseArgs([...args, '--out', '/another']));
  assert.throws(() => parseArgs([...args, '--file', '/']));
  assert.throws(() => parseArgs(['--out', 'relative']));
});
