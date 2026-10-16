// SPDX-License-Identifier: AGPL-3.0-or-later
// Metadata-only startup smoke against an explicitly supplied disposable command.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { MAX_FRAME, READY } from './http-stdio.mjs';
const [command, ...args] = process.argv.slice(2);
if (!command) throw new Error('supply an explicit disposable PDS command');
const child = spawn(command, args, { stdio: ['pipe', 'pipe', 'pipe'] });
let stderrBytes = 0; child.stderr.on('data', chunk => { stderrBytes += chunk.length; });
let pending = Buffer.alloc(0);
const frames = [];
let wake;
child.stdout.on('data', chunk => {
  pending = Buffer.concat([pending, chunk]);
  while (pending.length >= 4) {
    const size = pending.readUInt32BE();
    if (size > MAX_FRAME) { child.kill(); throw new Error('invalid protocol frame'); }
    if (pending.length < size + 4) break;
    frames.push(pending.subarray(4,size+4)); pending = pending.subarray(size+4);
    wake?.(); wake = undefined;
  }
});
const frame = async () => {
  while (!frames.length) await new Promise(resolve => { wake = resolve; });
  return frames.shift();
};
const deadline = setTimeout(() => { child.kill(); console.error('PDS startup smoke timed out'); process.exit(1); }, 60_000);
try {
  assert.deepEqual(await frame(), READY);
  const request = Buffer.from('GET /xrpc/com.atproto.server.describeServer HTTP/1.1\r\nHost: pds.federation.example.com\r\nConnection: close\r\n\r\n');
  const prefix = Buffer.alloc(4); prefix.writeUInt32BE(request.length);
  child.stdin.write(Buffer.concat([prefix,request]));
  const response = await frame();
  assert.match(response.subarray(0,response.indexOf('\r\n')).toString(), /^HTTP\/1.1 200 /);
  assert.match(response.toString(), /availableUserDomains/);
  child.stdin.end();
  const [code] = await once(child, 'exit');
  assert.equal(code, 0);
  console.log(JSON.stringify({ ready: true, describe_server: 200, clean_eof_shutdown: true, stderr_bytes_discarded: stderrBytes }));
} finally { clearTimeout(deadline); if (child.exitCode === null) child.kill(); }
