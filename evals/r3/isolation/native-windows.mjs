// SPDX-License-Identifier: AGPL-3.0-or-later
// Operator-configured private native helper; no paid calls or native-pass claim.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
const exec = promisify(execFile);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const plain = value => value && Object.getPrototypeOf(value) === Object.prototype;
const keys = (value, expected) => plain(value) && Object.keys(value).sort().join(',') === expected.split(',').sort().join(',');
const hex = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const cap = 1024 * 1024;
const profileKeys = 'schema_version,helper,io_root,node,guest,toolchains,search_path,library_path';

export function nativeCommandExitCode(code) {
  if (!Number.isSafeInteger(code) || code < -0x80000000 || code > 0xffffffff) throw Error('invalid native command exit status');
  const unsigned = code >>> 0;
  if (unsigned === 0xc0000135 || unsigned === 0xc0000142) throw Error('native toolchain loader failure invalidates execution');
  return unsigned;
}

function canonical(value) {
  if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
  if (plain(value)) return '{' + Object.keys(value).sort().map(key => JSON.stringify(key) + ':' + canonical(value[key])).join(',') + '}';
  if (typeof value !== 'string' && !Number.isSafeInteger(value)) throw Error('noncanonical native profile');
  return JSON.stringify(value);
}
export const nativeProfileHash = profile => {
  const bytes = canonical(profile);
  if (Buffer.byteLength(bytes) > 12 * cap) throw Error('native profile byte limit');
  return hash(bytes);
};
const equalPath = (a, b) => path.win32.normalize(a).toLowerCase() === path.win32.normalize(b).toLowerCase();
function absolute(value) {
  if (typeof value !== 'string' || value.length > 4096 || !/^[A-Za-z]:[\\/]/.test(value)
      || value.slice(2).includes(':') || value.includes('\0')
      || value.split(/[\\/]/).slice(1).some(part => part === '.' || part === '..'
        || part.endsWith('.') || part.endsWith(' ') || /[<>"|?*\x00-\x1f]/.test(part))) throw Error('invalid native absolute path');
}
function relative(value) {
  if (typeof value !== 'string' || value.length > 240 || !value.split('/').every(part => /^[a-zA-Z0-9_.-]+$/.test(part)
      && part !== '.' && part !== '..' && !part.endsWith('.') && !part.endsWith(' ')
      && !/^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)/i.test(part))) throw Error('unsafe native workspace path');
}
function bound(file) {
  if (!keys(file, 'path,sha256') || !hex(file.sha256)) throw Error('invalid native file binding');
  absolute(file.path);
}
export function validateNativeInput(options) {
  if (!plain(options) || Object.keys(options).some(key => !['execution','argv','files','timeoutMs','exportPaths','profile'].includes(key))) throw Error('native contract cannot contain Docker image or implicit options');
  const execution = options.execution;
  if (!keys(execution, 'kind,profile,profile_sha256') || execution.kind !== 'windows-native-v1'
      || !hex(execution.profile_sha256)) throw Error('explicit native profile required');
  const profile = execution.profile;
  if (!keys(profile, profileKeys) || profile.schema_version !== 1 || nativeProfileHash(profile) !== execution.profile_sha256) throw Error('native profile digest mismatch');
  for (const file of [profile.helper, profile.node, profile.guest]) bound(file);
  absolute(profile.io_root);
  if (profile.io_root.length <= 3 || !Array.isArray(profile.toolchains) || profile.toolchains.length < 1 || profile.toolchains.length > 8) throw Error('invalid native private root/toolchains');
  const inventory = new Map();
  for (const tree of profile.toolchains) {
    if (!keys(tree, 'root,files') || !Array.isArray(tree.files) || !tree.files.length) throw Error('invalid native toolchain');
    absolute(tree.root);
    for (const file of tree.files) {
      bound(file);
      const relative = path.win32.relative(tree.root, file.path);
      if (!relative || relative.startsWith('..') || path.win32.isAbsolute(relative) || inventory.has(file.path.toLowerCase())) throw Error('native inventory escape/alias');
      inventory.set(file.path.toLowerCase(), file.sha256);
      if (inventory.size > 65536) throw Error('native inventory limit');
    }
  }
  for (const file of [profile.node, profile.guest]) if (inventory.get(file.path.toLowerCase()) !== file.sha256) throw Error('native driver absent from reviewed tree');
  for (const list of [profile.search_path, profile.library_path]) {
    if (!Array.isArray(list) || list.length > 32) throw Error('invalid native search directories');
    for (const directory of list) {
      absolute(directory);
      if (!profile.toolchains.some(tree => {
        const relative = path.win32.relative(tree.root, directory);
        return !relative.startsWith('..') && !path.win32.isAbsolute(relative);
      })) throw Error('native search path outside reviewed toolchains');
    }
  }
  if (!Array.isArray(options.argv) || !options.argv.length || options.argv.length > 64
      || options.argv.some(value => typeof value !== 'string' || value.includes('\0') || value.length > 32768)) throw Error('invalid native argv');
  absolute(options.argv[0]);
  if (!inventory.has(options.argv[0].toLowerCase()) || !/\.exe$/i.test(options.argv[0])) throw Error('native command is not a pinned executable');
  if (!Number.isSafeInteger(options.timeoutMs ?? 10000) || (options.timeoutMs ?? 10000) < 1000 || (options.timeoutMs ?? 10000) > 60000) throw Error('invalid native deadline');
  if (options.profile !== undefined && !['text','rust'].includes(options.profile)) throw Error('invalid native resource profile');
  if (!plain(options.files) || Object.keys(options.files).length > 256) throw Error('native files must be explicit');
  let bytes = 0;
  const names = Object.keys(options.files);
  const foldedNames = names.map(name => name.toLowerCase());
  if (new Set(foldedNames).size !== names.length) throw Error('native input case alias');
  for (const name of names) {
    relative(name);
    if (typeof options.files[name] !== 'string' || (bytes += Buffer.byteLength(options.files[name])) > cap
        || foldedNames.some(other => other.startsWith(name.toLowerCase() + '/'))) throw Error('native file budget/conflict');
  }
  if (!Array.isArray(options.exportPaths) || options.exportPaths.length > 128
      || new Set(options.exportPaths.map(value => typeof value === 'string' ? value.toLowerCase() : value)).size !== options.exportPaths.length) throw Error('explicit native export selection required');
  options.exportPaths.forEach(relative);
  if (options.exportPaths.some(name => options.exportPaths.some(other => other.toLowerCase().startsWith(name.toLowerCase() + '/')))) throw Error('native export conflict');
}

async function invokeHelper(helper, args, timeout) {
  return exec(helper, args, { timeout, maxBuffer: cap, encoding: 'utf8', windowsHide: true, shell: false,
    killSignal: 'SIGKILL', env: { SystemRoot: process.env.SystemRoot ?? 'C:\Windows', PATH: '' } });
}

export function createNativeWindowsExecutor({ fileSystem = fs, invoke = invokeHelper, platform = process.platform, requestId = randomUUID } = {}) {
  return async options => {
    validateNativeInput(options);
    if (platform !== 'win32') throw Error('native Windows backend requires an explicit Windows host; no Docker/WSL fallback');
    const { execution, argv, files, exportPaths, timeoutMs = 10000 } = structuredClone(options);
    const profile = execution.profile;
    // Bootstrap trust is the operator-installed immutable helper, not fs.mode or
    // this checksum alone. Its first command validates ACLs BEFORE payload writes.
    for (const file of [profile.helper, profile.node, profile.guest]) {
      const stat = await fileSystem.stat(file.path);
      if (!stat.isFile() || stat.size > 512 * cap || hash(await fileSystem.readFile(file.path)) !== file.sha256) throw Error('native helper/driver bytes changed');
    }
    const id = requestId();
    if (typeof id !== 'string' || id === '00000000-0000-0000-0000-000000000000' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id)) throw Error('invalid native request identity');
    const directory = path.win32.join(profile.io_root, id);
    const started = performance.now();
    let prepared;
    try { prepared = JSON.parse((await invoke(profile.helper.path, ['--prepare-io', profile.io_root, '--request-id', id], 10000)).stdout); }
    catch { throw Error('native private I/O preparation failed; no payload written'); }
    if (prepared.format !== 'memory-windows-evaluation-io-v1' || prepared.request_id !== id
        || typeof prepared.directory !== 'string' || !equalPath(prepared.directory, directory)
        || ![prepared.root_identity, prepared.directory_identity].every(value => Array.isArray(value) && value.length === 2 && value.every(part => typeof part === 'string' && /^[0-9a-f]+$/.test(part)))) throw Error('native private I/O receipt mismatch; no payload written');
    const stdin = Buffer.from(JSON.stringify({ version: 1, argv, files, timeoutMs }));
    const stdinPath = path.win32.join(directory, 'input.json');
    await fileSystem.writeFile(stdinPath, stdin, { flag: 'wx' }); // inherits native-validated private ACL
    const descriptor = {
      schema_version: 1, profile: 'windows-appcontainer-evaluation-v1', executable: profile.node.path,
      // Node otherwise realpath-walks the drive root before loading its main
      // script. A zero-capability token cannot lstat C:\\, and we must not grant
      // drive-wide access to make the interpreter start. Toolchain/scratch
      // identities and reparse rejection remain enforced by the native helper.
      toolchains: profile.toolchains, arguments: ['--preserve-symlinks','--preserve-symlinks-main',profile.guest.path], search_path: profile.search_path,
      library_path: profile.library_path, deadline_ms: Math.min(300000, timeoutMs + 5000),
      max_output_bytes: 4 * cap, stdin_file: { path: stdinPath, sha256: hash(stdin) }, exports: exportPaths,
      allow_missing_exports: true,
    };
    const bytes = Buffer.from(JSON.stringify(descriptor));
    const descriptorPath = path.win32.join(directory, 'descriptor.json');
    await fileSystem.writeFile(descriptorPath, bytes, { flag: 'wx' });
    const resultDirectory = path.win32.join(directory, 'result');
    let receipt;
    try { receipt = JSON.parse((await invoke(profile.helper.path, ['--descriptor', descriptorPath, '--sha256', hash(bytes), '--output', resultDirectory], timeoutMs + 20000)).stdout); }
    catch { throw Error(`native evaluation failed; private operation retained at ${directory}`); }
    if (receipt.format !== 'memory-windows-evaluation-result-v1' || receipt.state !== 'succeeded'
        || receipt.descriptor_sha256 !== hash(bytes) || receipt.stdin_sha256 !== hash(stdin)
        || receipt.descriptor_verified !== true || receipt.exit_code !== 0 || receipt.disk_hard_limit_enforced !== false
        || receipt.profile !== 'windows-appcontainer-evaluation-v1' || receipt.architecture !== 'x86_64'
        || !Array.isArray(receipt.artifacts) || receipt.artifacts.length > exportPaths.length + 2) throw Error('native receipt binding failed');
    const artifacts = new Map();
    const artifactFiles = new Set();
    let total = 0;
    const output = Object.create(null);
    for (const item of receipt.artifacts) {
      if (!keys(item, 'source,file,sha256,bytes') || !hex(item.sha256) || !Number.isSafeInteger(item.bytes) || item.bytes < 0
          || !/^(?:stdout|stderr)\.bin$|^export-[0-9]{3}\.bin$/.test(item.file)
          || artifacts.has(item.source) || artifactFiles.has(item.file)
          || !['stdout','stderr',...exportPaths].includes(item.source)) throw Error('unexpected native artifact');
      const expectedFile = ['stdout','stderr'].includes(item.source) ? `${item.source}.bin`
        : `export-${String(exportPaths.indexOf(item.source)).padStart(3, '0')}.bin`;
      if (item.file !== expectedFile) throw Error('native artifact source binding failed');
      artifactFiles.add(item.file);
      const location = path.win32.join(resultDirectory, item.file);
      const stat = await fileSystem.stat(location);
      if (!stat.isFile() || stat.size !== item.bytes || (total += stat.size) > 4 * cap) throw Error('native artifact byte limit');
      const content = await fileSystem.readFile(location);
      if (hash(content) !== item.sha256 || content.length !== item.bytes) throw Error('native artifact changed');
      artifacts.set(item.source, content);
      if (exportPaths.includes(item.source)) output[item.source] = content.toString('base64');
    }
    if (!artifacts.has('stdout') || !artifacts.has('stderr')) throw Error('native diagnostics absent');
    let result;
    try { result = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(artifacts.get('stdout'))); }
    catch { throw Error('invalid native guest result'); }
    if (!keys(result, 'version,exitCode,stdout,stderr') || result.version !== 1
        || typeof result.stdout !== 'string' || typeof result.stderr !== 'string'
        || Buffer.byteLength(result.stdout) + Buffer.byteLength(result.stderr) > cap) throw Error('invalid native command result');
    result.exitCode = nativeCommandExitCode(result.exitCode);
    return { ...result, files: output, elapsedMs: performance.now() - started,
      execution: { kind: 'windows-native-v1', profile_sha256: execution.profile_sha256,
        descriptor_sha256: hash(bytes), receipt_sha256: hash(JSON.stringify(receipt)), disk_hard_limit_enforced: false },
      privateOperation: directory };
  };
}
export const runNativeWindows = createNativeWindowsExecutor();
