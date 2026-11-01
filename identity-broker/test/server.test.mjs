// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { request } from 'node:http';
import { mkdtemp,chmod,lstat,rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { listenProtected } from '../src/server.mjs';
function call(socket,path,body) {
  return new Promise((resolve,reject)=>{
    const req=request({socketPath:socket,path,method:body?'POST':'GET',agent:false,headers:body?{'content-type':'application/json'}:{}},res=>{
      let text='';res.setEncoding('utf8');res.on('data',data=>text+=data);res.on('end',()=>resolve({status:res.statusCode,body:JSON.parse(text),headers:res.headers}));
    });req.on('error',reject);req.end(body?JSON.stringify(body):undefined);
  });
}
test('Unix listener exposes only fixed routes and has restricted socket permissions',async()=>{
  const dir=await mkdtemp(join(tmpdir(),'codifix-identity-'));const socket=join(dir,'broker.sock');
  const calls=[];
  const broker={client:{clientMetadata:{scope:'atproto'}},async begin(binding,hint){calls.push({binding,hint});return {authorization_url:'https://auth.codifix.net'};},async poll(){return {status:'pending'};},async acknowledge(){return {acknowledged:true};},async discover(){return [];},async callback(params){assert.equal(params.get('code'),'synthetic-code');return {ok:true};}};
  let runtime;
  try {
    runtime=await listenProtected(socket,broker);assert.equal((await lstat(socket)).mode&0o777,0o660);
    assert.deepEqual((await call(socket,'/oauth/client-metadata.json')).body,{scope:'atproto'});
    assert.equal((await call(socket,'/oauth/callback?code=synthetic-code')).status,200);
    assert.equal((await call(socket,'/internal/link/begin',{binding:{test:true},login_hint:'alice.codifix.net'})).status,200);assert.equal(calls.length,1);
    assert.equal((await call(socket,'/internal/link/begin')).status,404);
    assert.equal((await call(socket,'/arbitrary')).status,404);
    assert.equal((await call(socket,'/internal/discover',{identifier:'a'.repeat(9000)})).status,400);
    assert.equal((await call(socket,'/oauth/client-metadata.json')).headers['cache-control'],'no-store');
    await assert.rejects(listenProtected(socket,broker),/already exists/);
  } finally {if(runtime)await runtime.close();await rm(dir,{recursive:true,force:true});}
});
test('listener refuses group-writable parent before creating a socket',async()=>{
  const dir=await mkdtemp(join(tmpdir(),'codifix-identity-'));
  try {await chmod(dir,0o770);await assert.rejects(listenProtected(join(dir,'broker.sock'),{}),/not group\/world writable/);}
  finally {await rm(dir,{recursive:true,force:true});}
});
