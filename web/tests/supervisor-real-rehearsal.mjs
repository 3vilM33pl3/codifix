// SPDX-License-Identifier: AGPL-3.0-or-later
// Launched only by the ignored Rust test which owns a synthetic DB/service.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, writeFile, rm, lstat, open } from 'node:fs/promises';
import { join, isAbsolute } from 'node:path';
import { tmpdir } from 'node:os';
import { chromium } from '@playwright/test';

const url = new URL(process.env.MEMORY_SUPERVISOR_REHEARSAL_URL ?? '');
assert.equal(url.protocol,'http:'); assert.equal(url.hostname,'127.0.0.1');
assert.ok(url.port && !['4040','4250'].includes(url.port));
const instance = process.env.MEMORY_SUPERVISOR_REHEARSAL_INSTANCE;
assert.match(instance ?? '',/^[0-9a-f-]{36}$/);
const binary = process.env.MEMORY_SUPERVISOR_REHEARSAL_CLI;
assert.ok(binary && isAbsolute(binary));
const privateDir=await mkdtemp(join(tmpdir(),'memory-supervisor-rehearsal-'));
let browser;
try {
  const config=join(privateDir,'synthetic.toml');
  await writeFile(config,`[service]\nbind_addr = "${url.host}"\napi_token = "synthetic-supervisor-rehearsal"\n[database]\nurl = "postgresql://unused/unused"\n`,{flag:'wx',mode:0o600});
  // No inherited Memory overrides, provider keys or R3 allowances reach the CLI.
  const env={PATH:process.env.PATH,LANG:'C.UTF-8',MEMORY_LAYER_PROFILE:'prod'};
  const cli=JSON.parse(execFileSync(binary,['--config',config,'supervisor','--json'],{env,timeout:15000,encoding:'utf8',cwd:privateDir}));
  assert.equal(cli.snapshot.instance_id,instance);
  assert.equal(cli.execution,'not_integrated');
  assert.equal(cli.snapshot.policy.enabled,false);
  assert.equal(cli.snapshot.journal[0].event,'policy_installed');
  browser=await chromium.launch({headless:true});
  const context=await browser.newContext({viewport:{width:1280,height:900},serviceWorkers:'block'});
  // This read-only rehearsal has no need for any other network destination.
  await context.route('**/*',route => new URL(route.request().url()).origin === url.origin ? route.continue() : route.abort());
  const page=await context.newPage();
  const errors=[];
  page.on('pageerror',error => errors.push(error.message));
  await page.goto(url.href,{waitUntil:'domcontentloaded',timeout:15000});
  await page.getByLabel('More tabs').selectOption('supervisor');
  const panel=page.getByRole('region',{name:'Area supervisor'});
  await panel.getByRole('heading',{name:'Workshop',exact:true}).waitFor();
  assert.match(await panel.innerText(),/execution is not integrated/);
  assert.match(await panel.innerText(),/Policy: paused/);
  assert.match(await panel.innerText(),/Policy prepared/);
  assert.match(await panel.innerText(),new RegExp(instance));
  const responsePromise=page.waitForResponse(r=>new URL(r.url()).pathname==='/v1/supervisor');
  await panel.getByRole('button',{name:'Refresh supervisor'}).click();
  const response=await responsePromise;
  assert.equal(response.status(),200);
  assert.equal(response.headers()['cache-control'],'no-store');
  assert.equal((await response.json()).snapshot.instance_id,instance);
  await panel.getByRole('heading',{name:'Workshop',exact:true}).waitFor();
  assert.equal(await page.locator('.vite-error-overlay,[data-nextjs-dialog]').count(),0);
  assert.deepEqual(errors,[]);
  let screenshot=null;
  const artifacts=process.env.MEMORY_SUPERVISOR_REHEARSAL_ARTIFACTS;
  if (artifacts) {
    assert.ok(isAbsolute(artifacts));
    const meta=await lstat(artifacts);
    assert.ok(meta.isDirectory() && meta.uid===process.getuid() && (meta.mode&0o777)===0o700);
    screenshot=join(artifacts,'supervisor.png');
    const image=await open(screenshot,'wx',0o600);
    try { await image.writeFile(await page.screenshot({fullPage:true})); await image.sync(); }
    finally { await image.close(); }
  }
  await page.getByRole('button',{name:'Help',exact:true}).click();
  await page.getByText('Supervisor Help',{exact:true}).waitFor();
  console.log(JSON.stringify({browser:'passed',cli:'passed',api_database_binding:'matched',worker_activated:false,screenshot}));
} finally {
  await browser?.close();
  await rm(privateDir,{recursive:true,force:true});
}
