// SPDX-License-Identifier: AGPL-3.0-or-later
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { dispatch, MAX_FRAME, READY, serveFrames } from './http-stdio.mjs';

test('real Node HTTP parser preserves binary bodies without any listener', async () => {
  const binary = Buffer.from([0,255,128,1]);
  const response = await dispatch((req,res) => {
    assert.equal(req.url, '/xrpc/fixture?did=example');
    assert.equal(req.headers.authorization, 'Bearer synthetic-only');
    const chunks = []; req.on('data', b => chunks.push(b)); req.on('end', () => {
      assert.deepEqual(Buffer.concat(chunks), binary);
      res.writeHead(200, { 'content-length': binary.length }); res.end(binary);
    });
  }, Buffer.concat([Buffer.from('POST /xrpc/fixture?did=example HTTP/1.1\r\nHost: pds.internal\r\nAuthorization: Bearer synthetic-only\r\nContent-Length: 4\r\n\r\n'), binary]));
  assert.deepEqual(response.subarray(response.indexOf('\r\n\r\n') + 4), binary);
});
test('bounded requests, responses and execution time fail closed', async () => {
  await assert.rejects(dispatch(() => {}, Buffer.alloc(MAX_FRAME+1)));
  await assert.rejects(dispatch((_req,res) => res.end(Buffer.alloc(MAX_FRAME+1)), Buffer.from('GET / HTTP/1.1\r\nHost: fixture\r\n\r\n')));
  await assert.rejects(dispatch(() => {}, Buffer.from('GET / HTTP/1.1\r\nHost: fixture\r\n\r\n'), 10));
});
test('framing starts explicitly and rejects oversized and partial input', async () => {
  for (const frame of [Buffer.from([255,255,255,255]), Buffer.from([0,0,0,2,1])]) {
    const input = new PassThrough(); const output = new PassThrough(); const chunks=[];
    output.on('data', b => chunks.push(b));
    const running = serveFrames((_req,res) => res.end(), input, output);
    input.end(frame);
    await assert.rejects(running);
    const ready = Buffer.concat(chunks);
    assert.equal(ready.readUInt32BE(), READY.length);
    assert.deepEqual(ready.subarray(4), READY);
  }
});
