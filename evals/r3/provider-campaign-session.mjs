// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted controller lifecycle, independent of synthetic providers. Transport
// construction/approval belongs to its owner. Only disposable snapshot services
// are managed here; no credentials, endpoints or model defaults are discovered.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { openRequestMediator } from './request-mediator.mjs';
import { validateProviderProfile, validateProviderBinding } from './provider-profile.mjs';
import { openPreparedSession } from './isolation/service-session.mjs';
import { createProviderExecutor } from './isolation/rehearsal-task.mjs';

export async function openProviderCampaignSession(options) {
  const { provider, ...plain } = options;
  const { configuration, snapshotBinary, memoryBinary, databaseUrl, split, offline } = structuredClone(plain);
  const { root, manifestSha256, budgetBinary } = configuration;
  if (!path.isAbsolute(root) || !path.isAbsolute(budgetBinary) || !path.isAbsolute(snapshotBinary) || !path.isAbsolute(memoryBinary)
      || !['development', 'held_out'].includes(split) || typeof offline !== 'boolean'
      || !provider || typeof provider.bind !== 'function') throw Error('invalid provider campaign owner');
  const canonicalRoot = await fs.realpath(root);
  const manifestFile = path.join(root, 'campaign.json');
  const manifestBytes = await fs.readFile(manifestFile);
  const manifest = JSON.parse(manifestBytes);
  validateProviderProfile(manifest);
  if (!manifest.preparation_budget || !manifest.provider_budget || (offline && manifest.limits.microusd !== 0)) throw Error('bounded provider preparation required');
  const readAsset = async relative => {
    const asset = manifest.assets.find(asset => asset.path === relative);
    if (!asset || typeof relative !== 'string' || relative.split('/').some(part => !part || part === '.' || part === '..')) throw Error('unlocked campaign asset');
    const file = path.join(root, relative);
    const stat = await fs.lstat(file), resolved = await fs.realpath(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 2 * 1024 * 1024 || !resolved.startsWith(canonicalRoot + path.sep)) throw Error('invalid campaign asset');
    const bytes = await fs.readFile(file);
    if (createHash('sha256').update(bytes).digest('hex') !== asset.sha256) throw Error('changed campaign asset');
    return bytes.toString('utf8');
  };
  const verify = async () => {
    if (!(await fs.readFile(manifestFile)).equals(manifestBytes)) throw Error('changed campaign manifest');
    for (const asset of manifest.assets) await readAsset(asset.path);
  };
  await verify();
  let state = 'new', session, execute;
  return {
    async prepare(contract) {
      if (state !== 'new') throw Error('provider preparation cannot be repeated');
      state = 'preparing';
      let mediator;
      try {
        const p = structuredClone(contract), allocated = manifest.preparation_budget;
        if (!p || p.directory !== path.join(root, 'run/preparation/provider') || p.task_id !== 'preparation'
            || p.manifest_sha256 !== manifestSha256 || !p.limits || Object.keys(p.limits).length !== 4
            || ['requests', 'tokens', 'microusd', 'task_seconds'].some(key => p.limits[key] !== allocated[key])
            || JSON.stringify(p.limits) !== JSON.stringify(p.task_limits)) throw Error('invalid preparation allocation');
        await verify();
        mediator = await openRequestMediator({ binary: budgetBinary, directory: p.directory, manifestSha256,
          limits: p.limits, taskLimits: p.task_limits });
        const binding = provider.bind(mediator, 'preparation');
        const profile = validateProviderBinding(manifest, binding);
        const documents = [], seen = new Set();
        for (const item of manifest.cases.filter(item => item.split === split)) {
          for (const source of item.sources) {
            const key = `${item.cluster}:${source}`;
            if (seen.has(key)) continue;
            seen.add(key);
            if (!manifest.assets.some(asset => asset.path === source && asset.kind === 'source')) throw Error('unlocked source');
            documents.push({ project: `r3-${item.cluster}`, source, text: await readAsset(source) });
          }
        }
        if (!documents.length) throw Error('empty campaign split');
        session = await openPreparedSession({ snapshotBinary, memoryBinary, databaseUrl, semanticTime: manifest.semantic_time,
          documents, embedding: { model: profile.model, dimensions: profile.dimensions, embed: binding.embed } });
        const baselineEmbeddings = new Map();
        for (const document of documents) baselineEmbeddings.set(`${document.project}:${document.source}`, await binding.embed(document.text, 'document'));
        await verify();
        execute = await createProviderExecutor({ session, baselineEmbeddings, wireProvider: provider, configuration, offline, split });
        const receipt = { version: 1, snapshot_sha256: session.preparation.snapshot_sha256, audit: (await mediator.audit()).audit };
        // Close the durable journal before making the preparation available.
        await mediator.close(); mediator = undefined;
        state = 'ready';
        return receipt;
      } catch (error) { state = 'failed'; throw error; }
      finally { await mediator?.close(); }
    },
    async execute(work, contract) {
      if (state !== 'ready') throw Error('provider campaign is not ready');
      state = 'running';
      try {
        const result = await execute(work, contract);
        state = 'ready';
        return result;
      } catch (error) { state = 'failed'; throw error; }
    },
    async close() {
      if (['preparing', 'running'].includes(state)) throw Error('cancel and await active provider work before closing');
      state = 'closed';
      await session?.close();
      session = undefined;
    },
  };
}
