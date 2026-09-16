#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Preparation only: no approval records, key access, database or provider calls.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { packagePaidRuntime, regular, digest } from './paid-runtime.mjs';
try {
  if (process.argv.length !== 4) throw Error('arguments');
  const root = path.resolve(process.argv[2]), inputs = JSON.parse(await regular(path.resolve(process.argv[3]), 16384));
  const generated = spawnSync(process.execPath, [fileURLToPath(new URL('./build-representative-pack.mjs', import.meta.url)), root, '--calibration'],
    { env: { PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 20000, maxBuffer: 16384 });
  if (generated.status !== 0) throw Error('pack preparation failed');
  const file = path.join(root, 'campaign.json'), manifest = JSON.parse(await regular(file));
  manifest.model = { provider: 'openai', resolved_id: 'gpt-5.4-mini-2026-03-17', reasoning: 'medium', embedding_id: 'text-embedding-3-small',
    max_input_tokens: 400000, max_output_tokens: 16384, input_microusd_per_million: 750000,
    output_microusd_per_million: 4500000, cached_input_microusd_per_million: 75000 };
  manifest.provider_budget = { max_calls_per_task: 12 };
  manifest.limits = { requests: 500, tokens: 10000000, microusd: 10000000, task_seconds: 300 };
  manifest.preparation_budget = { requests: 100, tokens: 819200, microusd: 20000, task_seconds: 300 };
  const add = async (relative, source) => {
    const bytes = await regular(new URL(source, import.meta.url));
    await fs.writeFile(path.join(root, relative), bytes, { flag: 'wx', mode: 0o600 });
    const pin = { path: relative, sha256: digest(bytes) };
    manifest.assets.push({ ...pin, kind: 'configuration' }); return pin;
  };
  const counter = await add('private/context-reservation.mjs', './context-reservation.mjs');
  const evidence = await add('private/provider-review.md', './context-accounting.md');
  manifest.provider_profile = { version: 2, protocol: 'openai-text-v1', input_accounting: 'full_context_reservation',
    responses_tokenizer: { id: 'full-context-responses-v1', asset: counter },
    embedding: { resolved_id: 'text-embedding-3-small', dimensions: 1536, max_input_tokens: 8192,
      input_microusd_per_million: 20000, tokenizer: { id: 'full-context-embeddings-v1', asset: counter } }, configuration_evidence: evidence };
  await fs.writeFile(file, JSON.stringify(manifest, null, 2) + '\n');
  const runtime = await packagePaidRuntime(root, inputs);
  const planned = spawnSync(path.join(root, runtime.binaries.memory.path), ['eval', 'campaign', 'plan', '--manifest', file, '--split', 'development'],
    { env: { PATH: '/usr/bin:/bin', HOME: path.join(root, 'private'), MEMORY_LAYER_PROFILE: 'prod' }, encoding: 'utf8', timeout: 20000, maxBuffer: 1048576 });
  if (planned.status !== 0) throw Error('locked draft invalid');
  const plan = JSON.parse(planned.stdout);
  console.log(JSON.stringify({ manifest: file, draft_manifest_sha256: plan.manifest_sha256, task_executions: plan.work.length,
    labels: 'draft', approval_created: false, provider_called: false, limits: manifest.limits,
    note: 'Review labels first; changing label_status to reviewed changes the hash. Approve spending against that final hash, never this draft identity.' }));
} catch {
  console.error('Calibration preparation failed. Retain the draft for inspection; no approvals or provider calls were made.');
  process.exitCode = 1;
}
