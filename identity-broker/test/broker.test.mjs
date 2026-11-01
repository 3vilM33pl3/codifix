// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { IdentityBroker,EphemeralStore,metadata,COLLECTION } from '../src/broker.mjs';
import { NodeOAuthClient } from '@atproto/oauth-client-node';

const did='did:plc:abcdefghijklmnopqrstuvwx';
const binding=()=>({attempt_id:randomUUID(),principal_id:randomUUID(),home_instance_id:randomUUID(),session_binding:'a'.repeat(64)});
function fixture(options={}) {
  let now=Date.now(), callbackCount=0, revoked=0, state;
  const stateStore=new EphemeralStore({now:()=>now}),sessionStore=new EphemeralStore({now:()=>now});
  const doc={id:did,service:[{id:'#atproto_pds',type:'AtprotoPersonalDataServer',serviceEndpoint:'https://pds.codifix.net'}]};
  const client={
    clientMetadata:metadata('https://codifix.net'),
    identityResolver:{async resolve(){return {did,handle:'olivier.codifix.net',didDoc:options.doc||doc};}},
    async authorize(_,{state:appState,scope}) {assert.equal(scope,'atproto');state=randomUUID();await stateStore.set(state,{appState});return new URL('https://auth.codifix.net/authorize');},
    async callback(){callbackCount++;const saved=await stateStore.get(state);await sessionStore.set(options.did||did,{secret:'synthetic-token'});return {state:saved.appState,session:{did:options.did||did,serverMetadata:{issuer:'https://auth.codifix.net'},async getTokenInfo(){return {sub:did,scope:options.scope||'atproto',aud:options.aud||'https://pds.codifix.net',iss:options.issuer||'https://auth.codifix.net'};},async signOut(){revoked++;if(options.revokeFails)throw new Error('remote revoke failed');}}};},
  };
  const requests=[];
  const fetch=async(url)=>{requests.push(String(url));return Response.json(options.record||{records:[]});};
  const broker=new IdentityBroker({client,stateStore,sessionStore,fetch,now:()=>now});
  return {broker,stateStore,sessionStore,params:()=>new URLSearchParams({state,code:'synthetic-code',iss:'https://auth.codifix.net'}),advance:()=>{now+=600001;},requests,counts:()=>({callbackCount,revoked})};
}
test('official SDK accepts identity-only public client configuration',()=>{
  const client=new NodeOAuthClient({clientMetadata:metadata('https://codifix.net'),stateStore:new EphemeralStore(),sessionStore:new EphemeralStore()});
  assert.equal(client.clientMetadata.scope,'atproto'); assert.equal(client.clientMetadata.token_endpoint_auth_method,'none');
});
test('bound completion is retryable until acknowledged and SDK sessions are discarded',async()=>{
  const f=fixture(),b=binding();
  const start=await f.broker.begin(b,'olivier.codifix.net'); assert.match(start.authorization_url,/https:/);
  assert.deepEqual(await f.broker.begin(b,'olivier.codifix.net'),start);
  assert.deepEqual(await f.broker.poll(b),{status:'pending'});
  await assert.rejects(f.broker.poll({...b,principal_id:randomUUID()}));
  await f.broker.callback(f.params());
  const completed=await f.broker.poll(b);
  assert.equal(completed.status,'complete');assert.equal(completed.did,did);assert.equal(completed.cleanup,'revoked');
  assert.deepEqual(await f.broker.poll(b),completed);assert.equal(await f.sessionStore.get(did),undefined);
  await assert.rejects(f.broker.callback(f.params()));assert.equal(f.counts().callbackCount,1);
  await f.broker.acknowledge(b);await assert.rejects(f.broker.poll(b));
});
test('owner, home and session binding cannot change an existing attempt',async()=>{
  const f=fixture(),b=binding();await f.broker.begin(b,'olivier.codifix.net');
  for(const patch of [{principal_id:randomUUID()},{home_instance_id:randomUUID()},{session_binding:'b'.repeat(64)}]) {
    await assert.rejects(f.broker.begin({...b,...patch},'olivier.codifix.net'));
    await assert.rejects(f.broker.poll({...b,...patch}));
  }
});
test('forged, replayed and expired callbacks fail before exchanging a token',async()=>{
  const f=fixture(),b=binding();await f.broker.begin(b,'olivier.codifix.net');
  await assert.rejects(f.broker.callback(new URLSearchParams({state:'forged',code:'test'})));
  f.advance();await assert.rejects(f.broker.callback(f.params()));assert.equal(f.counts().callbackCount,0);
});
for(const options of [{did:'did:plc:bcdefghijklmnopqrstuvwxy2'},{issuer:'https://evil.codifix.net'},{aud:'https://other.codifix.net'},{scope:'atproto transition:generic'}]) test(`rejects callback identity mismatch ${JSON.stringify(options)}`,async()=>{
  const f=fixture(options),b=binding();await f.broker.begin(b,'olivier.codifix.net');
  await assert.rejects(f.broker.callback(f.params()));assert.equal((await f.broker.poll(b)).status,'failed');assert.equal(await f.sessionStore.get(did),undefined);
});
test('failed revocation still discards all local tokens and exposes cleanup truthfully',async()=>{
  const f=fixture({revokeFails:true}),b=binding();await f.broker.begin(b,'olivier.codifix.net');await f.broker.callback(f.params());
  assert.equal((await f.broker.poll(b)).cleanup,'discarded_revocation_failed');assert.equal(await f.sessionStore.get(did),undefined);
});
test('discovery reads only authoritative PDS with URI binding and no agent request',async()=>{
  const profile={$type:COLLECTION,agentCardUrl:'https://agents.codifix.net/agent-card.json'};
  const record={uri:`at://${did}/${COLLECTION}/linear`,cid:'bafyfixture12345',value:profile};
  const f=fixture({record});
  const result=await f.broker.discover(`at://olivier.codifix.net/${COLLECTION}/linear`);
  assert.equal(result[0].did,did);assert.equal(result[0].uri,record.uri);assert.equal(f.requests.length,1);
  assert.match(f.requests[0],/^https:\/\/pds.codifix.net\/xrpc\/com.atproto.repo.getRecord\?/);
});
test('wrong record URI and wrong DID document reject discovery',async()=>{
  const f=fixture({record:{records:[{uri:`at://did:plc:other/${COLLECTION}/linear`,cid:'bafyfixture12345',value:{$type:COLLECTION}}]}});
  await assert.rejects(f.broker.discover(did));
  await assert.rejects(fixture({doc:{id:'did:plc:wrong',service:[]}}).broker.discover(did));
});
test('ambiguous and internal PDS identity documents are rejected',async()=>{
  for(const services of [[],[{id:'#atproto_pds',type:'AtprotoPersonalDataServer',serviceEndpoint:'http://localhost'}],[{id:'#atproto_pds',type:'AtprotoPersonalDataServer',serviceEndpoint:'https://pds.codifix.net'},{id:`${did}#atproto_pds`,type:'AtprotoPersonalDataServer',serviceEndpoint:'https://pds2.codifix.net'}]]) await assert.rejects(fixture({doc:{id:did,service:services}}).broker.discover(did));
});
