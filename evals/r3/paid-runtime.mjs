// SPDX-License-Identifier: AGPL-3.0-or-later
// Lock a draft pack's executable closure before its manifest is approved.
import fs from 'node:fs/promises';
import { createReadStream, constants } from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';

export const runtimeFiles = ['launch-paid.mjs', 'paid-runtime.mjs', 'approved-driver.mjs', 'campaign-dispatch.mjs',
  'approved-campaign-session.mjs', 'approved-provider.mjs', 'approved-credential.mjs', 'provider-campaign-session.mjs',
  'provider-profile.mjs', 'context-reservation.mjs', 'provider-transport.mjs', 'provider-diagnostics.mjs', 'request-mediator.mjs', 'responses-call.mjs',
  'responses-request.mjs', 'responses-receipt.mjs', 'embeddings-call.mjs', 'embedding-evidence.mjs', 'synthetic-embedding.mjs',
  'isolation/container.mjs', 'isolation/guest.mjs', 'isolation/rust.mjs', 'isolation/sequence.mjs', 'isolation/knowledge.mjs',
  'isolation/service-session.mjs', 'isolation/embedding-proxy.mjs', 'isolation/model-agent.mjs', 'isolation/model-answer.mjs',
  'isolation/model-dispatch.mjs', 'isolation/rehearsal-task.mjs'];
const binaryNames = ['bao', 'budget', 'memory', 'node', 'retriever', 'score', 'snapshot'];
export const digest = bytes => createHash('sha256').update(bytes).digest('hex');
async function fileHash(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}
export async function regular(file, maximum = 2 * 1024 * 1024) {
  const stat = await fs.lstat(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > maximum) throw Error('invalid runtime input');
  const bytes = await fs.readFile(file);
  if (bytes.length > maximum) throw Error('oversized runtime input');
  return bytes;
}
function imagesValid(images) {
  return images && Object.keys(images).sort().join(',') === 'node,rust'
    && Object.values(images).every(id => /^sha256:[a-f0-9]{64}$/.test(id));
}

export async function packagePaidRuntime(root, options) {
  if (!path.isAbsolute(root) || await fs.realpath(root) !== root
      || !options || Object.keys(options).sort().join(',') !== 'binaries,images'
      || Object.keys(options.binaries).sort().join(',') !== binaryNames.join(',')
      || !Object.values(options.binaries).every(p => typeof p === 'string' && path.isAbsolute(p))
      || !imagesValid(options.images)) throw Error('explicit binaries, canonical pack and immutable images required');
  const file = path.join(root, 'campaign.json'), original = await regular(file), manifest = JSON.parse(original);
  if (manifest.label_status !== 'draft' || manifest.provider_profile?.version !== 2) throw Error('only a draft v2 pack may be packaged');
  const runtime = { version: 1, profile: 'memory-r3-approved-v2', root, files: [], binaries: {}, images: options.images };
  await fs.mkdir(path.join(root, 'runtime'), { mode: 0o700 });
  await fs.mkdir(path.join(root, 'private/executables'), { mode: 0o700 });
  for (const name of runtimeFiles) {
    const source = await regular(new URL(name, import.meta.url)), relative = `runtime/${name}`;
    const bytes = name === 'approved-driver.mjs'
      ? Buffer.from(`#!${root}/private/executables/node\n${source.toString('utf8').split('\n').slice(1).join('\n')}`) : source;
    const destination = path.join(root, relative);
    await fs.mkdir(path.dirname(destination), { recursive: true, mode: 0o700 });
    await fs.writeFile(destination, bytes, { flag: 'wx', mode: name === 'approved-driver.mjs' ? 0o500 : 0o400 });
    const pin = { path: relative, sha256: digest(bytes) };
    runtime.files.push(pin); manifest.assets.push({ ...pin, kind: 'configuration' });
  }
  for (const name of binaryNames) {
    const source = await fs.realpath(options.binaries[name]), hash = await fileHash(source);
    const relative = `private/executables/${name}`, destination = path.join(root, relative);
    await fs.copyFile(source, destination, constants.COPYFILE_EXCL | constants.COPYFILE_FICLONE);
    await fs.chmod(destination, 0o500);
    if (await fileHash(destination) !== hash) throw Error('binary changed while packaging');
    runtime.binaries[name] = { path: relative, sha256: hash };
  }
  const bytes = JSON.stringify(runtime) + '\n';
  await fs.writeFile(path.join(root, 'private/paid-runtime.json'), bytes, { flag: 'wx', mode: 0o400 });
  manifest.assets.push({ path: 'private/paid-runtime.json', sha256: digest(bytes), kind: 'configuration' });
  if (!(await regular(file)).equals(original)) throw Error('manifest changed while packaging');
  await fs.writeFile(file, JSON.stringify(manifest, null, 2) + '\n');
  return runtime;
}

export async function verifyPaidRuntime(root) {
  if (!path.isAbsolute(root) || await fs.realpath(root) !== root) throw Error('canonical runtime root required');
  const manifest = JSON.parse(await regular(path.join(root, 'campaign.json')));
  const bytes = await regular(path.join(root, 'private/paid-runtime.json'), 65536), runtime = JSON.parse(bytes);
  const asset = relative => manifest.assets.find(a => a.path === relative && a.kind === 'configuration');
  if (asset('private/paid-runtime.json')?.sha256 !== digest(bytes)
      || Object.keys(runtime).sort().join(',') !== 'binaries,files,images,profile,root,version'
      || runtime.version !== 1 || runtime.profile !== 'memory-r3-approved-v2' || runtime.root !== root
      || !imagesValid(runtime.images) || Object.keys(runtime.binaries).sort().join(',') !== binaryNames.join(',')
      || !Array.isArray(runtime.files) || runtime.files.length !== runtimeFiles.length
      || new Set(runtime.files.map(p => p.path)).size !== runtimeFiles.length) throw Error('invalid locked runtime');
  for (const name of runtimeFiles) {
    const relative = `runtime/${name}`, pin = runtime.files.find(p => p.path === relative);
    if (!pin || pin.sha256 !== asset(relative)?.sha256 || await fs.realpath(path.join(root, relative)) !== path.join(root, relative)
        || digest(await regular(path.join(root, relative))) !== pin.sha256) throw Error('runtime source changed');
  }
  for (const name of binaryNames) {
    const pin = runtime.binaries[name], file = path.join(root, `private/executables/${name}`);
    if (pin.path !== `private/executables/${name}` || !(await fs.lstat(file)).isFile()
        || await fs.realpath(file) !== file || await fileHash(file) !== pin.sha256) throw Error('runtime executable changed');
  }
  return { runtime, manifest };
}
