#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Offline-only bootstrap. Copies the executable controller contract into the
// new pack before execution so the campaign hashes cover its actual code.
import fs from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { constants } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const [destination, ...flags] = process.argv.slice(2);
if (!destination || new Set(flags).size !== flags.length || flags.some(flag => !['--one-family-per-track','--mediated','--wire'].includes(flag))) throw Error('usage: node rehearse.mjs NEW_DIRECTORY [--one-family-per-track] [--mediated] [--wire]');
const selection = flags.includes('--one-family-per-track') ? '--one-family-per-track' : undefined;
const wire = flags.includes('--wire');
const mediated = flags.includes('--mediated') || wire;
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(destination);
const databaseUrl = process.env.MEMORY_LAYER_TEST_DATABASE_URL;
const binaries = {
  memory: process.env.MEMORY_R3_CLI_BIN, snapshot: process.env.MEMORY_R3_SNAPSHOT_BIN,
  score: process.env.MEMORY_R3_SCORE_BIN, retriever: process.env.MEMORY_R3_RETRIEVER_BIN,
  ...(mediated ? { budget: process.env.MEMORY_R3_BUDGET_BIN } : {}),
};
const images = { node: process.env.MEMORY_R3_TEST_IMAGE, rust: process.env.MEMORY_R3_RUST_IMAGE };
if (!databaseUrl || process.env.MEMORY_LAYER_TEST_REQUIRE_DB !== '1' || Object.values(binaries).some(value => !value || !path.isAbsolute(value))
    || Object.values(images).some(value => !/^sha256:[a-f0-9]{64}$/.test(value))) throw Error('explicit disposable database, binaries and immutable images required');
const env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: os.tmpdir(), MEMORY_LAYER_PROFILE: 'prod' };
const run = (binary, args, options = {}) => new Promise((resolve, reject) => {
  const child = spawn(binary, args, { env, stdio: 'inherit', ...options });
  const interrupt = () => child.kill('SIGINT');
  process.once('SIGINT', interrupt);
  process.once('SIGTERM', interrupt);
  child.once('error', () => reject(Error('rehearsal process unavailable')));
  child.once('close', (code, signal) => {
    process.removeListener('SIGINT', interrupt);
    process.removeListener('SIGTERM', interrupt);
    resolve(signal ? 1 : code);
  });
});
async function fileHash(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}
const pins = {};
for (const [name, file] of Object.entries(binaries)) pins[name] = { path: await fs.realpath(file), sha256: await fileHash(file) };
if (await run(process.execPath, [path.join(here, 'build-representative-pack.mjs'), root, ...(selection ? [selection] : [])]) !== 0) throw Error('representative pack preparation failed');
// Cargo can rebuild example executables during unrelated tests. Execute private
// copies, not mutable checkout paths; verify the copy matches the pinned bytes.
const executables = path.join(root, 'private/executables');
await fs.mkdir(executables, { mode: 0o700 });
for (const [name, pin] of Object.entries(pins)) {
  const destination = path.join(executables, name);
  await fs.copyFile(pin.path, destination, constants.COPYFILE_EXCL | constants.COPYFILE_FICLONE);
  if (await fileHash(destination) !== pin.sha256) throw Error('executable changed while preparing rehearsal');
  await fs.chmod(destination, 0o500);
  pin.path = destination;
}
const staging = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-controller-'));
try {
  const manifestPath = path.join(root, 'campaign.json');
  const manifest = JSON.parse(await fs.readFile(manifestPath));
  if (mediated) {
    manifest.provider_budget = { max_calls_per_task: 7 };
    manifest.model.resolved_id = 'synthetic-mediated-excerpt-noop-v1';
    if (wire) manifest.model.provider = 'openai'; // Fake wire protocol, never a hosted provider.
    manifest.limits.requests *= 7;
    const sources = new Set(manifest.cases.flatMap(item => item.sources)).size;
    manifest.preparation_budget = { requests: sources * 8, tokens: sources * 4096, microusd: 0, task_seconds: 600 };
    manifest.limits.requests += manifest.preparation_budget.requests;
  }
  const add = async (relative, bytes, executable = false) => {
    const target = path.join(root, relative);
    await fs.mkdir(path.dirname(target), { recursive: true, mode: 0o700 });
    await fs.writeFile(target, bytes, { flag: 'wx', mode: executable ? 0o700 : 0o600 });
    manifest.assets.push({ path: relative, sha256: createHash('sha256').update(bytes).digest('hex'), kind: 'configuration' });
  };
  for (const relative of ['rehearsal-controller.mjs', 'offline-driver.mjs', 'embedding-evidence.mjs', 'synthetic-embedding.mjs', 'request-mediator.mjs', 'isolation/container.mjs', 'isolation/guest.mjs',
    'synthetic-wire-provider.mjs', 'provider-profile.mjs', 'context-reservation.mjs', 'provider-campaign-session.mjs', 'provider-transport.mjs', 'responses-call.mjs', 'responses-request.mjs', 'responses-receipt.mjs', 'embeddings-call.mjs',
    'isolation/rust.mjs', 'isolation/sequence.mjs', 'isolation/knowledge.mjs', 'isolation/service-session.mjs', 'isolation/embedding-proxy.mjs',
    'isolation/model-agent.mjs', 'isolation/model-answer.mjs', 'isolation/model-dispatch.mjs', 'isolation/rehearsal-task.mjs']) {
    await add(`runtime/${relative}`, await fs.readFile(path.join(here, relative)), relative === 'offline-driver.mjs');
  }
  if (wire) {
    const asset = manifest.assets.find(asset => asset.path === 'runtime/synthetic-wire-provider.mjs');
    const tokenizer = { id: 'synthetic-wire-counter-not-real-tokenizer-v1', asset: { path: asset.path, sha256: asset.sha256 } };
    await add('private/provider-review.json', JSON.stringify({ version: 1, synthetic_only: true,
      tokenizer: 'Constant 64 symbolic tokens; not a verified tokenizer.', pricing: 'Zero-spend loopback provider only.',
      approved: false }) + '\n');
    const evidence = manifest.assets.find(asset => asset.path === 'private/provider-review.json');
    manifest.provider_profile = { version: 1, protocol: 'openai-text-v1', responses_tokenizer: tokenizer,
      embedding: { resolved_id: manifest.model.embedding_id, dimensions: 16, max_input_tokens: 8192,
        input_microusd_per_million: 0, tokenizer }, configuration_evidence: { path: evidence.path, sha256: evidence.sha256 } };
  }
  await add('private/runtime.json', JSON.stringify({ version: 1, binaries: pins, images, syntheticWire: wire,
    socket: path.join(staging, 'dispatch.sock') }) + '\n');
  // Only this freshly created draft manifest is amended; no existing pack is edited.
  await fs.writeFile(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  const code = await run(process.execPath, [path.join(root, 'runtime/rehearsal-controller.mjs')], {
    cwd: root, env: { ...env, HOME: staging, MEMORY_LAYER_TEST_DATABASE_URL: databaseUrl, MEMORY_LAYER_TEST_REQUIRE_DB: '1' } });
  process.exitCode = code;
} finally {
  // A controller can retain its own separate service staging on ambiguous
  // cleanup. This directory owns only the local dispatch socket and HOME.
  await fs.rm(staging, { recursive: true });
}
