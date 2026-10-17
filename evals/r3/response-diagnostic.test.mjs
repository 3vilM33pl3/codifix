// SPDX-License-Identifier: AGPL-3.0-or-later
// Real pinned CLI/budget gates; fake secret executable and mocked provider only.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { prepareResponseDiagnostic } from './prepare-response-diagnostic.mjs';
import { runResponseDiagnostic } from './response-diagnostic.mjs';
import { runPaidCampaign } from './launch-paid.mjs';

const names=['CODIFIX_R3_CLI_BIN','CODIFIX_R3_BUDGET_BIN','CODIFIX_R3_SNAPSHOT_BIN',
  'CODIFIX_R3_SCORE_BIN','CODIFIX_R3_RETRIEVER_BIN','CODIFIX_R3_TEST_IMAGE','CODIFIX_R3_RUST_IMAGE'];
const available=names.every(n=>process.env[n]);
if(process.env.CODIFIX_R3_REQUIRE_DIAGNOSTIC==='1'&&!available)throw Error('diagnostic binaries/images required');
const payloadHash='503b22223ccc9d67669be07b86980fc5f6f131603aae7b2418a37353901e5a79';
async function fixture(t){
  const directory=await fs.mkdtemp(path.join(os.tmpdir(),'r3-onecall-test-'));
  t.after(()=>fs.rm(directory,{recursive:true}));
  const reads=path.join(directory,'reads'),bao=path.join(directory,'fake-bao');
  await fs.writeFile(bao,`#!${process.execPath}\nrequire('node:fs').appendFileSync(${JSON.stringify(reads)},'read\\n');process.stdout.write('synthetic-diagnostic-key');`,{mode:0o700});
  const inputs=path.join(directory,'inputs.json');
  await fs.writeFile(inputs,JSON.stringify({binaries:{memory:process.env.CODIFIX_R3_CLI_BIN,
    budget:process.env.CODIFIX_R3_BUDGET_BIN,snapshot:process.env.CODIFIX_R3_SNAPSHOT_BIN,
    score:process.env.CODIFIX_R3_SCORE_BIN,retriever:process.env.CODIFIX_R3_RETRIEVER_BIN,node:process.execPath,bao},
    images:{node:process.env.CODIFIX_R3_TEST_IMAGE,rust:process.env.CODIFIX_R3_RUST_IMAGE}}));
  const source=path.join(directory,'source'),root=path.join(directory,'diagnostic');
  const built=spawnSync(process.execPath,['evals/r3/prepare-paid-calibration.mjs',source,inputs],{encoding:'utf8'});
  assert.equal(built.status,0,built.stderr);
  await fs.mkdir(path.join(source,'run/work-00000/provider'),{recursive:true});
  await fs.writeFile(path.join(source,'run/work-00000/request.json'),JSON.stringify({work:{case_id:'configuration-rollout',condition:'vector'}}));
  // Golden synthetic request: step two after an immediate finish on step one.
  await fs.writeFile(path.join(source,'run/work-00000/provider/budget.jsonl'),JSON.stringify({event:'reserved',
    id:'call-b5232b682b032d1b4922ca05a07d6dd44f7d4d3bfc90dba30edb92845571d81c',payload_sha256:payloadHash})+'\n');
  const prepared=await prepareResponseDiagnostic(source,root);
  assert.equal(prepared.reconstructed_payload_sha256,payloadHash);
  assert.equal(prepared.provider_called,false);assert.equal(prepared.approvals_created,false);
  const file=path.join(root,'campaign.json'),manifest=JSON.parse(await fs.readFile(file));
  manifest.label_status='reviewed'; // Synthetic fixture only, never real authority.
  await fs.writeFile(file,JSON.stringify(manifest));
  const planned=spawnSync(process.env.CODIFIX_R3_CLI_BIN,['eval','campaign','plan','--manifest',file],{encoding:'utf8'});
  assert.equal(planned.status,0,planned.stderr);
  const identity=JSON.parse(planned.stdout).manifest_sha256,approvals=[];
  for(const purpose of ['labels','development']){
    const p=path.join(directory,purpose+'.json');
    await fs.writeFile(p,JSON.stringify({purpose,manifest_sha256:identity,reviewer:'synthetic-test-only',
      approved_at:'2026-01-01T00:00:00Z',maximum_microusd:373728}),{mode:0o600});approvals.push(p);
  }
  const settings=path.join(directory,'settings.json'),values={version:1,approvals,
    secret:{address:'https://unreachable.example.test',mount:'secret',path:'synthetic/eval',field:'OPENAI_API_KEY',version:1}};
  await fs.writeFile(settings,JSON.stringify(values),{mode:0o600});
  return {root,source,settings,values,approvals,reads,directory};
}
function mock(t,mode,abort){
  let calls=0;
  t.mock.method(globalThis,'fetch',async(url,options)=>{
    calls++;assert.equal(url,'https://api.openai.com/v1/responses');
    assert.equal(options.headers.Authorization,'Bearer synthetic-diagnostic-key');
    assert.equal(options.redirect,'error');
    const body=JSON.parse(options.body);assert.equal(body.model,'gpt-5.4-mini-2026-03-17');
    assert.equal(body.max_output_tokens,16384);assert.deepEqual(body.tools,[]);
    if(mode==='abort')return new Promise((_resolve,reject)=>{
      options.signal.addEventListener('abort',()=>reject(Error('PRIVATE_ERROR')),{once:true});abort.abort();
    });
    const msg={type:'message',role:'assistant',phase:'final_answer',status:'completed',content:[{type:'output_text',annotations:[],
      text:'{"action":{"kind":"command","argv":["/PRIVATE_NEVER_EXECUTE"]}}'}]};
    return new Response(JSON.stringify({id:'resp_synthetic',object:'response',status:'completed',model:body.model,
      error:null,incomplete_details:null,output:mode==='multiple'?[msg,{...msg,content:[{...msg.content[0],text:msg.content[0].text+' '}]}]:[msg],
      usage:{input_tokens:100,output_tokens:20,total_tokens:120,input_tokens_details:{cached_tokens:0,cache_write_tokens:0},
        output_tokens_details:{reasoning_tokens:10}}}),{headers:{'Content-Type':'application/json'}});
  });
  return ()=>calls;
}

