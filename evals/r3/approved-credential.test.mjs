// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { withApprovedEvaluationKey } from './approved-credential.mjs';

async function fixture(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'r3-credential-'));
  t.after(() => fs.rm(root, { recursive: true }));
  const log = path.join(root, 'events.jsonl'), reportFile = path.join(root, 'report.json');
  const binary = async (name, source) => {
    const file = path.join(root, name);
    await fs.writeFile(file, `#!${process.execPath}\n` + source, { mode: 0o700 });
    return file;
  };
  const record = name => `require('node:fs').appendFileSync(${JSON.stringify(log)}, JSON.stringify({name:'${name}',args:process.argv.slice(2),env:process.env})+${JSON.stringify('\n')});\n`;
  const report = { version: 1, manifest_sha256: 'a'.repeat(64), split: 'development', work_items: 75,
    limits: { requests: 100, tokens: 10000, microusd: 1000000, task_seconds: 60 }, approvals_valid: true, paid_execution_enabled: false };
  await fs.writeFile(reportFile, JSON.stringify(report));
  const options = {
    memoryBinary: await binary('memory', record('gate') + `process.stdout.write(require('node:fs').readFileSync(${JSON.stringify(reportFile)}));`),
    baoBinary: await binary('bao', record('secret') + `process.stdout.write(${JSON.stringify('synthetic-dedicated-key\n')});`),
    manifest: path.join(root, 'campaign.json'), approvals: ['labels.json', 'spend.json'].map(file => path.join(root, file)),
    split: 'development', manifestSha256: report.manifest_sha256,
    secret: { address: 'https://secrets.example.test', mount: 'secret', path: 'memory-layer/r3-evaluation', field: 'OPENAI_API_KEY', version: 1 },
  };
  return { root, options, report, reportFile, binary, record,
    events: async () => (await fs.readFile(log, 'utf8').catch(() => '')).trim().split('\n').filter(Boolean).map(JSON.parse) };
}

test('exact approval precedes a version-pinned secret read and is rechecked before use', async t => {
  const f = await fixture(t);
  const inherited = ['OPENAI_API_KEY', 'BAO_ADDR', 'BAO_SKIP_VERIFY', 'HTTPS_PROXY', 'NODE_OPTIONS'];
  const previous = Object.fromEntries(inherited.map(key => [key, process.env[key]]));
  for (const key of inherited) process.env[key] = 'synthetic-ambient-must-not-inherit';
  t.after(() => {
    for (const key of inherited) {
      if (previous[key] === undefined) delete process.env[key];
      else process.env[key] = previous[key];
    }
  });
  let used = 0;
  const result = await withApprovedEvaluationKey(f.options, key => {
    used++; assert.equal(key, 'synthetic-dedicated-key'); return key;
  });
  assert.equal(used, 1);
  assert.deepEqual(result, { manifest_sha256: 'a'.repeat(64), credential_used: true });
  const events = await f.events();
  assert.deepEqual(events.map(e => e.name), ['gate', 'secret', 'gate']);
  assert.deepEqual(events[0].args, ['eval', 'campaign', 'check-approvals', '--manifest', f.options.manifest,
    '--split', 'development', '--approval', f.options.approvals[0], '--approval', f.options.approvals[1]]);
  assert.deepEqual(events[1].args, ['kv', 'get', '-address=https://secrets.example.test', '-tls-skip-verify=false',
    '-mount=secret', '-version=1', '-field=OPENAI_API_KEY', 'memory-layer/r3-evaluation']);
  assert.equal(events[1].env.BAO_SKIP_VERIFY, 'false');
  assert.equal(events[1].env.BAO_DISABLE_REDIRECTS, 'true');
  for (const event of events) assert.equal(event.env.OPENAI_API_KEY, undefined);
  for (const key of ['BAO_ADDR', 'HTTPS_PROXY', 'NODE_OPTIONS']) {
    for (const event of events) assert.equal(event.env[key], undefined);
  }
});

test('missing or changed approvals never read the secret; command failures remain sanitized', async t => {
  for (const mode of ['deny', 'hash', 'split', 'execution', 'malformed']) {
    const f = await fixture(t);
    if (mode === 'deny') f.options.memoryBinary = await f.binary('denied', f.record('gate') +
      `process.stderr.write('SECRET-ERROR-BODY');process.exit(1);`);
    else {
      if (mode === 'hash') f.report.manifest_sha256 = 'b'.repeat(64);
      if (mode === 'split') f.report.split = 'held_out';
      if (mode === 'execution') f.report.paid_execution_enabled = true;
      await fs.writeFile(f.reportFile, mode === 'malformed' ? 'SECRET-ERROR-BODY' : JSON.stringify(f.report));
    }
    await assert.rejects(withApprovedEvaluationKey(f.options, () => assert.fail('must not use key')),
      error => !error.message.includes('SECRET') && !error.cause);
    assert.deepEqual((await f.events()).map(e => e.name), ['gate']);
  }
});

test('mid-read changes, malformed keys and secret-store failures do not reach consumers', async t => {
  for (const mode of ['change', 'malformed', 'failure', 'oversized']) {
    const f = await fixture(t);
    const effect = mode === 'change'
      ? `require('node:fs').writeFileSync(${JSON.stringify(f.reportFile)}, '{}');process.stdout.write('synthetic-key');`
      : mode === 'oversized' ? `process.stdout.write('SECRET'.repeat(1000));`
        : mode === 'malformed' ? `process.stdout.write(${JSON.stringify('key\nextra')});`
        : `process.stdout.write('SECRET-ERROR-BODY');process.stderr.write('SECRET-ERROR-BODY');process.exit(1);`;
    f.options.baoBinary = await f.binary('bad-bao', f.record('secret') + effect);
    await assert.rejects(withApprovedEvaluationKey(f.options, () => assert.fail('must not use key')),
      error => !error.message.includes('SECRET') && !error.cause);
  }
});

const memoryBinary = process.env.CODIFIX_R3_CLI_BIN;
if (process.env.CODIFIX_R3_REQUIRE_REVIEW === '1' && !memoryBinary) throw Error('required real CLI missing');
test('real CLI denies a missing campaign before the fake secret process starts', { skip: !memoryBinary }, async t => {
  const f = await fixture(t);
  f.options.memoryBinary = memoryBinary;
  await assert.rejects(withApprovedEvaluationKey(f.options, () => assert.fail('must not use key')), /preflight failed/);
  assert.deepEqual(await f.events(), []);
});

test('no ambient credential fallback, unsafe destination, version ambiguity or callback leakage', async t => {
  const f = await fixture(t);
  for (const change of [o => { o.secret.address = 'http://secrets.example.test'; },
    o => { o.secret.address = 'https://user:pass@secrets.example.test'; },
    o => { o.secret.address += '?token=secret'; }, o => { o.secret.version = 0; },
    o => { o.secret.path = '../other'; }, o => { o.secret.field = '-flag'; },
    o => { o.approvals = []; }, o => { o.baoBinary = 'bao'; }]) {
    const options = structuredClone(f.options); change(options);
    await assert.rejects(withApprovedEvaluationKey(options, () => assert.fail('must not use key')));
  }
  assert.deepEqual(await f.events(), []);
  await assert.rejects(withApprovedEvaluationKey(f.options, key => { throw Error(key); }),
    error => error.message === 'evaluation credential consumer failed; no automatic retry' && !error.cause);
});
