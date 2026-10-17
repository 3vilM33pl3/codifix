#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// No credentials/settings enter this short-lived driver or agent environments.
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { fileURLToPath } from 'node:url';
try {
  if (process.env.CODIFIX_R3_OFFLINE !== '0' || process.argv.length !== 3) throw Error('approved dispatch only');
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const inputPath = path.resolve(process.argv[2]);
  if (inputPath !== path.join(process.cwd(), 'request.json')) throw Error('wrong request');
  const input = await fs.readFile(inputPath);
  if (input.length > 32768) throw Error('oversized');
  const request = JSON.parse(input), preparing = request.phase === 'preparation';
  if (request.offline !== false || request.manifest !== path.join(root, 'campaign.json')
      || request.response !== path.join(process.cwd(), preparing ? 'receipt.json' : 'observation.json')) throw Error('invalid dispatch');
  const bytes = await new Promise((resolve, reject) => {
    const call = http.request({ socketPath: path.join(root, 'private/dispatch.sock'), path: '/dispatch', method: 'POST',
      headers: { 'Content-Length': input.length }, timeout: 590000 }, response => {
      const chunks = []; let size = 0;
      response.on('data', chunk => { if ((size += chunk.length) > 16 * 1024 * 1024) call.destroy(Error('oversized')); else chunks.push(chunk); });
      response.once('end', () => response.statusCode === 200 ? resolve(Buffer.concat(chunks)) : reject(Error('refused')));
      response.once('error', reject);
    });
    call.once('timeout', () => call.destroy(Error('deadline'))); call.once('error', reject); call.end(input);
  });
  JSON.parse(bytes);
  const file = await fs.open(request.response, 'wx', 0o600);
  try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
} catch {
  console.error('Approved R3 dispatch refused or interrupted. Retain evidence; no automatic retry.');
  process.exitCode = 1;
}
