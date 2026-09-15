// SPDX-License-Identifier: AGPL-3.0-or-later
// Building block for the existing R3 campaign driver, not an evaluation/scoring engine.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
const exec = promisify(execFile);
const docker = (args, timeout = 15000) => exec('/usr/bin/docker', ['--host', 'unix:///var/run/docker.sock', ...args], {
  env: { PATH: '/usr/bin:/bin' }, timeout, maxBuffer: 4 * 1024 * 1024,
  killSignal: 'SIGKILL', encoding: 'utf8',
});
const cap = 1024 * 1024;

export function validateInput({ image, argv, files, timeoutMs = 10000, exportPaths, profile = 'text' }) {
  if (!['text', 'rust'].includes(profile)) throw Error('invalid resource profile');
  if (!/^sha256:[a-f0-9]{64}$/.test(image)) throw Error('require a reviewed immutable local image ID');
  if (!Array.isArray(argv) || !argv.length || argv.length > 64 || argv.some(v => typeof v !== 'string' || v.includes('\0') || v.length > 32768)) throw Error('invalid command');
  if (!argv[0].startsWith('/')) throw Error('command executable must be absolute');
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1000 || timeoutMs > 60000) throw Error('invalid deadline');
  if (!files || Object.getPrototypeOf(files) !== Object.prototype) throw Error('files must be an explicit map');
  if (exportPaths !== undefined) {
    if (!Array.isArray(exportPaths) || exportPaths.length > 256 || new Set(exportPaths).size !== exportPaths.length) throw Error('invalid export selection');
    for (const name of exportPaths) {
      if (typeof name !== 'string' || name.length > 240 || !name.split('/').every(p => /^[a-zA-Z0-9_.-]+$/.test(p) && p !== '.' && p !== '..')) throw Error('unsafe export path');
      if (exportPaths.some(other => typeof other === 'string' && other.startsWith(name + '/'))) throw Error('conflicting export paths');
    }
  }
  let bytes = 0;
  const names = Object.keys(files);
  if (names.length > 256) throw Error('input file count exceeded');
  for (const name of names) {
    if (name.length > 240 || !name.split('/').every(p => /^[a-zA-Z0-9_.-]+$/.test(p) && p !== '.' && p !== '..')) throw Error('unsafe input path');
    if (typeof files[name] !== 'string') throw Error('input must be UTF-8 text');
    bytes += Buffer.byteLength(files[name]);
    if (bytes > cap) throw Error('input byte limit exceeded');
    if (names.some(other => other.startsWith(name + '/'))) throw Error('conflicting input paths');
  }
}

// The guest is an execution boundary, not a grading authority. Treat its entire
// response as untrusted submission data, even if the process returned success.
export function validateResult(result) {
  if (!result || Object.getPrototypeOf(result) !== Object.prototype
      || Object.keys(result).sort().join(',') !== 'exitCode,files,stderr,stdout,version') throw Error('unexpected result fields');
  if (!result || result.version !== 1 || !Number.isInteger(result.exitCode) || result.exitCode < 0 || result.exitCode > 255) throw Error('invalid container result');
  if (typeof result.stdout !== 'string' || typeof result.stderr !== 'string'
      || Buffer.byteLength(result.stdout) + Buffer.byteLength(result.stderr) > cap) throw Error('invalid result output');
  if (!result.files || Object.getPrototypeOf(result.files) !== Object.prototype || Object.keys(result.files).length > 256) throw Error('invalid output files');
  let bytes = 0;
  for (const [name, encoded] of Object.entries(result.files)) {
    if (name.length > 240 || !name.split('/').every(p => /^[a-zA-Z0-9_.-]+$/.test(p) && p !== '.' && p !== '..')) throw Error('unsafe output path');
    if (typeof encoded !== 'string' || encoded.length > 2 * cap) throw Error('invalid file encoding');
    const data = Buffer.from(encoded, 'base64');
    if (data.toString('base64') !== encoded || (bytes += data.length) > cap) throw Error('invalid or excessive output bytes');
  }
}

