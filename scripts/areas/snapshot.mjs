#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Consistent PostgreSQL baseline + encrypted operator-selected files. This is
// not the final writer-stop cutover backup, nor proof that restore succeeded.
import fs from 'node:fs';
import fsp from 'node:fs/promises';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { pipeline } from 'node:stream/promises';
import { pathToFileURL } from 'node:url';

export function databaseEnvironment(config, inherited = process.env) {
  const blocks = [...config.matchAll(/^\[database\][ \t]*$([\s\S]*?)(?=^\[|$(?![\s\S]))/gm)];
  if (blocks.length !== 1) throw Error('Exactly one explicit database binding is required');
  const entries = [...blocks[0][1].matchAll(/^[ \t]*url[ \t]*=[ \t]*"([^"\r\n]+)"[ \t]*(?:#.*)?$/gm)];
  if (entries.length !== 1) throw Error('Exactly one explicit database URL is required');
  const raw = entries[0][1];
  let url;
  try { url = new URL(raw); } catch { throw Error('Explicit database.url is required'); }
  if (!['postgresql:', 'postgres:'].includes(url.protocol) || !url.hostname
      || !url.username || !/^\/[A-Za-z0-9_.-]+$/.test(url.pathname)
      || url.search || url.hash) throw Error('Unsupported database binding');
  // Do not put the URL/password in argv, error text or the evidence manifest.
  const base = Object.fromEntries(Object.entries(inherited).filter(([key]) =>
    ['PATH', 'LANG', 'LC_ALL', 'TZ', 'SYSTEMROOT'].includes(key)));
  const env = { ...base, PGHOST: url.hostname, PGPORT: url.port || '5432',
    PGDATABASE: url.pathname.slice(1), PGUSER: decodeURIComponent(url.username),
    PGPASSWORD: decodeURIComponent(url.password), PGCONNECT_TIMEOUT: '10',
    PGOPTIONS: '-c default_transaction_read_only=on -c timezone=UTC', PGPASSFILE: '/dev/null',
    PGSERVICEFILE: '/dev/null', PGSYSCONFDIR: '/nonexistent',
    PGAPPNAME: 'memory-area-readonly-snapshot' };
  // libpq treats an empty PGSERVICE as an explicit (missing) service name.
  delete env.PGSERVICE;
  return env;
}

export function parseArgs(args) {
  const options = { files: [] };
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i], value = args[i + 1];
    if (!value || value.startsWith('--')) throw Error('Every option requires a value');
    if (key === '--file') options.files.push(value);
    else if (['--database-config', '--out', '--recipient'].includes(key)) {
      if (options[key]) throw Error('Duplicate option');
      options[key] = value;
    } else throw Error('Unknown snapshot option');
  }
  if (!options['--database-config'] || !path.isAbsolute(options['--out'] || '')
      || !/^[A-Fa-f0-9]{40,64}$/.test(options['--recipient'] || ''))
    throw Error('Require --database-config FILE --out NEW_ABSOLUTE_DIRECTORY --recipient FULL_FINGERPRINT');
  options.files.forEach(file => {
    if (!path.isAbsolute(file) || file.includes('\0') || path.normalize(file) === '/')
      throw Error('Backup files must be explicit absolute paths, never filesystem root');
  });
  return options;
}

function child(command, args, env) {
  const process = spawn(command, args, { env, stdio: ['pipe', 'pipe', 'pipe'] });
  // Child stderr may describe a connection or sensitive filename. Drain it,
  // but expose only command name and exit status to the caller.
  process.stderr.resume();
  const done = new Promise((resolve, reject) => {
    process.once('error', () => reject(Error(`${command} could not start`)));
    process.once('close', code => code === 0 ? resolve() : reject(Error(`${command} failed (${code})`)));
  });
  return { process, done };
}

async function encryptCommand(command, args, env, output, recipient, input) {
  const source = child(command, args, env);
  const encryption = child('gpg', ['--batch', '--no-tty', '--encrypt', '--recipient', recipient,
    '--output', output], process.env);
  if (input === undefined) source.process.stdin.end();
  else source.process.stdin.end(input);
  const piping = pipeline(source.process.stdout, encryption.process.stdin);
  encryption.process.stdout.resume();
  try { await Promise.all([source.done, encryption.done, piping]); }
  catch (error) {
    source.process.kill('SIGTERM'); encryption.process.kill('SIGTERM');
    await Promise.allSettled([source.done, encryption.done, piping]);
    throw error;
  }
  await fsp.chmod(output, 0o600);
}

export async function hashFile(file) {
  const hash = createHash('sha256');
  for await (const chunk of fs.createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

export async function snapshot(options) {
  const out = path.resolve(options['--out']);
  const env = databaseEnvironment(await fsp.readFile(options['--database-config'], 'utf8'));
  // Refuse symlinks and directories: live stores need their own quiesced/export
  // procedure, not an unbounded recursive copy while writers are running.
  for (const file of options.files) {
    const stat = await fsp.lstat(file);
    if (!stat.isFile() || stat.isSymbolicLink()) throw Error('Only explicit regular files may be archived');
  }
  await fsp.mkdir(out, { mode: 0o700 }); // exclusive: no overwrite or reuse
  const oldUmask = process.umask(0o077);
  const manifest = { schema_version: 1, kind: 'online_baseline',
    started_at: new Date().toISOString(), recipient: options['--recipient'],
    database: env.PGDATABASE, database_session_timezone: 'UTC', artifacts: [],
    limitations: ['Not a final writer-stop backup.',
      'Roles, unreadable system configuration, offline stores and external files require separate capture.',
      'Selected files are not transactionally consistent with the database.',
      'Restore verification is required before migration.'] };
  try {
    const dump = path.join(out, 'database.pgdump.gpg');
    await encryptCommand('pg_dump', ['--format=custom', '--no-owner', '--no-privileges',
      '--serializable-deferrable'], env, dump, options['--recipient']);
    manifest.artifacts.push({ name: path.basename(dump), sha256: await hashFile(dump),
      bytes: (await fsp.stat(dump)).size });
    if (options.files.length) {
      const archive = path.join(out, 'selected-files.tar.gz.gpg');
      await encryptCommand('tar', ['--create', '--gzip', '--file=-', '--directory=/',
        '--null', '--verbatim-files-from', '--files-from=-'], process.env, archive,
      options['--recipient'], options.files.map(file => file.slice(1)).join('\0') + '\0');
      manifest.artifacts.push({ name: path.basename(archive), sha256: await hashFile(archive),
        bytes: (await fsp.stat(archive)).size });
      manifest.selected_file_count = options.files.length;
    }
    manifest.completed_at = new Date().toISOString();
    await fsp.writeFile(path.join(out, 'snapshot.json'), JSON.stringify(manifest, null, 2) + '\n',
      { mode: 0o600, flag: 'wx' });
    return manifest;
  } finally { process.umask(oldUmask); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  snapshot(parseArgs(process.argv.slice(2))).then(result => {
    console.log(JSON.stringify({ status: 'encrypted_baseline_created', artifacts: result.artifacts,
      restore_verified: false, cutover_ready: false }));
  }).catch(() => {
    console.error('Snapshot failed. Preserve the private output directory; no complete manifest means an incomplete backup. No credential details emitted.');
    process.exitCode = 1;
  });
}
