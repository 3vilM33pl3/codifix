// SPDX-License-Identifier: AGPL-3.0-or-later
// Private stdio bridge. It never binds a TCP port or creates a Node-owned pipe.
import http from 'node:http';
import { Duplex } from 'node:stream';

export const MAX_FRAME = 16 * 1024 * 1024 + 64 * 1024;
export const READY = Buffer.from('codifix-pds-stdio-v1');

export function dispatch(app, request, timeoutMs = 20_000) {
  if (!Buffer.isBuffer(request) || request.length > MAX_FRAME) {
    return Promise.reject(new Error('invalid bounded request'));
  }
  return new Promise((resolve, reject) => {
    const chunks = [];
    let length = 0;
    let settled = false;
    const server = http.createServer(app);
    server.keepAliveTimeout = 90_000;
    const finish = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      if (error) reject(new Error('private HTTP exchange failed'));
      else resolve(Buffer.concat(chunks, length));
    };
    const socket = new Duplex({
      read() {},
      write(chunk, encoding, callback) {
        length += chunk.length;
        if (length > MAX_FRAME) { callback(new Error('response exceeds limit')); return; }
        chunks.push(Buffer.from(chunk)); callback();
      },
    });
    // Node's HTTP parser receives an ordinary duplex connection, but no OS
    // network socket exists. These values are not remote authentication.
    socket.remoteAddress = '127.0.0.1';
    socket.remotePort = 0;
    socket.setTimeout = () => socket;
    socket.setNoDelay = () => socket;
    socket.setKeepAlive = () => socket;
    socket.on('error', finish);
    server.on('clientError', finish);
    server.on('request', (_request, response) => {
      response.shouldKeepAlive = false;
      response.on('error', finish);
      response.once('finish', () => setImmediate(() => finish()));
    });
    const timer = setTimeout(() => finish(new Error('deadline')), timeoutMs);
    server.emit('connection', socket);
    socket.push(request);
  });
}

export async function serveFrames(app, input, output) {
  const write = (payload) => new Promise((resolve, reject) => {
    const prefix = Buffer.alloc(4); prefix.writeUInt32BE(payload.length);
    output.write(Buffer.concat([prefix, payload]), error => error ? reject(error) : resolve());
  });
  await write(READY);
  let pending = Buffer.alloc(0);
  for await (const chunk of input) {
    if (pending.length + chunk.length > MAX_FRAME + 4) throw new Error('frame exceeds limit');
    pending = Buffer.concat([pending, chunk]);
    while (pending.length >= 4) {
      const size = pending.readUInt32BE();
      if (!size || size > MAX_FRAME) throw new Error('invalid frame');
      if (pending.length < size + 4) break;
      const request = pending.subarray(4, size + 4);
      pending = pending.subarray(size + 4);
      await write(await dispatch(app, request));
    }
  }
  if (pending.length) throw new Error('interrupted frame');
}
