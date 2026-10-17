#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Hash-locked host-side campaign driver. No socket/config is mounted in agents.
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createWindowsHostJournal } from './windows-host-journal.mjs';
import { requestWindowsDispatch } from './windows-dispatch-client.mjs';

try {
  if (process.env.MEMORY_R3_OFFLINE !== '1' || process.argv.length !== 3) throw Error('offline only');
  const inputPath = path.resolve(process.argv[2]);
  if (inputPath !== path.join(process.cwd(), 'request.json')) throw Error('wrong request path');
  const input = await fs.readFile(inputPath);
  if (input.length > 32768) throw Error('oversized request');
  const request = JSON.parse(input);
  const preparing = request.phase === 'preparation';
  if (request.offline !== true || request.maximum_microusd !== 0
      || request.response !== path.join(process.cwd(), preparing ? 'receipt.json' : 'observation.json')) throw Error('invalid dispatch');
  const configurationBytes = await fs.readFile(new URL('../private/runtime.json', import.meta.url));
  const config = JSON.parse(configurationBytes);
  const native = config.execution?.kind === 'windows-native-v1';
  if ((process.platform === 'win32' && !native) || (config.execution !== undefined && !native)) throw Error('explicit native driver required');
  const journal = native ? await createWindowsHostJournal(config.execution) : undefined;
  if (native && (config.syntheticWire || config.socket !== undefined || config.images !== undefined
      || config.dispatch?.kind !== 'windows-loopback-v1' || config.dispatch?.endpoint !== 'private/dispatch-endpoint.json'
      || Object.keys(config.dispatch).sort().join(',') !== 'endpoint,kind')) throw Error('invalid native driver contract');
  const endpoint = native ? await journal.read(path.dirname(fileURLToPath(new URL('../private/runtime.json', import.meta.url))), 'dispatch-endpoint.json') : undefined;
  const response = native ? await requestWindowsDispatch({ endpoint, input,
    expected: { manifestSha256: request.manifest_sha256,
      configurationSha256: createHash('sha256').update(configurationBytes).digest('hex') } }) : await new Promise((resolve, reject) => {
    const call = http.request({ socketPath: config.socket, path: '/dispatch', method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': input.length }, timeout: 590000 }, result => {
      let bytes = 0;
      const chunks = [];
      result.on('data', chunk => {
        bytes += chunk.length;
        if (bytes > 16 * 1024 * 1024) call.destroy(Error('oversized response'));
        else chunks.push(chunk);
      });
      result.once('end', () => result.statusCode === 200 ? resolve(Buffer.concat(chunks)) : reject(Error('dispatch failed')));
      result.once('error', reject);
    });
    call.once('timeout', () => call.destroy(Error('dispatch deadline')));
    call.once('error', reject);
    call.end(input);
  });
  JSON.parse(response); // Complete bounded JSON; the Rust runner validates its contract.
  if (journal) {
    await journal.write(process.cwd(), path.basename(request.response), JSON.parse(response));
    if (!preparing) await journal.write(process.cwd(), 'preparation-microusd.json', 0);
  } else {
    await fs.writeFile(request.response, response, { flag: 'wx', mode: 0o600 });
    if (!preparing) await fs.writeFile(path.join(process.cwd(), 'preparation-microusd.json'), '0', { flag: 'wx', mode: 0o600 });
  }
} catch {
  console.error('Offline R3 dispatch failed; no automatic repeat was attempted.');
  process.exitCode = 1;
}
