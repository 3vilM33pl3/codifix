// SPDX-License-Identifier: AGPL-3.0-or-later
// Only called by the opt-in Rust disposable fixture; no intercepted/mocked APIs.
import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
let input = ''; for await (const chunk of process.stdin) input += chunk;
const f = JSON.parse(input);
assert.equal(f.synthetic, true);
assert.equal(new URL(f.url).hostname, '127.0.0.1');
const browser = await chromium.launch({ headless: true });
try {
  const context = await browser.newContext({ viewport: { width: 1360, height: 960 } });
  await context.addCookies([
    { name: 'codifix_session', value: f.session, url: f.url, httpOnly: true, sameSite: 'Lax' },
    { name: 'codifix_csrf', value: f.csrf, url: f.url, sameSite: 'Lax' },
  ]);
  await context.addInitScript(project => localStorage.setItem('codifix.project', project), f.project);
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  await page.goto(f.url);
  assert(await page.locator('body').innerText());
  assert.equal(await page.locator('vite-error-overlay, .vite-error-overlay, [data-nextjs-dialog]').count(), 0);
  await page.getByLabel('More tabs').selectOption('federation');
  await page.getByRole('heading', { name: 'Knowledge federation' }).waitFor();
  const collections = page.getByLabel('Expertise collection management');
  const id = randomUUID();
  await collections.getByLabel('Collection UUID (blank creates a new collection)').fill(id);
  await collections.getByLabel('Collection name', { exact: true }).fill('Browser recovery');
  await collections.getByLabel('Reviewed collection description').fill('Synthetic reviewed incident knowledge');
  await collections.getByLabel('Collection topics (comma separated)').fill('recovery');
  await collections.getByLabel('Allowed requesting instance UUIDs').fill(f.instance);
  await collections.getByLabel('Selected canonical memory UUIDs').fill(f.canonical);
  await collections.getByRole('button', { name: 'Prepare collection review' }).click();
  const confirm = collections.getByRole('button', { name: 'Confirm collection action' });
  await confirm.waitFor();
  assert(await confirm.isDisabled());
  const bytes = await collections.getByLabel('Exact collection action bytes').innerText();
  assert(bytes.includes('Reviewed incident diagnosis'));
  assert(!bytes.includes('NEVER_SHARE'));
  await collections.getByRole('checkbox').check();
  await confirm.click();
  await collections.getByText(/Collection action queued locally/).waitFor();
  await collections.getByRole('button', { name: 'Browse collections' }).click();
  await collections.getByRole('button', { name: 'Inspect collection Browser recovery' }).click();
  await collections.getByLabel('Answering service principal UUID').fill(f.service);
  await collections.getByRole('button', { name: 'Review service grant', exact: true }).click();
  assert(await confirm.isDisabled());
  await collections.getByRole('checkbox').check();
  await confirm.click();
  await collections.getByText(/Service collection grant updated locally/).waitFor();
  await collections.getByRole('button', { name: 'Browse collections' }).click();
  await collections.getByRole('button', { name: 'Inspect collection Browser recovery' }).click();
  await collections.getByRole('button', { name: 'Inspect service grant' }).click();
  await collections.getByText(/Stored grant for .*: enabled/).waitFor();
  await collections.getByRole('button', { name: 'Review service grant revocation' }).click();
  await collections.getByRole('checkbox').check(); await confirm.click();
  await collections.getByText(/Service collection grant updated locally/).waitFor();
  await collections.getByRole('button', { name: 'Browse collections' }).click();
  await collections.getByRole('button', { name: 'Inspect collection Browser recovery' }).click();
  await collections.getByRole('button', { name: 'Inspect service grant' }).click();
  await collections.getByText(/Stored grant for .*: disabled/).waitFor();
  await collections.getByRole('button', { name: 'Review collection withdrawal' }).click();
  await collections.getByRole('checkbox').check(); await confirm.click();
  await collections.getByText(/Collection action queued locally/).waitFor();
  await collections.getByRole('button', { name: 'Browse collections' }).click();
  await collections.getByText(/Browser recovery: withdrawn/).waitFor();

  const expertise = page.getByLabel('Private expertise exchange');
  await expertise.getByRole('button', { name: 'Discover expertise' }).click();
  await expertise.getByRole('button', { name: 'Select expertise Fixture recovery' }).click();
  await expertise.getByLabel('Remote question').fill('How is incident recovery reviewed?');
  await expertise.getByRole('button', { name: 'Prepare remote question' }).click();
  const send = expertise.getByRole('button', { name: 'Send approved question' });
  await send.waitFor(); assert(await send.isDisabled());
  await expertise.getByRole('checkbox').check(); await send.click();
  await expertise.getByText(/Question status: queued/).waitFor();
  await expertise.getByRole('button', { name: 'Fetch expertise result' }).click();
  await expertise.getByText(/Question status: queued/).waitFor();
  assert.equal(await expertise.getByRole('heading', { name: /Untrusted answer/ }).count(), 0);
  assert.equal(errors.length, 0, errors.join('\n'));
  const screenshotRoot = process.env.MEMORY_FEDERATION_F1_SCREENSHOTS;
  if (screenshotRoot) { await mkdir(screenshotRoot, { recursive: true }); await page.screenshot({ path: join(screenshotRoot, 'f1-desktop.png'), fullPage: true }); }
  await page.setViewportSize({ width: 390, height: 844 });
  const mobileLayout = await page.evaluate(() => ({
    viewport: window.innerWidth,
    scrollX: window.scrollX,
    root: { client: document.documentElement.clientWidth, rect: document.documentElement.getBoundingClientRect().width },
    body: { client: document.body.clientWidth, scroll: document.body.scrollWidth, rect: document.body.getBoundingClientRect().width },
    width: document.documentElement.scrollWidth,
    overflowing: [...document.querySelectorAll('body *')]
      .filter(element => element.scrollWidth > element.clientWidth + 2 || element.getBoundingClientRect().right + window.scrollX > window.innerWidth + 2)
      .map(element => ({ tag: element.tagName, className: element.className, width: element.getBoundingClientRect().width, client: element.clientWidth, scroll: element.scrollWidth }))
      .slice(0, 20),
  }));
  assert(mobileLayout.width <= mobileLayout.viewport + 2, JSON.stringify(mobileLayout));
  console.log(`F1 mobile width: ${mobileLayout.width}px within ${mobileLayout.viewport}px viewport.`);
  if (screenshotRoot) await page.screenshot({ path: join(screenshotRoot, 'f1-mobile.png'), fullPage: true });
  console.log('F1 live browser: collection review/activation/withdrawal, service grant/inspection/revocation, signed discovery and confirmed queued question/status passed; desktop/mobile checks passed. No completed-answer/two-origin/PDS claim.');
} finally { await browser.close(); }
