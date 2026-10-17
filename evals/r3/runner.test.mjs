// SPDX-License-Identifier: AGPL-3.0-or-later
// Driver protocol/recovery verification only. Synthetic isolation attestations
// do not establish container isolation or Memory quality.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawn } from 'node:child_process';

const binary = process.env.CODIFIX_R3_CLI_BIN;
if (process.env.CODIFIX_R3_REQUIRE_RUNNER === '1' && !binary) throw Error('required campaign binary missing');
test('real campaign run and explicit resume preserve dispatch count, results and ambiguous attempts', { skip: !binary || process.platform === 'win32' }, async t => {
  assert(path.isAbsolute(binary));
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-runner-cli-'));
  t.after(() => fs.rm(root, { recursive: true }));
  const pack = path.join(root, 'pack');
  const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: root, CODIFIX_PROFILE: 'prod' };
  const invoke = (program, args) => new Promise((resolve, reject) => {
    const child = spawn(program, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    const timer = setTimeout(() => child.kill('SIGKILL'), 600000);
    child.stdout.on('data', b => { stdout += b; if (stdout.length > 8 * 1024 * 1024) child.kill('SIGKILL'); });
    child.stderr.on('data', b => { stderr += b; if (stderr.length > 8 * 1024 * 1024) child.kill('SIGKILL'); });
    child.on('error', e => { clearTimeout(timer); reject(e); });
    child.on('close', status => { clearTimeout(timer); resolve({ status, stdout, stderr }); });
  });
  const ok = result => { assert.equal(result.status, 0, result.stderr); return result; };
  const write = (file, value) => fs.writeFile(file, JSON.stringify(value));
  const read = async file => JSON.parse(await fs.readFile(file, 'utf8'));
  const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
  ok(await invoke(process.execPath, ['evals/r3/build-review-pack.mjs', pack]));
  const manifestPath = path.join(pack, 'campaign.json');
  const manifest = await read(manifestPath);
  const driver = path.join(pack, 'driver.sh');
  const script = '#!/bin/sh\nset -eu\nprintf x >> ../dispatch-count\ncp ../../driver-observations/"$(basename "$PWD")".json observation.json\nprintf 0 > preparation-microusd.json\n';
  await fs.writeFile(driver, script, { mode: 0o700 });
  manifest.assets.push({ path: 'driver.sh', sha256: hash(script), kind: 'configuration' });
  await write(manifestPath, manifest);
  const cli = args => invoke(binary, ['eval', 'campaign', ...args, '--manifest', manifestPath]);
  const plan = JSON.parse(ok(await cli(['plan', '--split', 'development'])).stdout);
  assert.equal(plan.work.length, 660);
  const prepared = path.join(pack, 'driver-observations');
  await fs.mkdir(prepared);
  const configuration = manifest.assets.find(a => a.path === 'configuration.json').sha256;
  for (const [i, work] of plan.work.entries()) {
    await write(path.join(prepared, `work-${String(i).padStart(5, '0')}.json`), {
      work, model: manifest.model,
      result: { item_id: work.case_id, eval_type: 'synthetic-dispatch-test', condition: 'full-memory', metadata: {},
        success: false, skipped: false, scores: {}, duration_ms: 1,
        token_usage: { input_tokens: 0, output_tokens: 0, total_tokens: 0 }, answer: null, notes: [], sub_results: [] },
      execution_microusd: 0, provider_requests: 0, uncertain_charge: false, critical_failure: false, unresolved_rubric: false,
      isolation: { before_sha256: hash('synthetic-snapshot'), after_sha256: hash('synthetic-snapshot'), snapshot_sha256: hash('synthetic-snapshot'),
        configuration_sha256: configuration, driver_sha256: hash(script), service_build: 'synthetic-protocol-test-only',
        semantic_time: manifest.semantic_time, sealed: true, restricted_network: true, hidden_tests_inaccessible: true },
    });
  }
  const out = path.join(pack, 'out');
  const args = ['run', '--offline', '--allow-shell', '--driver', driver, '--out', out];
  const result = JSON.parse(ok(await cli(args)).stdout);
  assert(result.harness_valid && !result.official_evidence);
  const original = await fs.readFile(path.join(out, 'artifact.json'), 'utf8');
  const count = async () => (await fs.readFile(path.join(out, 'dispatch-count'))).length;
  assert.equal(await count(), 660);
  assert.equal((await cli(args)).status, 1, 'implicit overwrite refused');
  ok(await cli([...args, '--resume']));
  assert.equal(await count(), 660, 'complete resume must not invoke driver');
  assert.equal(await fs.readFile(path.join(out, 'artifact.json'), 'utf8'), original);
  const journalPath = path.join(out, 'dispatch-budget.jsonl');
  const journal = await fs.readFile(journalPath, 'utf8');
  const lines = journal.trimEnd().split('\n');
  assert.equal(JSON.parse(lines.at(-1)).event, 'settled');
  await fs.writeFile(journalPath, lines.slice(0, -1).join('\n') + '\n');
  ok(await cli([...args, '--resume']));
  assert.equal(await fs.readFile(journalPath, 'utf8'), journal, 'complete receipt closes missing settlement');
  assert.equal(await count(), 660);
  await fs.writeFile(journalPath, lines.slice(0, -1).join('\n') + '\n');
  await fs.rm(path.join(out, 'dispatch-results', hash('work-00659') + '.json'));
  const blocked = await cli([...args, '--resume']);
  assert.equal(blocked.status, 1);
  assert.match(blocked.stderr, /ambiguous or invalid/);
  assert.equal(await count(), 660, 'ambiguous result must not invoke any driver');
  assert.equal(await fs.readFile(path.join(out, 'artifact.json'), 'utf8'), original);
});
