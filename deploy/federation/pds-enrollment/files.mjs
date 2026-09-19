// SPDX-License-Identifier: AGPL-3.0-or-later
import { constants } from 'node:fs';
import { open, lstat, rename, unlink } from 'node:fs/promises';
import { isAbsolute, join, normalize } from 'node:path';
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

export async function privateRead(path, { optional = false, limit = 65536, publicFile = false } = {}) {
  absolute(path);
  let file;
  try {
    file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    const meta = await file.stat();
    check(meta.isFile() && meta.size <= limit
      && (publicFile || (meta.mode & 0o777) === 0o600), 'unsafe_private_file');
    const bytes = await file.readFile(); check(bytes.length <= limit, 'unsafe_private_file'); return bytes;
  } catch (error) {
    if (optional && error.code === 'ENOENT') return null;
    if (error instanceof OperatorError) throw error;
    throw new OperatorError('private_file_unavailable');
  } finally { await file?.close(); }
}

export class StateFiles {
  constructor(directory, fault = async () => {}) { this.directory = absolute(directory); this.fault = fault; }
  async validate() {
    const meta = await lstat(this.directory).catch(() => { throw new OperatorError('private_state_directory_required'); });
    check(meta.isDirectory() && !meta.isSymbolicLink() && (meta.mode & 0o777) === 0o700, 'private_state_directory_required');
  }
  path(name) { check(['journal.json', 'session.json', 'did.json', 'operator.lock'].includes(name)); return join(this.directory, name); }
  async read(name) {
    const bytes = await privateRead(this.path(name), { optional: true });
    return bytes ? parseJson(bytes.toString('utf8')) : null;
  }
  async write(name, data) {
    const target = this.path(name);
    // Existing destinations must already be private regular files, never aliases.
    await privateRead(target, { optional: true });
    const temporary = join(this.directory, `.pending-${randomUUID()}`);
    let file;
    try {
      file = await open(temporary, 'wx', 0o600);
      await file.writeFile(`${canonical(data)}\n`);
      await this.fault('before_file_sync', name);
      await file.sync(); await file.close(); file = null;
      await rename(temporary, target);
      const directory = await open(this.directory, constants.O_RDONLY);
      try { await directory.sync(); } finally { await directory.close(); }
      await this.fault('after_atomic_write', name);
    } finally {
      await file?.close();
      // Only this call's uniquely named temporary file is eligible for cleanup.
      await unlink(temporary).catch(error => { if (error.code !== 'ENOENT') throw new OperatorError('private_write_unconfirmed'); });
    }
  }
  async locked(work) {
    await this.validate();
    let lock;
    try { lock = await open(this.path('operator.lock'), 'wx', 0o600); }
    catch { throw new OperatorError('operator_locked_manual_recovery_required'); }
    try {
      await lock.writeFile(canonical({ process_id: process.pid })); await lock.sync();
      return await work();
    } finally { await lock.close(); await unlink(this.path('operator.lock')); }
  }
}
