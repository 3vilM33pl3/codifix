// Fixed independent-preview forwarding only. No destination selection, CONNECT,
// credential handling or internet proxy. Copy this reviewed template beside the
// existing private relay.json; activation/restart is a separate operator action.
import net from 'node:net';
import fs from 'node:fs';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export const MAX_WEB_CONNECTIONS = 32;
export const WEB_IDLE_MS = 300_000;
export const MAX_DATABASE_CONNECTIONS = 64;

export function validateTargets(config) {
  if (!config || typeof config !== 'object' || Array.isArray(config)) throw Error('Invalid private target');
  for (const host of [config.database, config.application].filter(Boolean)) {
    if (net.isIP(host) !== 4 || !/^172\.(1[6-9]|2\d|3[01])\./.test(host)) throw Error('Invalid private target');
  }
  return config;
}

function main() {
  const config = validateTargets(JSON.parse(fs.readFileSync(new URL('./relay.json', import.meta.url), 'utf8')));
  // Database forwarding stays on the reviewed internal Docker network. Neither
  // listener accepts a caller-supplied target, port, hostname or URL.
  if (config.database) {
    let active = 0;
    net.createServer({ allowHalfOpen: false }, socket => {
      if (active >= MAX_DATABASE_CONNECTIONS) { socket.destroy(); return; }
      active++;
      const remote = net.connect({ host: config.database, port: 5432 });
      socket.setTimeout(120_000, () => socket.destroy());
      remote.setTimeout(120_000, () => remote.destroy());
      socket.on('error', () => remote.destroy());
      remote.on('error', () => socket.destroy());
      socket.on('close', () => { active--; remote.destroy(); });
      remote.on('close', () => socket.destroy());
      socket.pipe(remote); remote.pipe(socket);
    }).listen(55460, '127.0.0.1');
  }
  // A TUI startup can retain more than sixteen HTTP keep-alive connections
  // alongside the browser/WebSocket. A separate private question auth request
  // must not be dropped merely because those connections occupy idle slots.
  // Each stdio bridge uses two threads. Thirty-two bridges leave headroom for
  // the service below the container's unchanged 128-PID/thread ceiling.
  // Keep a finite 32-slot cap and the existing idle timeout: this application
  // has no proven periodic WebSocket heartbeat, so 30 seconds is not safe.
  let webActive = 0;
  net.createServer(socket => {
    if (webActive >= MAX_WEB_CONNECTIONS) { socket.destroy(); return; }
    webActive++;
    // Real loopback inside the app namespace preserves local-session bootstrap
    // semantics. No remotely reachable proxy can upgrade a peer to owner.
    const child = spawn('/usr/bin/docker', ['--host', 'unix:///var/run/docker.sock', 'exec', '--interactive', '--user', '1000:1000', 'memory-workshop-preview-app-20260918', '/loopback-stdio'], { env: { PATH: '/usr/bin:/bin' }, stdio: ['pipe', 'pipe', 'ignore'] });
    socket.setTimeout(WEB_IDLE_MS, () => socket.destroy());
    socket.pipe(child.stdin); child.stdout.pipe(socket);
    child.stdin.on('error', () => socket.destroy());
    child.stdout.on('error', () => socket.destroy());
    child.on('error', () => socket.destroy());
    child.on('close', () => socket.destroy());
    socket.on('error', () => child.kill());
    socket.on('close', () => { webActive--; child.stdin.end(); child.kill(); });
  }).listen(4260, '127.0.0.1');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
