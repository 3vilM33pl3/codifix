// SPDX-License-Identifier: AGPL-3.0-or-later
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { Writable } from 'node:stream';
import { serveFrames } from './http-stdio.mjs';

// Save the protocol writer before redirecting every ordinary stdout logger.
// The trusted Rust broker drains/discards stderr rather than displaying content.
const protocolWrite = process.stdout.write.bind(process.stdout);
const output = new Writable({ write(chunk, encoding, callback) { protocolWrite(chunk, encoding, callback); } });
process.stdout.write = process.stderr.write.bind(process.stderr);
let pds;
try {
  if (process.versions.node !== '24.18.1') throw new Error('pinned Node required');
  const require = createRequire(import.meta.url);
  const entry = require.resolve('@atproto/pds');
  const metadata = JSON.parse(readFileSync(join(dirname(entry), '..', 'package.json'), 'utf8'));
  if (metadata.version !== '0.5.27') throw new Error('pinned PDS required');
  const { PDS } = await import(pathToFileURL(entry).href);
  pds = await PDS.fromEnv();
  // Audited against pinned PDS.start(): its only initialization before listen
  // is sequencer.start(). keepAliveTimeout is set by dispatch's HTTP server.
  // PDS.destroy() always destroys ctx even without its optional TCP terminator.
  await pds.ctx.sequencer.start();
  await serveFrames(pds.app, process.stdin, output);
} catch {
  process.stderr.write('native PDS stdio stopped; inspect protected operator state\n');
  process.exitCode = 1;
} finally {
  await pds?.destroy();
}
