// SPDX-License-Identifier: AGPL-3.0-or-later
// Pinned driver inside the zero-capability AppContainer. No provider client.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawn } from 'node:child_process';
const cap = 1024 * 1024;
let requestBytes = 0;
const chunks = [];
for await (const chunk of process.stdin) {
  requestBytes += chunk.length;
  if (requestBytes > 32 * cap) throw Error('native request exceeded budget');
  chunks.push(chunk);
}
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
const stdout = [], stderr = [];
let total = 0;
const exitCode = await new Promise((resolve, reject) => {
  const child = spawn(request.argv[0], request.argv.slice(1), { shell: false, windowsHide: true,
    cwd: process.cwd(), env: { ...process.env }, stdio: ['ignore','pipe','pipe'] });
  const timer = setTimeout(() => reject(Error('native command deadline expired')), request.timeoutMs);
  for (const [stream, destination] of [[child.stdout, stdout], [child.stderr, stderr]]) stream.on('data', bytes => {
    total += bytes.length;
    if (total > cap) reject(Error('native command output budget exceeded'));
    else destination.push(bytes);
  });
  child.on('error', () => {
    clearTimeout(timer);
    // Native commands are already pinned inventory members. Missing runtime
    // DLLs or access failures invalidate execution, not the submitted answer.
    reject(Error('native command startup failed'));
  });
  child.on('close', code => { clearTimeout(timer); Number.isInteger(code) ? resolve(code) : reject(Error('native command exit unavailable')); });
});
// The host helper reaps the whole Job before exporting selected regular files.
process.stdout.write(JSON.stringify({ version: 1, exitCode,
  stdout: Buffer.concat(stdout).toString('utf8'), stderr: Buffer.concat(stderr).toString('utf8') }));
