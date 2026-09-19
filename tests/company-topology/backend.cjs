// SPDX-License-Identifier: AGPL-3.0-or-later
// Synthetic upstream only: no Memory, PostgreSQL, PDS or authentication behavior.
const http = require('node:http');
const [port, upstream] = process.argv.slice(2);
if (!/^\d{4}$/.test(port) || !/^[a-z_]+$/.test(upstream)) process.exit(2);
http.createServer((request, response) => {
  request.resume();
  response.writeHead(200, { 'content-type': 'application/json' });
  response.end(JSON.stringify({ upstream, method: request.method, path: request.url }));
}).listen(Number(port), '0.0.0.0');
