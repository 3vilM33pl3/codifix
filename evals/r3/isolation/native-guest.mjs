// SPDX-License-Identifier: AGPL-3.0-or-later
// Pinned driver inside the zero-capability AppContainer. No provider client.
import fs from 'node:fs/promises';
import { writeSync, mkdtempSync, openSync, closeSync, fstatSync, readSync } from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
const cap = 1024 * 1024;
// Static metadata only. Bounded private helper diagnostics distinguish module
// startup, pipe EOF and child-process failure without printing input contents.
const mark = stage => writeSync(2, `native_guest stage=${stage}\n`);
mark('stdin_begin');
let requestBytes = 0;
const chunks = [];
for await (const chunk of process.stdin) {
  requestBytes += chunk.length;
  if (requestBytes > 32 * cap) throw Error('native request exceeded budget');
  chunks.push(chunk);
}
mark('stdin_eof');
const request = JSON.parse(Buffer.concat(chunks));
if (request.version !== 1 || !Array.isArray(request.argv) || !request.argv.length
    || !/^[A-Za-z]:[\\/]/.test(request.argv[0]) || !Number.isInteger(request.timeoutMs)
    || request.timeoutMs < 1000 || request.timeoutMs > 60000 || !request.files || typeof request.files !== 'object') throw Error('invalid native request');
for (const [relative, content] of Object.entries(request.files)) {
  if (!relative.split('/').every(part => /^[a-zA-Z0-9_.-]+$/.test(part) && part !== '.' && part !== '..')
      || typeof content !== 'string') throw Error('unsafe native input');
  const target = path.resolve(relative);
  if (!target.startsWith(process.cwd() + path.sep)) throw Error('native input outside scratch');
  await fs.mkdir(path.dirname(target), { recursive: true });
  await fs.writeFile(target, content, { flag: 'wx' });
}
// libuv's Windows UV_CREATE_PIPE names are outside AppContainer's LOCAL
// namespace. In that token it can loop on access-denied before spawn returns.
// Explicit inherited regular-file descriptors avoid that path entirely; no
// network capability, ACL exemption or host pipe namespace is introduced.
const io = mkdtempSync(path.join(process.cwd(), '.memory-stdio-'));
const handles = [];
let result;
try {
  for (const name of ['stdin','stdout','stderr']) handles.push(openSync(path.join(io, name), 'wx+'));
  const sizes = () => handles.slice(1).map(fd => fstatSync(fd).size);
  const bounded = () => sizes().reduce((sum, size) => sum + size, 0) <= cap;
  const read = fd => {
    const length = fstatSync(fd).size;
    if (length > cap) throw Error('native command output budget exceeded');
    const bytes = Buffer.alloc(length);
    let offset = 0;
    while (offset < length) {
      const count = readSync(fd, bytes, offset, length - offset, offset);
      if (!count) throw Error('native command output changed during read');
      offset += count;
    }
    return bytes.toString('utf8');
  };
  result = await new Promise((resolve, reject) => {
  // The reviewed Node interpreter must not realpath-walk the drive root for
  // relative harness scripts either. This is not a new tool or ACL grant;
  // non-Node tools retain their literal argument vectors unchanged.
  const args = path.resolve(request.argv[0]).toLowerCase() === path.resolve(process.execPath).toLowerCase()
    ? ['--preserve-symlinks','--preserve-symlinks-main',...request.argv.slice(1)] : request.argv.slice(1);
  mark('child_start');
  const child = spawn(request.argv[0], args, { shell: false, windowsHide: true,
    cwd: process.cwd(), env: { ...process.env }, stdio: handles });
  mark('child_created');
  let settled = false;
  const fail = error => {
    if (settled) return;
    settled = true; clearTimeout(timer); clearInterval(monitor);
    // Exact owned child only; the trusted helper owns whole-Job reaping.
    try { child.kill('SIGKILL'); } catch { /* helper still owns whole-Job cancellation */ }
    reject(error);
  };
  const timer = setTimeout(() => fail(Error('native command deadline expired')), request.timeoutMs);
  // Scratch has no hard disk quota. This is an output-content bound and prompt
  // cancellation check, not a promise to cap bytes written between polls.
  const monitor = setInterval(() => {
    try { if (!bounded()) fail(Error('native command output budget exceeded')); }
    catch { fail(Error('native command output unavailable')); }
  }, 10);
  child.on('error', () => {
    mark('child_error');
    // Native commands are already pinned inventory members. Missing runtime
    // DLLs or access failures invalidate execution, not the submitted answer.
    fail(Error('native command startup failed'));
  });
  child.on('close', code => {
    mark('child_close');
    if (settled) return;
    settled = true; clearTimeout(timer); clearInterval(monitor);
    if (!Number.isInteger(code) || code < -0x80000000 || code > 0xffffffff) {
      reject(Error('native command exit unavailable')); return;
    }
    const unsigned = code >>> 0;
    // CreateProcess can succeed before loader failure. These statuses mean
    // missing DLL / DLL initialization failure, not a failed benchmark answer.
    // Mirror the trusted-host check: a guest result is never grading authority.
    if (unsigned === 0xc0000135 || unsigned === 0xc0000142) {
      reject(Error('native toolchain loader failure invalidates execution')); return;
    }
    try {
      if (!bounded()) throw Error('native command output budget exceeded');
      resolve({ exitCode: unsigned, stdout: read(handles[1]), stderr: read(handles[2]) });
    } catch (error) { reject(error); }
  });
  });
} finally { for (const fd of handles) closeSync(fd); }
mark('result');
// The host helper reaps the whole Job before exporting selected regular files.
process.stdout.write(JSON.stringify({ version: 1, ...result }));