test('concurrent one-shot launches make exactly one call and never execute or persist returned action', {skip:!available,timeout:120000}, async t=>{
  const f=await fixture(t),calls=mock(t,'valid');
  const outcomes=await Promise.allSettled([runResponseDiagnostic(f.root,f.settings),runResponseDiagnostic(f.root,f.settings)]);
  assert.equal(outcomes.filter(x=>x.status==='fulfilled').length,1);
  const result=outcomes.find(x=>x.status==='fulfilled').value;
  assert.equal(result.accepted,true);assert.equal(result.official_evidence,false);assert.equal(result.benchmark_observations,0);
  assert.equal(result.usage.requests,1);assert.equal(calls(),1);
  await assert.rejects(runResponseDiagnostic(f.root,f.settings),/already started/);
  assert.equal(await fs.readFile(f.reads,'utf8'),'read\n');assert.equal(calls(),1);
  const dir=path.join(f.root,'private/diagnostic-attempt/results');
  for(const n of await fs.readdir(dir))assert(!(await fs.readFile(path.join(dir,n),'utf8')).includes('PRIVATE_NEVER_EXECUTE'));
});

for(const mode of ['multiple','abort'])test(`one-shot ${mode} retains uncertainty and blocks restart`,{skip:!available,timeout:120000},async t=>{
  const f=await fixture(t),abort=new AbortController(),calls=mock(t,mode,abort);
  await assert.rejects(runResponseDiagnostic(f.root,f.settings,abort.signal));
  assert.equal(calls(),1);
  await assert.rejects(runResponseDiagnostic(f.root,f.settings),/already started/);
  assert.equal(await fs.readFile(f.reads,'utf8'),'read\n');
  const dir=path.join(f.root,'private/diagnostic-attempt');
  const rows=(await fs.readFile(path.join(dir,'budget.jsonl'),'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(rows.filter(r=>r.event==='reserved').length,1);
  assert.equal(rows.filter(r=>r.event==='settled').length,0);
  const failure=(await fs.readdir(dir)).find(n=>n.startsWith('failure-'));
  const bytes=await fs.readFile(path.join(dir,failure),'utf8');assert(!bytes.includes('PRIVATE'));
  if(mode==='multiple'){
    assert.equal(JSON.parse(bytes).output_reason,'multiple_messages');
    assert.deepEqual(JSON.parse(bytes).output_shape.map(x=>x.phase),['final_answer','final_answer']);
  }
});

test('missing approval, changed input and ordinary campaign launch stop before key access', {skip:!available,timeout:120000}, async t=>{
  const f=await fixture(t),calls=mock(t,'valid');
  const ordinary=path.join(f.directory,'ordinary.json');
  await fs.writeFile(ordinary,JSON.stringify({...f.values,split:'development',disposable_database:true,
    database_url:'postgresql://synthetic:synthetic@127.0.0.1:9/memory_test'}),{mode:0o600});
  await assert.rejects(runPaidCampaign(f.root,ordinary),/diagnostic pack/);
  const approval=await fs.readFile(f.approvals[1]);await fs.unlink(f.approvals[1]);
  await assert.rejects(runResponseDiagnostic(f.root,f.settings));
  await fs.writeFile(f.approvals[1],approval);
  const input=path.join(f.root,'private/diagnostic-payload.json');await fs.chmod(input,0o600);await fs.appendFile(input,' ');
  await assert.rejects(runResponseDiagnostic(f.root,f.settings),/changed diagnostic/);
  await assert.rejects(fs.stat(f.reads),{code:'ENOENT'});assert.equal(calls(),0);
  await assert.rejects(fs.stat(path.join(f.root,'private/diagnostic-attempt')),{code:'ENOENT'});
  await assert.rejects(prepareResponseDiagnostic(f.source,f.root));
});
