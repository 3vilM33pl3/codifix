// SPDX-License-Identifier: AGPL-3.0-or-later
// Only launched by the ignored test owning a fresh synthetic database/service.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join, isAbsolute } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { chromium } from '@playwright/test';

const url = new URL(process.env.MEMORY_SUPERVISOR_REHEARSAL_URL ?? '');
assert.equal(url.protocol,'http:'); assert.equal(url.hostname,'127.0.0.1');
assert.ok(url.port && !['4040','4250'].includes(url.port));
const instance = process.env.MEMORY_SUPERVISOR_REHEARSAL_INSTANCE;
const token = process.env.MEMORY_SUPERVISOR_REHEARSAL_TOKEN;
assert.match(instance ?? '',/^[0-9a-f-]{36}$/);
assert.match(token ?? '',/^[0-9a-f-]{36}$/);
const binary = process.env.MEMORY_SUPERVISOR_REHEARSAL_CLI;
assert.ok(binary && isAbsolute(binary));
const privateDir = await mkdtemp(join(tmpdir(),'memory-supervisor-controls-'));
let browser;
try {
  const config = join(privateDir,'synthetic.toml');
  await writeFile(config,`[service]\nbind_addr = "${url.host}"\napi_token = "${token}"\n[database]\nurl = "postgresql://unused/unused"\n`,{flag:'wx',mode:0o600});
  const env = {PATH:process.env.PATH, LANG:'C.UTF-8', MEMORY_LAYER_PROFILE:'prod'};
  const cli = args => execFileSync(binary,['--config',config,'supervisor',...args],{env,timeout:15000,encoding:'utf8',cwd:privateDir});
  const inspect = () => JSON.parse(cli(['--json']));
  let state = inspect();
  assert.equal(state.snapshot.instance_id,instance); assert.equal(state.can_control,true);
  assert.equal(state.snapshot.policy.enabled,false);
  for (const [action, enabled] of [['resume',true],['pause',false]]) {
    const args = [action,'--instance',instance,'--policy',state.snapshot.policy.revision,'--request-id',randomUUID()];
    assert.match(cli(args),/Preview only; no mutation sent/);
    assert.equal(inspect().snapshot.policy.enabled,!enabled);
    assert.match(cli([...args,'--confirm']),/Receipt confirms this request/);
    state = inspect(); assert.equal(state.snapshot.policy.enabled,enabled);
    // An exact repeated confirmation must not create another change.
    assert.match(cli([...args,'--confirm']),/"replayed": true/);
  }
  const pty = JSON.parse(execFileSync('python3',[fileURLToPath(new URL('../../scripts/tests/supervisor-pty.py',import.meta.url)),binary,config,instance,'controls'],{env,timeout:25000,encoding:'utf8',cwd:privateDir}));
  assert.equal(pty.tui_pty,'passed'); assert.equal(pty.mutations,true);
  assert.equal(inspect().snapshot.policy.enabled,false);
  browser = await chromium.launch({headless:true});
  const context = await browser.newContext({viewport:{width:1280,height:1000},serviceWorkers:'block'});
  await context.addCookies([
    {name:'memory_session',value:'synthetic-supervisor-session',url:url.href,httpOnly:true,sameSite:'Lax'},
    {name:'memory_csrf',value:'synthetic-csrf',url:url.href,sameSite:'Lax'},
  ]);
  await context.route('**/*',route => new URL(route.request().url()).origin === url.origin ? route.continue() : route.abort());
  const page = await context.newPage();
  const errors = []; page.on('pageerror',error=>errors.push(error.message));
  await page.goto(url.href,{waitUntil:'domcontentloaded',timeout:15000});
  await page.getByLabel('More tabs').selectOption('supervisor');
  const panel = page.getByRole('region',{name:'Area supervisor'});
  for (const [action, enabled] of [['Resume supervisor',true],['Pause supervisor',false]]) {
    await panel.getByRole('button',{name:action,exact:true}).click();
    const dialog = panel.getByRole('dialog',{name:'Confirm supervisor policy change'});
    assert.match(await dialog.innerText(),new RegExp(instance));
    assert.equal(inspect().snapshot.policy.enabled,!enabled);
    const confirmed = page.waitForResponse(r=>new URL(r.url()).pathname==='/v1/supervisor/policy');
    await dialog.getByRole('button',{name:'Confirm policy change'}).click();
    assert.equal((await confirmed).status(),200);
    await panel.getByRole('button',{name:enabled?'Pause supervisor':'Resume supervisor',exact:true}).waitFor();
    assert.equal(inspect().snapshot.policy.enabled,enabled);
  }
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({browser:'passed',cli:'passed',tui_pty:'passed',confirmed_mutations:6,final_policy:'paused',provider_calls:0,real_data:false}));
} finally {
  if(browser) await browser.close();
  await rm(privateDir,{recursive:true,force:true});
}
