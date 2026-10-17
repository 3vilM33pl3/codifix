#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Offline reconstruction of one known failed synthetic request. No key, provider,
// database, model-produced command, approval record or benchmark run.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { verifyPaidRuntime, regular, digest } from './paid-runtime.mjs';
import { createResponsesPreparation } from './responses-call.mjs';

export async function prepareResponseDiagnostic(sourceRoot, destination) {
  const { manifest: source, runtime } = await verifyPaidRuntime(sourceRoot);
  const read = async relative => {
    const pin = source.assets.find(a => a.path === relative), file = path.join(sourceRoot,relative);
    if (!pin || await fs.realpath(file) !== file) throw Error('unlocked reconstruction source');
    const bytes = await regular(file);
    if (digest(bytes) !== pin.sha256) throw Error('changed reconstruction source');
    return bytes.toString();
  };
  const request = JSON.parse(await regular(path.join(sourceRoot,'run/work-00000/request.json')));
  if (request.work?.case_id !== 'configuration-rollout' || request.work.condition !== 'vector') throw Error('unsupported diagnostic source');
  const family = JSON.parse(await read('private/families.json')).find(f => f.id === request.work.case_id);
  if (!family || family.sources.length !== 1) throw Error('single-source reconstruction required');
  const stages = JSON.parse(await read(family.stages)), initial = {};
  for (const asset of source.assets.filter(a => a.path.startsWith(family.fixture+'/'))) {
    initial[asset.path.slice(family.fixture.length+1)] = await read(asset.path);
  }
  const model = { provider: source.model.provider, resolved_id: source.model.resolved_id, reasoning: source.model.reasoning };
  const payload = { model, step_id: `${family.id}-${stages[1].id}`,
    files: { ...Object.fromEntries(Object.entries(initial).filter(([p]) => !family.submission_paths.includes(p))),
      'task.json': JSON.stringify({ prompt: stages[1].prompt,
        context: [{ id: family.sources[0], text: await read(family.sources[0]) }] }),
      ...Object.fromEntries(family.submission_paths.map(p => [p,initial[p]])) }, previous: null,
    tools: { protocol: 'isolated-command-v1', command: { kind: 'command', argv: ['absolute-executable','arguments'] },
      finish: { kind: 'finish' }, network: 'none', persistence: 'selected-submission-files-only' } };
  const prepare = createResponsesPreparation({ mediator: { price: async () => ({}) }, model: source.model,
    inputAccounting: source.provider_profile.input_accounting,
    tokenizer: { id: source.provider_profile.responses_tokenizer.id, sha256: source.provider_profile.responses_tokenizer.asset.sha256 },
    countInputTokens: () => 400000, send: () => { throw Error('offline reconstruction only'); } });
  const prepared = await prepare(payload), payloadHash = digest(JSON.stringify(prepared.payload));
  const requestId = 'call-'+digest(JSON.stringify(['work-00000',payload.step_id,0]));
  const journal = (await regular(path.join(sourceRoot,'run/work-00000/provider/budget.jsonl'))).toString().trim().split('\n').map(JSON.parse);
  if (!journal.some(e => e.event === 'reserved' && e.id === requestId && e.payload_sha256 === payloadHash)
      || journal.some(e => e.event === 'settled' && e.id === requestId)) throw Error('cannot exactly reconstruct the unsettled request');
  const memory = path.join(sourceRoot,runtime.binaries.memory.path);
  const plan = root => {
    const result = spawnSync(memory,['eval','campaign','plan','--manifest',path.join(root,'campaign.json')], {
      env: { PATH: '/usr/bin:/bin', HOME: path.join(root,'private'), CODIFIX_PROFILE: 'prod' },
      encoding: 'utf8', timeout: 20000, maxBuffer: 1048576,
    });
    if (result.status !== 0) throw Error('invalid diagnostic approval manifest');
    return JSON.parse(result.stdout).manifest_sha256;
  };
  const sourceIdentity = plan(sourceRoot);
  const inputs = { binaries: Object.fromEntries(Object.entries(runtime.binaries).map(([n,p]) => [n,path.join(sourceRoot,p.path)])), images: runtime.images };
  // Reuse the normal preparer and pin its executable closure, never edit a used pack.
  const temp = await fs.mkdtemp(path.join(path.dirname(destination),'r3-diagnostic-inputs-'));
  try {
    const file = path.join(temp,'runtime.json');
    await fs.writeFile(file,JSON.stringify(inputs), { flag: 'wx', mode: 0o600 });
    const built = spawnSync(process.execPath,[fileURLToPath(new URL('./prepare-paid-calibration.mjs',import.meta.url)),destination,file], {
      env: { PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 60000, maxBuffer: 1048576,
    });
    if (built.status !== 0) throw Error('diagnostic packaging failed');
  } finally { await fs.rm(temp,{recursive:true}); }
  const file = path.join(destination,'campaign.json'), manifest = JSON.parse(await regular(file));
  if (JSON.stringify(manifest.model) !== JSON.stringify(source.model)
      || JSON.stringify(manifest.assets.filter(a => a.kind !== 'configuration'))
        !== JSON.stringify(source.assets.filter(a => a.kind !== 'configuration'))) {
    throw Error('diagnostic preparer differs from reviewed model or benchmark');
  }
  // Two schema slots, but only ONE funded/executable diagnostic request. The
  // unfunded preparation slot is never run; the ordinary launcher denies this pack.
  manifest.limits = { requests: 2, tokens: 416385, microusd: 373728, task_seconds: 120 };
  manifest.preparation_budget = { requests: 1, tokens: 1, microusd: 0, task_seconds: 120 };
  manifest.provider_budget = { max_calls_per_task: 1 };
  const contract = { version: 1, kind: 'single-response-diagnostic',
    source_manifest_sha256: sourceIdentity, payload_sha256: payloadHash,
    limits: { requests: 1, tokens: 416384, microusd: 373728, task_seconds: 120 } };
  const add = async (relative,bytes) => {
    await fs.writeFile(path.join(destination,relative),bytes,{flag:'wx',mode:0o400});
    manifest.assets.push({path:relative,sha256:digest(bytes),kind:'configuration'});
  };
  await add('private/diagnostic-payload.json',JSON.stringify(payload)+'\n');
  await add('private/response-diagnostic.json',JSON.stringify(contract)+'\n');
  await add('runtime/response-diagnostic.mjs',await regular(new URL('./response-diagnostic.mjs',import.meta.url)));
  await fs.writeFile(file,JSON.stringify(manifest,null,2)+'\n');
  await verifyPaidRuntime(destination);
  return { root: destination, draft_manifest_sha256: plan(destination), source_manifest_sha256: sourceIdentity,
    reconstructed_payload_sha256: payloadHash, wire_bytes: Buffer.byteLength(JSON.stringify(prepared.payload.request)),
    limits: contract.limits, provider_called: false, approvals_created: false, benchmark_observations: 0 };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 4) throw Error('source and unused destination required');
    console.log(JSON.stringify(await prepareResponseDiagnostic(path.resolve(process.argv[2]),path.resolve(process.argv[3]))));
  } catch { console.error('Diagnostic preparation failed; no approvals or provider calls created.'); process.exitCode=1; }
}
