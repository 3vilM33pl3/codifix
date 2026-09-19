// SPDX-License-Identifier: AGPL-3.0-or-later
import { constants } from 'node:fs';
import { open, lstat, rename, unlink } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, normalize } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';

export class OperatorError extends Error {
  constructor(code) { super(code); this.code = code; }
}
export function check(condition, code = 'invalid_configuration') {
  if (!condition) throw new OperatorError(code);
}
export const hash = bytes => createHash('sha256').update(bytes).digest('hex');
export function canonical(value) {
  if (value === null || typeof value === 'boolean' || typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'number') { check(Number.isSafeInteger(value)); return String(value); }
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  check(value && Object.getPrototypeOf(value) === Object.prototype);
  return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
}
export const digest = value => hash(canonical(value));
export function strict(value, keys) {
  check(value && Object.getPrototypeOf(value) === Object.prototype
    && Object.keys(value).every(key => keys.includes(key))
    && keys.every(key => Object.hasOwn(value, key)));
}
export function absolute(path) {
  check(typeof path === 'string' && path.length <= 512 && isAbsolute(path)
    && normalize(path) === path && path.split('/').filter(Boolean).length >= 2
    && !/[\u0000-\u001f\u007f]/u.test(path));
  return path;
}

// JSON.parse alone silently discards duplicate keys. Detect them recursively
// before accepting any private configuration, journal, session or wire response.
export function parseJson(text) {
  let at = 0;
  const whitespace = () => { while (/\s/u.test(text[at] ?? '') && at < text.length) at++; };
  function string() {
    check(text[at] === '"');
    const start = at++;
    while (at < text.length) {
      if (text[at] === '\\') { at += 2; continue; }
      if (text[at++] === '"') return JSON.parse(text.slice(start, at));
    }
    throw new OperatorError('invalid_json');
  }
  function value(depth = 0) {
    check(depth < 32, 'invalid_json'); whitespace();
    if (text[at] === '{') {
      at++; whitespace(); const keys = new Set();
      if (text[at] === '}') { at++; return; }
      while (at < text.length) {
        whitespace(); const key = string(); check(!keys.has(key), 'duplicate_json_key'); keys.add(key);
        whitespace(); check(text[at++] === ':', 'invalid_json'); value(depth + 1); whitespace();
        const end = text[at++]; if (end === '}') return; check(end === ',', 'invalid_json');
      }
    } else if (text[at] === '[') {
      at++; whitespace(); if (text[at] === ']') { at++; return; }
      while (at < text.length) { value(depth + 1); whitespace(); const end = text[at++]; if (end === ']') return; check(end === ',', 'invalid_json'); }
    } else if (text[at] === '"') string();
    else { const token = /^(?:true|false|null|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)/u.exec(text.slice(at)); check(token, 'invalid_json'); at += token[0].length; }
  }
  try { value(); whitespace(); check(at === text.length, 'invalid_json'); return JSON.parse(text); }
  catch (error) { if (error instanceof OperatorError) throw error; throw new OperatorError('invalid_json'); }
}

