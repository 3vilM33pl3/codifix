// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted, read-only supervisor. The evaluated command never owns this file.
import fs from 'node:fs/promises';
import { constants } from 'node:fs';
import { spawn } from 'node:child_process';
import path from 'node:path';

const request = JSON.parse(await fs.readFile('/input/request.json', 'utf8'));
const cap = 1024 * 1024;
const environment = { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: '/tmp/home', LANG: 'C.UTF-8' };
await fs.mkdir(environment.HOME, { recursive: true });
await fs.cp('/input/files', '/work', { recursive: true });
const stdout = [], stderr = [];
let bytes = 0;
const exitCode = await new Promise((resolve, reject) => {
  const child = spawn(request.argv[0], request.argv.slice(1), {
    cwd: '/work', env: environment, stdio: ['ignore', 'pipe', 'pipe'], detached: true,
  });
  const stop = () => { try { process.kill(-child.pid, 'SIGKILL'); } catch {} };
  const timer = setTimeout(() => { stop(); reject(Error('command deadline exceeded')); }, request.commandTimeoutMs);
  for (const [stream, output] of [[child.stdout, stdout], [child.stderr, stderr]]) {
    stream.on('data', value => {
      bytes += value.length;
      if (bytes > cap) { stop(); reject(Error('command output limit exceeded')); }
      else output.push(value);
    });
  }
  child.on('error', () => { clearTimeout(timer); reject(Error('command startup failed')); });
  child.on('close', code => { clearTimeout(timer); stop(); resolve(code); });
});
// Only regular, singly linked files cross the container boundary. No tar extraction,
// executable permissions, symlinks, devices or paths supplied by stdout are used.
const files = Object.create(null);
let total = 0, count = 0;
async function collect(directory, prefix = '') {
  for (const name of (await fs.readdir(directory)).sort()) {
    const relative = prefix + name;
    if (request.exportPaths && !request.exportPaths.some(p => p === relative || p.startsWith(relative + '/'))) continue;
    if (!/^[a-zA-Z0-9_.-]+$/.test(name) || name === '.' || name === '..') throw Error('unsafe output name');
    const location = path.join(directory, name);
    const stat = await fs.lstat(location);
    if (++count > 512) throw Error('output file count exceeded');
    if (stat.isDirectory()) {
      if (request.exportPaths?.includes(relative)) throw Error('export must be a file');
      await collect(location, relative + '/'); continue;
    }
    if (request.exportPaths && !request.exportPaths.includes(relative)) throw Error('export parent must be a directory');
    if (!stat.isFile() || stat.nlink !== 1 || stat.size > cap) throw Error('unsafe output file');
    const handle = await fs.open(location, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const current = await handle.stat();
      if (!current.isFile() || current.nlink !== 1 || current.size > cap) throw Error('unsafe output file');
      const content = await handle.readFile();
      total += content.length;
      if (total > cap) throw Error('output byte limit exceeded');
      files[relative] = content.toString('base64');
    } finally { await handle.close(); }
  }
}
await collect('/work');
process.stdout.write(JSON.stringify({ version: 1, exitCode,
  stdout: Buffer.concat(stdout).toString('utf8'), stderr: Buffer.concat(stderr).toString('utf8'), files }));
