// SPDX-License-Identifier: AGPL-3.0-or-later
// Synthetic independent-instance fixture only; real HTTP and client processes.
import { chromium, expect } from '@playwright/test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, isAbsolute } from 'node:path';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
let input = ''; for await (const chunk of process.stdin) input += chunk;
const f = JSON.parse(input);
assert.equal(f.synthetic, true);
for (const side of [f.left, f.right]) assert.equal(new URL(side.url).hostname, '127.0.0.1');
const directory = await mkdtemp(join(tmpdir(), 'memory-f1-clients-'));
const browser = await chromium.launch({ headless: true });
// Explicit disposable configs must not inherit operator routing or overrides.
const environment = { ...process.env, HOME: directory,
  XDG_CONFIG_HOME: join(directory, 'config'), XDG_STATE_HOME: join(directory, 'state'), XDG_CACHE_HOME: join(directory, 'cache'),
  USERPROFILE: directory, LOCALAPPDATA: join(directory, 'local'), APPDATA: join(directory, 'roaming') };
for (const key of Object.keys(environment)) if (key.startsWith('CODIFIX_')) delete environment[key];
environment.CODIFIX_PROFILE = 'prod';
const binary = f.binary;
assert.equal(typeof binary, 'string');
assert(isAbsolute(binary));
let mcp;
async function processResult(program, args, data) {
  const child = spawn(program, args, { env: environment, stdio: ['pipe', 'pipe', 'pipe'] });
  let output = '', error = '';
  child.stdout.on('data', b => output += b); child.stderr.on('data', b => error += b);
  child.stdin.end(data ? JSON.stringify(data) : undefined);
  const timer = setTimeout(() => child.kill(), 60000);
  const code = await new Promise((res, rej) => { child.once('error', rej); child.once('close', res); });
  clearTimeout(timer); assert.equal(code, 0, `Synthetic client failed: ${error.slice(-2000)}`);
  return output;
}
const cli = async (side, ...args) => JSON.parse(await processResult(binary, ['--config', side.config, 'federation', '--project', side.project, ...args]));
async function until(read, accept) {
  for (let i = 0; i < 40; i++) { const value = await read(); if (accept(value)) return value; await new Promise(r => setTimeout(r, 200)); }
  throw new Error('F1 answer deadline');
}
function checkAnswer(result, responder) {
  assert.equal(result.status, 'answered');
  assert(result.answer.untrusted);
  assert.equal(result.answer.citations[0].responder, responder);
  const text = JSON.stringify(result);
  assert(text.includes('Reviewed synthetic evidence only'));
  for (const sentinel of ['NEVER_SHARE', 'UNSELECTED_SECRET_SENTINEL', 'DO_NOT_SHARE']) assert(!text.includes(sentinel));
}
try {
  for (const [name, side] of Object.entries({ left: f.left, right: f.right })) {
    side.config = join(directory, `${name}.toml`);
    await writeFile(side.config, `[service]\nbind_addr=${JSON.stringify(new URL(side.url).host)}\napi_token=${JSON.stringify(side.token)}\n[database]\nurl="postgres://unused:unused@127.0.0.1/unused"\n[mcp]\nread_only=false\n`, { mode: 0o600 });
  }
  // Two real browsers: explicit review, submission, signed fresh-origin answer.
  for (const [side, remote] of [[f.left, f.right], [f.right, f.left]]) {
    const context = await browser.newContext({ viewport: { width: 1360, height: 960 } });
    await context.addCookies([{ name: 'codifix_session', value: side.session, url: side.url, httpOnly: true, sameSite: 'Lax' }, { name: 'codifix_csrf', value: side.csrf, url: side.url, sameSite: 'Lax' }]);
    await context.addInitScript(project => localStorage.setItem('codifix.project', project), side.project);
    const page = await context.newPage();
    const errors = []; page.on('pageerror', e => errors.push(String(e)));
    await page.goto(side.url);
    assert.equal(await page.locator('vite-error-overlay, [data-nextjs-dialog]').count(), 0);
    await page.getByLabel('More tabs').selectOption('federation');
    const panel = page.getByLabel('Private expertise exchange');
    await panel.getByRole('button', { name: 'Discover expertise' }).click();
    await panel.getByRole('button', { name: 'Select expertise Reviewed recovery' }).click();
    await panel.getByLabel('Remote question').fill('What is the reviewed incident diagnosis?');
    await panel.getByRole('button', { name: 'Prepare remote question' }).click();
    const send = panel.getByRole('button', { name: 'Send approved question' });
    await send.waitFor(); assert(await send.isDisabled());
    assert((await panel.innerText()).includes(remote.instance));
    await panel.getByRole('checkbox').check(); await send.click();
    await until(async () => {
      await panel.getByRole('button', { name: 'Fetch expertise result' }).click();
      await expect(panel.getByRole('button', { name: 'Fetch expertise result' })).toBeEnabled();
      return panel.innerText();
    }, text => text.includes('Question status: answered'));
    const attribution = await panel.getByLabel('Answer citations and attribution').innerText();
    assert(attribution.includes(remote.instance)); assert(attribution.includes('Reviewed synthetic evidence only'));
    assert.equal(errors.length, 0);
    if (process.env.CODIFIX_FEDERATION_F1_SCREENSHOTS) {
      await mkdir(process.env.CODIFIX_FEDERATION_F1_SCREENSHOTS, { recursive: true });
      await page.screenshot({ path: join(process.env.CODIFIX_FEDERATION_F1_SCREENSHOTS, `f1-answer-${side.instance}.png`), fullPage: true });
    }
    await page.setViewportSize({ width: 390, height: 844 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2));
    await context.close();
  }
  const catalog = await cli(f.left, 'capabilities');
  assert.equal(catalog.capabilities[0].owner, f.right.instance);
  const question = join(directory, 'question.txt'), reviewFile = join(directory, 'review.json');
  await writeFile(question, 'What is the reviewed incident diagnosis?', { mode: 0o600 });
  const review = await cli(f.left, 'ask', 'prepare', '--publisher', f.right.instance, '--collection', f.right.collection, '--request-id', randomUUID(), '--question-file', question);
  assert.equal(review.question.output_destination, 'local_human');
  await writeFile(reviewFile, JSON.stringify(review), { mode: 0o600 });
  for (let i = 0; i < 2; i++) await cli(f.left, 'ask', 'submit', '--file', reviewFile, '--confirm');
  checkAnswer(await until(() => cli(f.left, 'answer', '--request-id', review.question.request_id), r => r.status === 'answered'), f.right.instance);
  assert((await processResult('python3', ['web/tests/f1-terminal.py'], { binary, config: f.left.config, project: f.left.project, publisher: f.right.instance, collection: f.right.collection })).includes('F1 PTY confirmed question and cited answer passed'));

  // MCP uses a DIFFERENT operator-bound cloud-output token. It cannot promote
  // the human token or self-authorize a destination in tool arguments.
  const mcpConfig = join(directory, 'mcp.toml');
  await writeFile(mcpConfig, `[service]\nbind_addr=${JSON.stringify(new URL(f.left.url).host)}\napi_token=${JSON.stringify(f.mcp_token)}\n[database]\nurl="postgres://unused:unused@127.0.0.1/unused"\n[mcp]\nread_only=false\n`, { mode: 0o600 });
  mcp = spawn(binary, ['--config', mcpConfig, 'mcp', 'run', '--project', f.left.project], { env: environment, stdio: ['pipe', 'pipe', 'pipe'] });
  mcp.stderr.resume();
  let serial = 0, buffered = ''; const pending = new Map();
  mcp.stdout.on('data', bytes => {
    buffered += bytes;
    while (buffered.includes('\n')) {
      const split = buffered.indexOf('\n'), line = buffered.slice(0, split); buffered = buffered.slice(split + 1);
      const message = JSON.parse(line), waiter = pending.get(message.id);
      if (waiter) { pending.delete(message.id); clearTimeout(waiter.timer); waiter.resolve(message); }
    }
  });
  function rpc(method, params) {
    const id = ++serial;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { pending.delete(id); reject(new Error('MCP fixture deadline')); }, 30000);
      pending.set(id, { resolve, timer }); mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    });
  }
  assert(!(await rpc('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'synthetic-f1', version: '1' } })).error);
  mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  const tools = (await rpc('tools/list', {})).result.tools;
  assert.equal(tools.find(t => t.name === 'codifix_expertise_ask').annotations.readOnlyHint, false);
  async function call(name, args) {
    const response = await rpc('tools/call', { name, arguments: { project: f.left.project, space: f.left.space, ...args } });
    assert(!response.error, 'MCP expertise tool denied'); assert(!response.result.isError);
    return response.result.structuredContent.response;
  }
  assert.equal((await call('codifix_expertise_discover', {})).capabilities[0].owner, f.right.instance);
  const mcpReview = await call('codifix_expertise_prepare', { publisher: f.right.instance, collection: f.right.collection, request_id: randomUUID(), question: 'What is the reviewed incident diagnosis?' });
  assert.equal(mcpReview.question.output_destination, 'synthetic_cloud');
  await call('codifix_expertise_ask', { review_json: JSON.stringify(mcpReview), confirmed: true });
  checkAnswer(await until(() => call('codifix_expertise_result', { request_id: mcpReview.question.request_id }), r => r.status === 'answered'), f.right.instance);
  console.log('F1 real clients: bidirectional browser cited answers, CLI exact retry, PTY confirmation/result and bound-cloud MCP discovery/prepare/ask/result passed.');
} finally {
  mcp?.kill(); await browser.close(); await rm(directory, { recursive: true, force: true });
}
