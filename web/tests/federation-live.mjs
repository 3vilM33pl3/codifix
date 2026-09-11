// SPDX-License-Identifier: AGPL-3.0-or-later
// Invoked only by the disposable Rust/PDS E fixture. No mocked application APIs.
import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
let input = ''; for await (const chunk of process.stdin) input += chunk;
const fixture = JSON.parse(input);
const directory = await mkdtemp(join(tmpdir(), 'memory-e-clients-'));
const browser = await chromium.launch({ headless: true });
const errors = [];
async function rpc(side, suffix, body) {
  const response = await fetch(`${side.url}/v1/projects/${side.project}/federation/${side.space}${suffix}`, {
    method: body === undefined ? 'GET' : 'POST', headers: { Authorization: `Bearer ${side.token}`, 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  assert.equal(response.ok, true, `Fixture API returned ${response.status} for ${suffix}`);
  return response.json();
}
async function until(test) {
  for (let i = 0; i < 100; i++) { const value = await test(); if (value) return value; await new Promise(r => setTimeout(r, 100)); }
  throw new Error('Timed out waiting for accepted/replicated workflow state');
}
async function clientProcess(program, args, stdin) {
  const child = spawn(program, args, { stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, MEMORY_LAYER_PROFILE: 'prod' } });
  let stdout = '', stderr = ''; child.stdout.on('data', b => stdout += b); child.stderr.on('data', b => stderr += b);
  if (stdin) child.stdin.end(JSON.stringify(stdin)); else child.stdin.end();
  const timer = setTimeout(() => child.kill('SIGTERM'), 45000);
  const code = await new Promise((res, rej) => { child.once('error', rej); child.once('exit', res); });
  clearTimeout(timer); assert.equal(code, 0, `Client failed: ${stderr.slice(-2000)}`); return stdout;
}
async function cli(side, ...args) {
  return JSON.parse(await clientProcess(resolve('target/debug/memory'), ['--config', side.config, 'federation', '--project', side.project, ...args]));
}
async function pageFor(side) {
  const context = await browser.newContext({ viewport: { width: 1360, height: 960 } });
  await context.addCookies([
    { name: 'memory_session', value: side.session, url: side.url, httpOnly: true, sameSite: 'Lax' },
    { name: 'memory_csrf', value: side.csrf, url: side.url, sameSite: 'Lax' },
  ]);
  await context.addInitScript(project => localStorage.setItem('memory-layer.project', project), side.project);
  const page = await context.newPage(); page.on('pageerror', e => errors.push(String(e)));
  await page.goto(side.url);
  await page.getByLabel('More tabs').selectOption('federation');
  await page.getByRole('heading', { name: 'Knowledge federation' }).waitFor();
  await page.getByRole('button', { name: 'Refresh local status', exact: true }).click();
  return page;
}
async function confirm(page) {
  const section = page.getByLabel('Confirm federation action');
  await section.getByRole('checkbox').check();
  await section.getByRole('button', { name: 'Confirm and submit' }).click();
  await section.waitFor({ state: 'hidden' });
  await page.getByText('Queued locally — not yet accepted, published or replicated. Refresh status to observe progress.').waitFor();
}
try {
  // Skill-driven dev-server gut check: independent browser navigation, rendered
  // controls, screenshot and overlay check before the authenticated journey.
  if (process.env.MEMORY_FEDERATION_E_AGENT_BROWSER === '1') {
    const session = `memory-e-${randomUUID()}`;
    const ab = (...args) => clientProcess('npx', ['--yes', 'agent-browser', '--session', session, ...args]);
    try {
      await ab('open', fixture.left.url);
      const snapshot = await ab('snapshot', '-i');
      assert(snapshot.trim().length > 0);
      await ab('screenshot', join(directory, 'initial-ui.png'));
      assert((await ab('eval', 'document.body.innerText.trim().length > 0 && !document.querySelector(".vite-error-overlay, [data-nextjs-dialog]")')).includes('true'));
      console.log('Browser gut check: page loads, controls render, no blank page or framework error overlay.');
    } finally { await ab('close'); }
  }
  for (const [name, side] of Object.entries({ left: fixture.left, right: fixture.right })) {
    side.config = join(directory, `${name}.toml`);
    await writeFile(side.config, `[service]\nbind_addr = ${JSON.stringify(new URL(side.url).host)}\napi_token = ${JSON.stringify(side.token)}\n[database]\nurl = "postgres://unused:unused@127.0.0.1/unused"\n`, { mode: 0o600 });
    assert.equal((await cli(side, 'discover')).space, side.space);
  }
  const left = await pageFor(fixture.left), right = await pageFor(fixture.right);
  await left.getByLabel('Canonical memory UUID', { exact: true }).fill(fixture.canonical);
  await left.getByLabel('Public evidence label').fill('Approved incident excerpt');
  await left.getByLabel('Selected evidence text (optional)').fill('Reviewed evidence: <script>do not execute</script>.');
  await left.getByRole('button', { name: 'Prepare review', exact: true }).click();
  await left.getByText('Reviewed incident diagnosis', { exact: true }).waitFor();
  await left.getByRole('button', { name: 'Review publication consent' }).click();
  await confirm(left);
  await until(async () => (await rpc(fixture.right, '/catalog')).streams.items.some(s => s.shared_version === 1));
  await right.getByRole('button', { name: 'Refresh local status', exact: true }).click();
  await right.getByRole('button', { name: `Read and cite ${fixture.canonical}`, exact: true }).click();
  await right.getByLabel('Proposed corrected text').fill('Reviewed two-organization correction');
  await right.getByRole('button', { name: 'Review correction proposal' }).click();
  await confirm(right);
  const proposed = await until(async () => (await rpc(fixture.left, '/catalog')).proposals.items.find(p => p.resolution === 'pending'));
  await left.getByRole('button', { name: 'Refresh local status', exact: true }).click();
  await left.getByRole('button', { name: `Inspect proposal ${proposed.key.id}`, exact: true }).click();
  await left.getByRole('button', { name: 'Review approval' }).click();
  await confirm(left);
  await until(async () => (await rpc(fixture.right, '/catalog')).streams.items.some(s => s.shared_version === 2));
  const context = await cli(fixture.right, 'context', '--origin', fixture.left.instance, '--canonical', fixture.canonical);
  assert.equal(context.citations[0].shared_version, 2);
  assert.equal(context.external_forwarding_requires_fresh_policy, true);
  assert(!JSON.stringify(context).includes('NEVER_SHARE'));
  await right.getByRole('button', { name: 'Refresh local status', exact: true }).click();
  await right.getByRole('button', { name: `Read and cite ${fixture.canonical}`, exact: true }).click();
  await right.getByText('Reviewed two-organization correction', { exact: true }).waitFor();
  assert.equal(await right.locator('script').filter({ hasText: 'do not execute' }).count(), 0);
  await right.screenshot({ path: join(directory, 'federation-desktop.png'), fullPage: true });
  await right.setViewportSize({ width: 390, height: 844 });
  assert(await right.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2));
  await right.screenshot({ path: join(directory, 'federation-mobile.png'), fullPage: true });
  // Real PTY interaction submits another proposal; CLI then inspects/rejects it.
  const terminal = await clientProcess('python3', ['web/tests/federation-terminal.py'], {
    binary: resolve('target/debug/memory'), config: fixture.right.config, project: fixture.right.project,
    origin: fixture.left.instance, canonical: fixture.canonical,
  });
  assert(terminal.includes('PTY federation context and confirmed correction passed'));
  const second = await until(async () => (await rpc(fixture.left, '/catalog')).proposals.items.find(p => p.resolution === 'pending'));
  assert.equal((await cli(fixture.left, 'proposal', '--publisher', second.key.publisher, '--record', second.key.id)).resolution, 'pending');
  const requestId = randomUUID();
  const args = ['resolve', '--publisher', second.key.publisher, '--record', second.key.id, '--request-id', requestId, '--confirm'];
  const rejection = await cli(fixture.left, ...args);
  assert.deepEqual(await cli(fixture.left, ...args), rejection);
  await until(async () => (await rpc(fixture.left, '/catalog')).proposals.items.find(p => p.key.id === second.key.id && p.resolution === 'rejected'));
  assert.equal(errors.length, 0, `Browser errors: ${errors.join(', ')}`);
  console.log('E clients: real browser review/publish/cite/propose/approve, CLI discovery/context/idempotent rejection, PTY proposal, desktop/mobile and escaped imported evidence passed.');
  // Only synthetic screenshots are optionally retained; session/config secrets
  // stay in this private disposable directory and are always removed.
  if (process.env.MEMORY_FEDERATION_E_SCREENSHOTS) {
    const { copyFile, mkdir } = await import('node:fs/promises');
    await mkdir(process.env.MEMORY_FEDERATION_E_SCREENSHOTS, { recursive: true });
    for (const name of ['federation-desktop.png', 'federation-mobile.png']) await copyFile(join(directory, name), join(process.env.MEMORY_FEDERATION_E_SCREENSHOTS, name));
  }
} finally { await browser.close(); await rm(directory, { recursive: true, force: true }); }
