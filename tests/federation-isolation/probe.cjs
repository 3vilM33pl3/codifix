// SPDX-License-Identifier: AGPL-3.0-or-later
const {readFileSync,existsSync}=require('node:fs');
const {randomUUID}=require('node:crypto');
const https=require('node:https');const net=require('node:net');const assert=require('node:assert/strict');
const config=JSON.parse(readFileSync('/member/probe.json'));
const token=readFileSync('/member/token','utf8');
const ca=readFileSync('/member/ca.pem');
async function request(host,path,auth=token,trust=ca,body=null) {
  return new Promise((resolve,reject)=>{
    const req=https.request({hostname:host,port:8443,path,ca:trust,method:body?'POST':'GET',headers:{authorization:`Bearer ${auth}`,'content-type':'application/json'},timeout:2500},res=>{
      let data='';res.on('data',chunk=>{data+=chunk;if(data.length>1024*1024) req.destroy(new Error('too large'));});
      res.on('end',()=>resolve({status:res.statusCode,data}));
    });req.on('error',reject);req.on('timeout',()=>req.destroy(new Error('timeout')));req.end(body?JSON.stringify(body):undefined);
  });
}
async function tcp(host,port) {
  return new Promise(resolve=>{
    const socket=net.connect({host,port});const done=result=>{socket.destroy();resolve(result);};
    socket.setTimeout(500);socket.on('connect',()=>done(true));socket.on('error',()=>done(false));socket.on('timeout',()=>done(false));
  });
}
(async()=>{
  const nonce=randomUUID();const path=`/v1/pull?after=0&limit=1&nonce=${nonce}`;
  let last='not attempted';
  for(let attempt=0;;attempt++) {
    try {const r=await request(config.gateway,path);if(r.status===200) break;last=`HTTP ${r.status}`;} catch(error) {last=error.code||error.message;}
    if(attempt>=15) throw new Error(`gateway readiness timed out (${last})`);
    await new Promise(resolve=>setTimeout(resolve,250));
  }
  const result=await request(config.gateway,path);assert.equal(result.status,200);
  const signed=JSON.parse(result.data);assert.equal(signed.claims.nonce,nonce);assert.equal(signed.claims.page.space,config.space);assert.equal(signed.signature.length,64);
  assert.equal((await request(config.gateway,path,'wrong-token')).status,401);
  assert.equal((await request(config.otherGateway,path)).status,401);
  const expertise = {nonce:randomUUID(),operation:{operation:'discover',after:null,limit:1}};
  const discovered=await request(config.gateway,'/v1/expertise',token,ca,expertise);
  assert.equal(discovered.status,200);
  const reply=JSON.parse(discovered.data);
  assert.equal(reply.claims.nonce,expertise.nonce);
  assert.equal(reply.claims.purpose,'codifix-expertise-discover-v1');
  assert.equal((await request(config.gateway,'/v1/expertise','wrong-token',ca,expertise)).status,401);
  assert.equal((await request(config.otherGateway,'/v1/expertise',token,ca,expertise)).status,401);
  await assert.rejects(()=>request(config.gateway,path,token,null));
  for(const raw of ['/xrpc/com.atproto.sync.getRepo','/xrpc/com.atproto.sync.subscribeRepos','/xrpc/com.atproto.repo.listRecords','/mcp','/api/memories','/v1/pull/../export']) {
    assert.equal((await request(config.gateway,raw)).status,404);
  }
  for(const target of config.denied) assert.equal(await tcp(target.host,target.port),false,`bypass ${target.host}:${target.port}`);
  assert.equal(await tcp(config.localDatabase,5432),true,'own instance DB positive control');
  for(const secret of ['/config/gateway.json','/config/audit-key','/config/pds-token','/var/run/docker.sock']) assert.equal(existsSync(secret),false);
  console.log(`PASS ${config.id}: TLS gateway, token/space denial, raw routes, database/PDS IP isolation, private mounts`);
})().catch(error=>{console.error(`FAIL isolated member trust-boundary probe: ${String(error.message).replace(/[a-f0-9]{64}/gi,'[fixture-secret]')}`);process.exitCode=1;});
