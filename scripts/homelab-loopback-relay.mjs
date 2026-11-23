// SPDX-License-Identifier: AGPL-3.0-or-later
// Operator-local preview only. The destination container, ports and executable
// are fixed; this is not an authenticated network gateway or a generic proxy.
import net from 'node:net';
import fs from 'node:fs';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export const TARGET = Object.freeze({
  container: 'memory-homelab-preview-app-20260919',
  webHost: '127.0.0.2', webPort: 4261, databasePort: 55461,
});
export function validateDatabase(value) {
  if (typeof value !== 'string' || net.isIP(value) !== 4
      || !/^172\.(1[6-9]|2\d|3[01])\./.test(value)) throw Error('Invalid private database target');
  return value;
}
function main() {
  const config = JSON.parse(fs.readFileSync(new URL('./relay.json', import.meta.url), 'utf8'));
  if (Object.keys(config).length !== 1) throw Error('Unexpected relay configuration');
  const database = validateDatabase(config.database);
  let databaseActive = 0;
  net.createServer(socket => {
    if (databaseActive >= 32) { socket.destroy(); return; }
    databaseActive++;
    const remote = net.connect({ host: database, port: 5432 });
    socket.setTimeout(120_000, () => socket.destroy());
    remote.setTimeout(120_000, () => remote.destroy());
    socket.on('error', () => remote.destroy()); remote.on('error', () => socket.destroy());
    socket.on('close', () => { databaseActive--; remote.destroy(); });
    remote.on('close', () => socket.destroy());
    socket.pipe(remote); remote.pipe(socket);
  }).listen(TARGET.databasePort, '127.0.0.1');
  let active = 0;
  net.createServer(socket => {
    if (active >= 32) { socket.destroy(); return; }
    active++;
    // An independent loopback host also avoids sharing browser session cookies
    // with Workshop. Never expose this owner-bootstrap bridge on the LAN.
    const child = spawn('/usr/bin/docker', ['--host', 'unix:///var/run/docker.sock',
      'exec', '--interactive', '--user', '1000:1000', TARGET.container, '/loopback-stdio'],
    { env: { PATH: '/usr/bin:/bin' }, stdio: ['pipe', 'pipe', 'ignore'] });
    socket.setTimeout(300_000, () => socket.destroy());
    socket.pipe(child.stdin); child.stdout.pipe(socket);
    child.stdin.on('error', () => socket.destroy()); child.stdout.on('error', () => socket.destroy());
    child.on('error', () => socket.destroy()); child.on('close', () => socket.destroy());
    socket.on('error', () => child.kill());
    socket.on('close', () => { active--; child.stdin.end(); child.kill(); });
  }).listen(TARGET.webPort, TARGET.webHost);
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
