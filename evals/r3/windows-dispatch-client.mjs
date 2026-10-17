// SPDX-License-Identifier: AGPL-3.0-or-later
import http from 'node:http';
import { validateWindowsDispatchEndpoint, createWindowsDispatchRequest, authenticateWindowsDispatchResponse } from './windows-dispatch-auth.mjs';

export function requestWindowsDispatch({ endpoint: value, expected, input, timeoutMs = 590000 }) {
  const endpoint = validateWindowsDispatchEndpoint(value, expected);
  const request = createWindowsDispatchRequest(endpoint, input);
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 600000) throw Error('bounded native dispatch deadline required');
  return new Promise((resolve, reject) => {
    const fail = () => reject(Error('native dispatch failed or uncertain; no automatic repeat'));
    const call = http.request({ host: '127.0.0.1', port: endpoint.port, method: 'POST', path: '/dispatch',
      agent: false, maxHeaderSize: 8192, timeout: timeoutMs,
      headers: { 'Content-Type': 'application/json', 'Content-Length': input.length, ...request.headers } }, response => {
      const chunks = []; let length = 0;
      response.on('data', bytes => {
        length += bytes.length;
        if (length > 16 * 1024 * 1024) call.destroy(Error('oversized response'));
        else chunks.push(bytes);
      });
      response.on('error', fail);
      response.once('end', () => {
        try {
          const bytes = Buffer.concat(chunks);
          authenticateWindowsDispatchResponse(endpoint, request.proof, response.statusCode, response.headers, bytes);
          if (response.statusCode !== 200) throw Error('refused');
          JSON.parse(bytes); // Never parse/accept evidence before mutual authentication.
          resolve(bytes);
        } catch { fail(); }
      });
    });
    call.on('error', fail); call.once('timeout', () => call.destroy(Error('deadline')));
    call.end(input);
  });
}