export async function runOfflineContainer(options) {
  validateInput(options);
  // Preserve the reviewed contract across image inspection and staging awaits.
  const { image, argv, files, timeoutMs = 10000, exportPaths, profile = 'text' } = structuredClone(options);
  const memory = profile === 'rust' ? '512m' : '256m';
  const scratch = profile === 'rust' ? '128m' : '16m';
  const executable = profile === 'rust' ? 'exec' : 'noexec';
  const fileSize = profile === 'rust' ? '33554432' : '2097152';
  const uid = process.getuid(), gid = process.getgid();
  if (!uid || !gid) throw Error('run the supervisor as a non-root operator');
  // No registry login, implicit pull, custom Docker context or inherited credentials.
  const metadata = JSON.parse((await docker(['image', 'inspect', image])).stdout)[0];
  if (metadata.Id !== image || Object.keys(metadata.Config.Volumes ?? {}).length) throw Error('image identity/implicit volumes rejected');
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-container-'));
  const name = `memory-r3-${randomUUID()}`;
  let created = false;
  try {
    await fs.mkdir(path.join(directory, 'files'));
    for (const [relative, content] of Object.entries(files)) {
      const target = path.join(directory, 'files', relative);
      await fs.mkdir(path.dirname(target), { recursive: true });
      await fs.writeFile(target, content, { flag: 'wx', mode: 0o600 });
    }
    await fs.writeFile(path.join(directory, 'request.json'), JSON.stringify({ argv, commandTimeoutMs: timeoutMs, exportPaths }), { flag: 'wx', mode: 0o600 });
    const guest = fileURLToPath(new URL('./guest.mjs', import.meta.url));
    // Mark before create: a lost successful response still requires exact-name cleanup.
    created = true;
    await docker(['create', '--name', name, '--label', 'memory.r3.disposable=true',
      '--pull', 'never', '--network', 'none', '--no-healthcheck', '--read-only', '--cap-drop', 'ALL',
      '--security-opt', 'no-new-privileges', '--user', `${uid}:${gid}`, '--pids-limit', '64',
      '--memory', memory, '--memory-swap', memory, '--cpus', '1', '--ulimit', `fsize=${fileSize}:${fileSize}`,
      '--tmpfs', `/work:rw,nosuid,nodev,${executable},size=${scratch},uid=${uid},gid=${gid},mode=0700`,
      '--tmpfs', `/tmp:rw,nosuid,nodev,noexec,size=16m,uid=${uid},gid=${gid},mode=0700`,
      '--mount', `type=bind,src=${directory},dst=/input,readonly`,
      '--mount', `type=bind,src=${guest},dst=/runner.mjs,readonly`,
      '--workdir', '/work', '--entrypoint', '/usr/bin/env', image,
      '-i', 'PATH=/usr/local/bin:/usr/bin:/bin', '/usr/local/bin/node', '/runner.mjs']);
    const started = performance.now();
    const { stdout } = await docker(['start', '--attach', name], timeoutMs + 5000);
    const inspection = JSON.parse((await docker(['inspect', name])).stdout)[0];
    if (inspection.State.Running || inspection.State.ExitCode !== 0 || inspection.State.OOMKilled) throw Error('offline container failed');
    const result = JSON.parse(stdout);
    validateResult(result);
    if (exportPaths && Object.keys(result.files).some(name => !exportPaths.includes(name))) throw Error('unsolicited export');
    return { ...result, elapsedMs: performance.now() - started, image };
  } catch {
    // Do not surface evaluated stdout, provider text, host config or Docker errors.
    throw Error(`offline execution failed; owned container ${name}`);
  } finally {
    if (created) {
      try { await docker(['rm', '--force', name]); }
      catch {
        // Retain the staging directory when cleanup is ambiguous. No other
        // container or workspace is a recovery target.
        throw Error(`reconcile owned container ${name}; staging retained at ${directory}`);
      }
    }
    await fs.rm(directory, { recursive: true });
  }
}
