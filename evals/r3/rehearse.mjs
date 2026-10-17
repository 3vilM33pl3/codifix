#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Offline-only bootstrap. Copies the executable controller contract into the
// new pack before execution so the campaign hashes cover its actual code.
import defaultFs from 'node:fs/promises';
import { constants, createReadStream } from 'node:fs';
import os from 'node:os';
import hostPath from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { nativeProfileHash } from './isolation/native-windows.mjs';
import { validateExecutionInput } from './isolation/execution.mjs';

const here = hostPath.dirname(fileURLToPath(import.meta.url));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const exec = promisify(execFile);

// Trusted dependencies support fake native bootstrap checks on other hosts.
export async function rehearse(argv = process.argv.slice(2), environment = process.env, {
  fileSystem = defaultFs, runProcess, platform = process.platform, processExecutable = process.execPath,
  callerPid = process.pid, log = value => console.log(value),
  invokeHelper = (binary, args, env) => exec(binary, args,
    { env, timeout: 10000, maxBuffer: 1048576, shell: false, windowsHide: true }),
} = {}) {
  const fs = fileSystem;
  const [destination, ...flags] = argv;
  if (!destination || new Set(flags).size !== flags.length || flags.some(flag => !['--one-family-per-track','--mediated','--wire','--native','--prepare-only'].includes(flag))) throw Error('usage: node rehearse.mjs NEW_DIRECTORY [--one-family-per-track] [--mediated | --wire] [--native] [--prepare-only]');
  const native = flags.includes('--native');
  const path = native ? hostPath.win32 : hostPath;
  if (platform === 'win32' && !native) throw Error('explicit --native selection required on Windows');

  const selection = flags.includes('--one-family-per-track') ? '--one-family-per-track' : undefined;
  const wire = flags.includes('--wire');
  const mediated = flags.includes('--mediated') || wire;
  const root = path.resolve(destination);
  const databaseUrl = environment.CODIFIX_TEST_DATABASE_URL;
  const binaries = {
    memory: environment.CODIFIX_R3_CLI_BIN, snapshot: environment.CODIFIX_R3_SNAPSHOT_BIN,
    score: environment.CODIFIX_R3_SCORE_BIN, retriever: environment.CODIFIX_R3_RETRIEVER_BIN,
    ...(mediated ? { budget: environment.CODIFIX_R3_BUDGET_BIN } : {}),
  };
  const images = { node: environment.CODIFIX_R3_TEST_IMAGE, rust: environment.CODIFIX_R3_RUST_IMAGE };
  if (!databaseUrl || environment.CODIFIX_TEST_REQUIRE_DB !== '1' || Object.values(binaries).some(value => !value || !path.isAbsolute(value))
      || (!native && Object.values(images).some(value => !/^sha256:[a-f0-9]{64}$/.test(value)))) throw Error('explicit disposable database, binaries and immutable images required');
  if (native && mediated) throw Error('native bootstrap supports deterministic zero-spend rehearsal only');
  if (native && Object.values(images).some(value => value !== undefined)) throw Error('native bootstrap cannot contain Linux images');
  if (!native && (environment.CODIFIX_R3_NATIVE_EXECUTION !== undefined || environment.CODIFIX_R3_NATIVE_COMPILER !== undefined)) throw Error('native profile requires explicit --native selection');
  let env = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: os.tmpdir(), CODIFIX_PROFILE: 'prod' };
  const run = runProcess ?? ((binary, args, options = {}) => new Promise((resolve, reject) => {
    const child = spawn(binary, args, { env, stdio: 'inherit', shell: false, windowsHide: true, ...options });
    const interrupt = () => child.kill(native ? 'SIGKILL' : 'SIGINT');
    process.once('SIGINT', interrupt);
    process.once('SIGTERM', interrupt);
    child.once('error', () => {
      process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', interrupt);
      reject(Error('rehearsal process unavailable'));
    });
    child.once('close', (code, signal) => {
      process.removeListener('SIGINT', interrupt);
      process.removeListener('SIGTERM', interrupt);
      resolve(signal ? 1 : code);
    });
  }));
  async function fileHash(file) {
    if (fs !== defaultFs) return hash(await fs.readFile(file));
    const digest = createHash('sha256');
    for await (const chunk of createReadStream(file)) digest.update(chunk);
    return digest.digest('hex');
  }
  const pins = {};
  let execution, compiler, nativeGuest, staging;
  if (native) {
    if (platform !== 'win32') throw Error('native bootstrap requires Windows; no Linux fallback');
    if (!environment.CODIFIX_R3_NATIVE_EXECUTION || !environment.CODIFIX_R3_NATIVE_COMPILER) throw Error('explicit native execution JSON and compiler path required');
    execution = JSON.parse(await fs.readFile(environment.CODIFIX_R3_NATIVE_EXECUTION, 'utf8'));
    validateExecutionInput({ execution, argv: [execution?.profile?.node?.path], files: {}, exportPaths: [] });
    const profile = execution.profile, inventory = profile.toolchains.flatMap(tree => tree.files);
    if (path.normalize(processExecutable).toLowerCase() !== path.normalize(profile.node.path).toLowerCase()) throw Error('native bootstrap requires its reviewed Node runtime');
    compiler = inventory.find(file => file.path.toLowerCase() === environment.CODIFIX_R3_NATIVE_COMPILER.toLowerCase());
    if (!compiler || path.basename(compiler.path).toLowerCase() !== 'rustc.exe') throw Error('explicit pinned native compiler required');
    const parent = path.dirname(root), requestId = path.basename(parent);
    if (path.basename(root) !== 'pack' || requestId === '00000000-0000-0000-0000-000000000000'
        || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(requestId)
        || path.normalize(path.dirname(parent)).toLowerCase() !== path.normalize(profile.io_root).toLowerCase()) throw Error('native destination must be PROFILE_IO_ROOT/FRESH_UUID/pack');
    if (profile.toolchains.length > 6) throw Error('native bootstrap requires room for two private reviewed toolchain trees');
    const checkedPins = [profile.helper, profile.node, profile.guest, compiler];
    for (const [name, file] of Object.entries({ ...binaries, node: profile.node.path })) {
      validateExecutionInput({ execution, argv: [file], files: {}, exportPaths: [] });
      const pin = inventory.find(pin => pin.path.toLowerCase() === file.toLowerCase());
      pins[name] = structuredClone(pin); checkedPins.push(pin);
    }
    for (const pin of checkedPins) {
      const stat = await fs.stat(pin.path);
      if (!stat.isFile() || stat.size < 1 || stat.size > 512 * 1024 * 1024 || await fileHash(pin.path) !== pin.sha256) throw Error('native bootstrap helper/toolchain bytes changed');
    }
    nativeGuest = await fs.readFile(profile.guest.path);
    if (hash(nativeGuest) !== profile.guest.sha256) throw Error('native guest changed before its private copy');
    const systemRoot = environment.SystemRoot ?? 'C:\\Windows';
    if (!/^[A-Za-z]:\\Windows$/i.test(systemRoot) || !Number.isSafeInteger(callerPid) || callerPid < 1) throw Error('invalid native bootstrap host');
    env = { SystemRoot: systemRoot, WINDIR: systemRoot, PATH: '', CODIFIX_PROFILE: 'prod' };
    let job;
    try { job = JSON.parse((await invokeHelper(profile.helper.path, ['--verify-outer-job', String(callerPid)], env)).stdout); }
    catch { throw Error('native bootstrap requires a verified outer Job; no output created'); }
    if (!job || Object.keys(job).sort().join(',') !== 'breakaway_allowed,caller_pid,format,helper_pid,kill_on_close'
        || job.format !== 'memory-windows-trusted-outer-job-v1' || job.caller_pid !== callerPid
        || !Number.isSafeInteger(job.helper_pid) || job.helper_pid < 1 || job.helper_pid === callerPid
        || job.kill_on_close !== true || job.breakaway_allowed !== false) throw Error('invalid native bootstrap outer Job receipt');
    let prepared;
    try { prepared = JSON.parse((await invokeHelper(profile.helper.path, ['--prepare-io', profile.io_root, '--request-id', requestId], env)).stdout); }
    catch { throw Error('native bootstrap private staging preparation failed; no output created'); }
    if (!prepared || prepared.format !== 'memory-windows-evaluation-io-v1' || prepared.request_id !== requestId
        || typeof prepared.directory !== 'string' || path.normalize(prepared.directory).toLowerCase() !== path.normalize(parent).toLowerCase()
        || ![prepared.root_identity, prepared.directory_identity].every(value => Array.isArray(value) && value.length === 2
          && value.every(part => typeof part === 'string' && /^[0-9a-f]+$/.test(part)))) throw Error('invalid native bootstrap private staging receipt');
    staging = path.join(parent, 'controller');
    await fs.mkdir(staging);
    env = { ...env, HOME: staging, USERPROFILE: staging, TEMP: staging, TMP: staging };
  } else for (const [name, file] of Object.entries(binaries)) pins[name] = { path: await fs.realpath(file), sha256: await fileHash(file) };
  if (await run(native ? execution.profile.node.path : processExecutable,
    [hostPath.join(here, 'build-representative-pack.mjs'), root, ...(selection ? [selection] : [])], { env }) !== 0) throw Error('representative pack preparation failed');
  // Cargo can rebuild example executables during unrelated tests. Execute private
  // copies, not mutable checkout paths; verify the copy matches the pinned bytes.
  const executables = path.join(root, 'private/executables');
  await fs.mkdir(executables, native ? {} : { mode: 0o700 });
  for (const [name, pin] of Object.entries(pins)) {
    const filename = native ? ({ memory: 'memory.exe', snapshot: 'r3_snapshot.exe', score: 'r3_score.exe', retriever: 'r3_retriever.exe', node: 'node.exe' })[name] : name;
    const destination = path.join(executables, filename);
    await fs.copyFile(pin.path, destination, constants.COPYFILE_EXCL | constants.COPYFILE_FICLONE);
    if (await fileHash(destination) !== pin.sha256) throw Error('executable changed while preparing rehearsal');
    if (!native) await fs.chmod(destination, 0o500);
    pin.path = destination;
  }
  if (!native) staging = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-controller-'));
  try {
    const manifestPath = path.join(root, 'campaign.json');
    const manifest = JSON.parse(await fs.readFile(manifestPath));
    if (manifest.smoke_only !== true || manifest.label_status !== 'draft' || manifest.limits.microusd !== 0) throw Error('draft zero-spend generated rehearsal required');
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
      await fs.mkdir(path.dirname(target), { recursive: true, ...(native ? {} : { mode: 0o700 }) });
      await fs.writeFile(target, bytes, { flag: 'wx', ...(native ? {} : { mode: executable ? 0o700 : 0o600 }) });
      manifest.assets.push({ path: relative, sha256: createHash('sha256').update(bytes).digest('hex'), kind: 'configuration' });
    };
    for (const relative of ['rehearsal-controller.mjs', 'campaign-dispatch.mjs', 'offline-driver.mjs', 'embedding-evidence.mjs', 'synthetic-embedding.mjs', 'request-mediator.mjs', 'isolation/container.mjs', 'isolation/guest.mjs',
      'windows-dispatch-auth.mjs', 'windows-dispatch-client.mjs', 'windows-host-journal.mjs',
      'isolation/execution.mjs', 'isolation/native-windows.mjs', 'isolation/native-guest.mjs',
      'synthetic-wire-provider.mjs', 'provider-profile.mjs', 'context-reservation.mjs', 'provider-campaign-session.mjs', 'provider-transport.mjs', 'provider-diagnostics.mjs', 'responses-call.mjs', 'responses-request.mjs', 'responses-receipt.mjs', 'embeddings-call.mjs',
      'isolation/rust.mjs', 'isolation/sequence.mjs', 'isolation/knowledge.mjs', 'isolation/service-session.mjs', 'isolation/embedding-proxy.mjs',
      'isolation/model-agent.mjs', 'isolation/model-answer.mjs', 'isolation/model-dispatch.mjs', 'isolation/rehearsal-task.mjs']) {
      await add(`runtime/${relative}`, await fs.readFile(hostPath.join(here, relative)), relative === 'offline-driver.mjs');
    }
    if (native) {
      const copied = Object.values(pins).map(pin => structuredClone(pin));
      for (const pin of copied) manifest.assets.push({ path: path.relative(root, pin.path).split(path.sep).join('/'), sha256: pin.sha256, kind: 'configuration' });
      await add('private/native-driver/guest.mjs', nativeGuest);
      const guest = { path: path.join(root, 'private/native-driver/guest.mjs'), sha256: hash(nativeGuest) };
      execution.profile.node = pins.node;
      execution.profile.guest = guest;
      execution.profile.toolchains.push({ root: executables, files: copied }, { root: path.dirname(guest.path), files: [guest] });
      execution.profile_sha256 = nativeProfileHash(execution.profile);
      validateExecutionInput({ execution, argv: [pins.node.path], files: {}, exportPaths: [] });
      await add('runtime/offline-driver.mjs.windows-runtime.json', JSON.stringify({ schema_version: 1,
        node: 'private/executables/node.exe', node_sha256: pins.node.sha256 }) + '\n');
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
    const configuration = { version: 1, binaries: pins, syntheticWire: wire,
      ...(native ? { execution, nativeScorer: pins.score, compiler,
        dispatch: { kind: 'windows-loopback-v1', endpoint: 'private/dispatch-endpoint.json' } }
        : { images, socket: path.join(staging, 'dispatch.sock') }) };
    await add('private/runtime.json', JSON.stringify(configuration) + '\n');
    // Only this freshly created draft manifest is amended; no existing pack is edited.
    await fs.writeFile(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
    if (flags.includes('--prepare-only')) {
      log(JSON.stringify({ prepared: true, root, native, official_evidence: false, execution_started: false }));
      return { root, configuration, manifest };
    }
    if (native) throw Error('native bootstrap prepared; protected controller IPC integration is required before execution');
    const code = await run(processExecutable, [path.join(root, 'runtime/rehearsal-controller.mjs')], {
      cwd: root, env: { ...env, HOME: staging, CODIFIX_TEST_DATABASE_URL: databaseUrl, CODIFIX_TEST_REQUIRE_DB: '1' } });
    process.exitCode = code;
  } finally {
    // A controller can retain its own separate service staging on ambiguous
    // cleanup. This directory owns only the local dispatch socket and HOME.
    await fs.rm(staging, { recursive: true });
  }
}

const entry = process.argv[1] && hostPath.resolve(process.argv[1]);
const current = fileURLToPath(import.meta.url);
if (entry && (process.platform === 'win32' ? entry.toLowerCase() === current.toLowerCase() : entry === current)) await rehearse();
