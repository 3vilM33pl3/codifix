#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Hash-locked host-side campaign driver. No socket/config is mounted in agents.
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';

try {
  if (process.env.MEMORY_R3_OFFLINE !== '1' || process.argv.length !== 3) throw Error('offline only');
  const inputPath = path.resolve(process.argv[2]);
  if (inputPath !== path.join(process.cwd(), 'request.json')) throw Error('wrong request path');
  const input = await fs.readFile(inputPath);
  if (input.length > 32768) throw Error('oversized request');
  const request = JSON.parse(input);
  if (request.offline !== true || request.maximum_microusd !== 0 || request.response !== path.join(process.cwd(), 'observation.json')) throw Error('invalid dispatch');
  const config = JSON.parse(await fs.readFile(new URL('../private/runtime.json', import.meta.url)));
  const response = await new Promise((resolve, reject) => {
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
  await fs.writeFile(request.response, response, { flag: 'wx', mode: 0o600 });
  await fs.writeFile(path.join(process.cwd(), 'preparation-microusd.json'), '0', { flag: 'wx', mode: 0o600 });
} catch {
  console.error('Offline R3 dispatch failed; no automatic repeat was attempted.');
  process.exitCode = 1;
}
