// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted, offline host only. Windows ACLs/identity/publication are established
// by the pinned unprivileged helper, never by ignored Node POSIX mode bits.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { validateExecutionInput } from './isolation/execution.mjs';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const maximum = 16 * 1024 * 1024;

export function createHostJournalHelperInvoker({ spawnProcess = spawn } = {}) {
  if (typeof spawnProcess !== 'function') throw Error('invalid trusted journal process adapter');
  return (binary, args, input, env) => new Promise((resolve, reject) => {
    let child;
    try { child = spawnProcess(binary, args, { env, shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] }); }
    catch { reject(Error('protected host journal helper failed')); return; }
    const chunks = []; let size = 0, failed = false;
    const fail = () => { if (failed) return; failed = true; try { child.kill('SIGKILL'); } catch { /* close owns completion */ } };
    const timer = setTimeout(fail, 30000);
    child.on('error', fail); child.stdin.on('error', fail);
    const consume = (bytes, retain) => {
      size += bytes.length;
      if (size > 40 * 1024 * 1024) fail();
      else if (retain && !failed) chunks.push(bytes);
    };
    child.stdout.on('data', bytes => consume(bytes, true));
    child.stderr.on('data', bytes => consume(bytes, false));
    child.once('close', (code, signal) => {
      clearTimeout(timer);
      if (failed || signal || code !== 0) { reject(Error('protected host journal helper failed')); return; }
      try { resolve(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks))); }
      catch { reject(Error('protected host journal helper output invalid')); }
    });
    try { child.stdin.end(input); } catch { fail(); }
  });
}
const invoke = createHostJournalHelperInvoker();
function directoryPath(value) {
  if (typeof value !== 'string' || value.length > 4096 || !/^[A-Za-z]:[\\/]/.test(value)
      || value.slice(2).includes(':') || value.split(/[\\/]/).slice(1).some(part => !part || part === '.' || part === '..'
        || part.endsWith('.') || part.endsWith(' ') || /[<>"|?*\x00-\x1f]/.test(part)
        || /^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)/i.test(part))) throw Error('invalid native journal directory');
  return path.win32.normalize(value);
}
function component(value) {
  if (typeof value !== 'string' || !/^[a-z0-9][a-z0-9.-]{0,127}$/.test(value)
      || value.endsWith('.') || value.includes('..')
      || /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/.test(value)) throw Error('invalid native journal component');
}
function payload(value) {
  try {
    const json = JSON.stringify(value);
    if (json === undefined || Buffer.byteLength(json) > maximum) throw Error('invalid');
    return Buffer.from(json);
  } catch { throw Error('invalid bounded native journal payload'); }
}
export async function createWindowsHostJournal(execution, { platform = process.platform, callerPid = process.pid,
  fileSystem = fs, invokeHelper = invoke, systemRoot = process.env.SystemRoot } = {}) {
  if (platform !== 'win32' || execution?.kind !== 'windows-native-v1'
      || !/^[A-Za-z]:\\Windows$/i.test(systemRoot ?? '') || !Number.isSafeInteger(callerPid) || callerPid < 1) {
    throw Error('explicit native host journal required');
  }
  const reviewed = structuredClone(execution);
  validateExecutionInput({ execution: reviewed, argv: [reviewed.profile?.node?.path], files: {}, exportPaths: [] });
  if (typeof invokeHelper !== 'function') throw Error('invalid trusted journal helper adapter');
  const pin = reviewed.profile.helper;
  const verifyHelper = async () => {
    try {
      const info = await fileSystem.stat(pin.path);
      if (!info.isFile() || info.nlink !== 1 || info.size < 1 || info.size > 512 * 1024 * 1024
          || hash(await fileSystem.readFile(pin.path)) !== pin.sha256) throw Error('changed');
    } catch { throw Error('native journal helper changed or unavailable'); }
  };
  await verifyHelper();
  const identities = new Map(), digests = new Map();
  const env = { SystemRoot: systemRoot, WINDIR: systemRoot, PATH: '' };
  async function call(operation, directory, name, bytes = Buffer.alloc(0)) {
    directory = directoryPath(directory); component(name);
    if (bytes.length > maximum) throw Error('invalid native journal contract');
    await verifyHelper();
    let receipt;
    try {
      receipt = JSON.parse(await invokeHelper(pin.path,
        ['--host-journal', operation, directory, name, '--caller-pid', String(callerPid)], bytes, env));
    } catch { throw Error('protected native journal operation failed; retain private evidence'); }
    const keys = ['directory_identity','format','name','operation', ...(operation === 'create' ? [] : ['sha256']), ...(operation === 'read' ? ['value'] : [])].sort();
    if (!receipt || Object.keys(receipt).sort().join(',') !== keys.join(',')
        || receipt.format !== 'memory-windows-host-journal-v1' || receipt.operation !== operation || receipt.name !== name
        || !Array.isArray(receipt.directory_identity) || receipt.directory_identity.length !== 2
        || !receipt.directory_identity.every(v => typeof v === 'string' && /^[0-9a-f]+$/.test(v))
        || (operation !== 'create' && !/^[0-9a-f]{64}$/.test(receipt.sha256))
        || (operation === 'write' && receipt.sha256 !== hash(bytes))) throw Error('invalid protected native journal receipt');
    const key = (operation === 'create' ? path.win32.join(directory, name) : directory).toLowerCase();
    const identity = JSON.stringify(receipt.directory_identity);
    if (identities.has(key) && identities.get(key) !== identity) throw Error('native journal directory identity changed');
    if (operation !== 'create') {
      if (operation === 'read') payload(receipt.value);
      const fileKey = `${key}\\${name}`;
      if (digests.has(fileKey) && digests.get(fileKey) !== receipt.sha256) throw Error('native journal immutable content changed');
      digests.set(fileKey, receipt.sha256);
    }
    identities.set(key, identity);
    return receipt;
  }
  return {
    async create(directory) {
      const reviewed = directoryPath(directory);
      await call('create', path.win32.dirname(reviewed), path.win32.basename(reviewed));
    },
    async write(directory, name, value) { await call('write', directory, name, payload(value)); },
    async read(directory, name) { return (await call('read', directory, name)).value; },
  };
}
