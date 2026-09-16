// SPDX-License-Identifier: AGPL-3.0-or-later
// Host-only provider owner for the existing campaign session; not a launcher.
// A benchmark/agent cannot choose credentials, endpoints or counter callbacks.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { checkApprovedCampaign, withApprovedEvaluationKey } from './approved-credential.mjs';
import { createProviderTransport } from './provider-transport.mjs';
import { createProviderBindings, validateProviderProfile } from './provider-profile.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const reservationSource = await fs.readFile(new URL('./context-reservation.mjs', import.meta.url));

async function readRegular(file) {
  const stat = await fs.lstat(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 2 * 1024 * 1024) throw Error('invalid provider configuration asset');
  const bytes = await fs.readFile(file);
  if (bytes.length > 2 * 1024 * 1024) throw Error('oversized provider configuration asset');
  return bytes;
}

export async function withApprovedProvider(options, use, stopSignal = new AbortController().signal) {
  if (!(stopSignal instanceof AbortSignal) || stopSignal.aborted) throw Error('approved provider cancelled');
  const locked = structuredClone(options);
  if (typeof use !== 'function' || !path.isAbsolute(locked.manifest)) throw Error('invalid approved provider owner');
  const bytes = await readRegular(locked.manifest), manifest = JSON.parse(bytes);
  const { profile } = validateProviderProfile(manifest);
  if (profile.version !== 2) throw Error('live owner requires explicit full-context reservations');
  const root = await fs.realpath(path.dirname(locked.manifest));
  const verify = async () => {
    if (stopSignal.aborted) throw Error('approved provider cancelled');
    if (!(await readRegular(locked.manifest)).equals(bytes)) throw Error('changed approved provider manifest');
    for (const pin of [profile.responses_tokenizer.asset, profile.embedding.tokenizer.asset]) {
      const file = path.join(root, pin.path), canonical = await fs.realpath(file);
      if (!canonical.startsWith(root + path.sep)) throw Error('counter asset escaped campaign root');
      const actual = await readRegular(file);
      if (hash(actual) !== pin.sha256 || !actual.equals(reservationSource)) throw Error('counter is not the reviewed reservation implementation');
    }
    // The Rust gate also checks all source/label/configuration assets and exact
    // approvals. This check is repeated immediately before every disclosure.
    const checked = await checkApprovedCampaign(locked);
    if (stopSignal.aborted) throw Error('approved provider cancelled');
    return checked;
  };
  await verify();
  return withApprovedEvaluationKey(locked, async key => {
    const lifetime = new AbortController();
    const ownerSignal = AbortSignal.any([stopSignal, lifetime.signal]);
    if (ownerSignal.aborted) throw Error('approved provider cancelled');
    let open = true;
    const send = api => {
      const transport = createProviderTransport({ endpoint: `https://api.openai.com/v1/${api}`, api, bearer: key, offline: false });
      return async (body, signal) => {
        if (!open || ownerSignal.aborted || signal?.aborted) throw Error('provider owner closed or cancelled');
        try {
          await verify();
          if (!open || ownerSignal.aborted || signal?.aborted) throw Error('provider owner closed or cancelled');
          return await transport(body, AbortSignal.any([signal, ownerSignal]));
        } catch {
          // Poison after any failed verification or uncertain transmission. The
          // durable mediator keeps reservations; opening a new task is not retry.
          open = false; lifetime.abort();
          throw Error('approved provider stopped; retain private journals');
        }
      };
    };
    const sendResponses = send('responses'), sendEmbeddings = send('embeddings');
    const provider = {
      bind(mediator, taskId) {
        if (!open || ownerSignal.aborted) throw Error('approved provider owner closed');
        return createProviderBindings({ manifest, mediator, taskId, sendResponses, sendEmbeddings, signal: ownerSignal });
      },
    };
    try { await use(provider); }
    finally { open = false; lifetime.abort(); }
  });
}