const uid = () => process.geteuid();
const fdPath = (directory, name) => `/proc/self/fd/${directory.fd}${name ? `/${name}` : ''}`;
function safeDirectory(meta, privateFinal = false) {
  check(meta.isDirectory(), 'unsafe_parent_directory');
  if (privateFinal) {
    check(meta.uid === uid() && (meta.mode & 0o777) === 0o700, 'private_state_directory_required');
  } else {
    const trustedTemporaryRoot = meta.uid === 0 && (meta.mode & 0o1000) !== 0;
    check((meta.uid === 0 || meta.uid === uid()) && ((meta.mode & 0o022) === 0 || trustedTemporaryRoot), 'unsafe_parent_directory');
  }
}
// Linux openat-equivalent via the process's own pinned directory descriptors.
// Walk every component without following symlinks; a writable foreign ancestor
// must not be able to substitute an alternative private-looking child directory.
async function directoryHandle(path, privateFinal = false) {
  check(process.platform === 'linux' && typeof process.geteuid === 'function', 'linux_safe_filesystem_required');
  let directory = await open('/', constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    safeDirectory(await directory.stat());
    for (const part of path.split('/').filter(Boolean)) {
      const next = await open(fdPath(directory, part), constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
      try { safeDirectory(await next.stat()); } catch (error) { await next.close(); throw error; }
      await directory.close(); directory = next;
    }
    safeDirectory(await directory.stat(), privateFinal); return directory;
  } catch (error) { await directory.close(); throw error; }
}
export function safeFile(meta, limit, publicFile) {
  check(meta.isFile() && meta.nlink === 1 && meta.size <= limit
    && (publicFile ? (meta.uid === 0 || meta.uid === uid()) && (meta.mode & 0o022) === 0
      : meta.uid === uid() && (meta.mode & 0o777) === 0o600), 'unsafe_private_file');
}
async function readAt(directory, name, { optional = false, limit = 65536, publicFile = false } = {}) {
  let file;
  try {
    file = await open(fdPath(directory, name), constants.O_RDONLY | constants.O_NOFOLLOW);
    const before = await file.stat(); safeFile(before, limit, publicFile);
    const bytes = await file.readFile(); const after = await file.stat(); safeFile(after, limit, publicFile);
    check(bytes.length <= limit && before.size === after.size && before.mtimeMs === after.mtimeMs, 'private_file_changed');
    return bytes;
  } catch (error) {
    if (optional && error.code === 'ENOENT') return null;
    if (error instanceof OperatorError) throw error;
    throw new OperatorError('private_file_unavailable');
  } finally { await file?.close(); }
}
export async function privateRead(path, options) {
  absolute(path);
  const directory = await directoryHandle(dirname(path));
  try { return await readAt(directory, basename(path), options); } finally { await directory.close(); }
}

export class StateFiles {
  constructor(directory, fault = async () => {}) { this.directory = absolute(directory); this.fault = fault; this.bound = null; }
  async validate() {
    await this.withDirectory(async () => {});
  }
  path(name) { check(['journal.json', 'session.json', 'did.json', 'operator.lock'].includes(name)); return join(this.directory, name); }
  async checkBinding(directory) {
    const current = await directoryHandle(this.directory, true);
    try { const a = await current.stat(); const b = await directory.stat(); check(a.dev === b.dev && a.ino === b.ino, 'state_directory_replaced'); }
    finally { await current.close(); }
  }
  async withDirectory(work) {
    if (this.bound) { await this.checkBinding(this.bound); return work(this.bound); }
    const directory = await directoryHandle(this.directory, true);
    try { return await work(directory); } finally { await directory.close(); }
  }
  async bytes(name) { this.path(name); return this.withDirectory(directory => readAt(directory, name, { optional: true })); }
  async read(name) {
    const bytes = await this.bytes(name);
    return bytes ? parseJson(bytes.toString('utf8')) : null;
  }
  async write(name, data) {
    this.path(name);
    return this.withDirectory(async directory => {
      await readAt(directory, name, { optional: true });
      const temporary = `.pending-${randomUUID()}`;
      let file; let owned; let renamed = false;
      try {
        file = await open(fdPath(directory, temporary), constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
        owned = await file.stat(); safeFile(owned, 65536, false);
        await file.writeFile(`${canonical(data)}\n`);
        await this.fault('before_file_sync', name);
        await this.checkBinding(directory);
        await file.sync(); await file.close(); file = null;
        // Recheck any destination after the asynchronous write; no symlink or
        // hard-linked file is silently replaced by a state commit.
        await readAt(directory, name, { optional: true });
        await this.verifyOwned(directory, temporary, owned);
        await rename(fdPath(directory, temporary), fdPath(directory, name)); renamed = true;
        await directory.sync();
        await this.fault('after_atomic_write', name);
        await this.checkBinding(directory);
      } finally {
        await file?.close();
        if (owned && !renamed) await this.unlinkOwned(directory, temporary, owned);
      }
    });
  }
  async verifyOwned(directory, name, owned) {
    const current = await lstat(fdPath(directory, name));
    safeFile(current, 65536, false);
    check(current.dev === owned.dev && current.ino === owned.ino, 'owned_state_file_replaced');
  }
  async unlinkOwned(directory, name, owned) {
    await this.verifyOwned(directory, name, owned);
    await unlink(fdPath(directory, name));
  }
  async locked(work) {
    check(!this.bound, 'operator_locked_manual_recovery_required');
    const directory = await directoryHandle(this.directory, true);
    let lock; let owned;
    try {
      try { lock = await open(fdPath(directory, 'operator.lock'), constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600); }
      catch { throw new OperatorError('operator_locked_manual_recovery_required'); }
      owned = await lock.stat(); safeFile(owned, 65536, false); this.bound = directory;
      await lock.writeFile(canonical({ process_id: process.pid })); await lock.sync();
      return await work();
    } finally {
      this.bound = null;
      try { await lock?.close(); if (owned) await this.unlinkOwned(directory, 'operator.lock', owned); }
      finally { await directory.close(); }
    }
  }
}
