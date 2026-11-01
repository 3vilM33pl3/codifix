// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { Readable } from 'node:stream';
import { publicAddress,publicUrl,boundedJson,createPublicFetch } from '../src/network.mjs';

test('public transport rejects local, mapped, reserved and metadata addresses',()=>{
  for(const address of ['127.0.0.1','10.1.2.3','169.254.169.254','100.64.0.1','192.168.1.1','0.0.0.0','198.18.1.1','224.1.2.3','::1','::ffff:127.0.0.1','fc00::1','fe80::1','2001:db8::1','2002:7f00:1::']) assert.equal(publicAddress(address),false,address);
  assert.equal(publicAddress('8.8.8.8'),true);assert.equal(publicAddress('2606:4700:4700::1111'),true);
  for(const url of ['http://example.com','https://127.0.0.1','https://[::1]','https://x.local','https://example.com:8080','https://user:pass@example.com','https://example.com/#x']) assert.throws(()=>publicUrl(url));
});
test('JSON complexity limits apply before SDK schema parsing',()=>{
  assert.throws(()=>boundedJson(JSON.stringify({items:Array(513).fill(null)})));
  let nested={};for(let i=0;i<33;i++)nested={next:nested};assert.throws(()=>boundedJson(JSON.stringify(nested)));
  assert.deepEqual(boundedJson('{"issuer":"https://auth.codifix.net"}'),{issuer:'https://auth.codifix.net'});
});
test('DNS pinning rejects mixed public/private results without opening a socket',async()=>{
  let sent=false;
  const fetch=createPublicFetch({resolve:async()=>[{address:'8.8.8.8',family:4},{address:'127.0.0.1',family:4}],transport:()=>{sent=true;}});
  await assert.rejects(fetch('https://auth.codifix.net'));assert.equal(sent,false);
});
function transportResponse(status,body,inspect=()=>{}) {
  return (_url,options,callback)=>{
    inspect(options);
    const outgoing=new EventEmitter();outgoing.end=()=>queueMicrotask(()=>{const response=Readable.from([Buffer.from(body)]);response.statusCode=status;response.headers={'content-type':'application/json'};callback(response);});return outgoing;
  };
}
test('the checked DNS address is used for connection and redirects are never followed',async()=>{
  let lookups=0;
  const resolve=async()=>{lookups++;return [{address:'8.8.8.8',family:4}];};
  const fetch=createPublicFetch({resolve,transport:transportResponse(200,'{"ok":true}',options=>options.lookup('auth.codifix.net',{},(_,address)=>assert.equal(address,'8.8.8.8')))});
  assert.deepEqual(await (await fetch('https://auth.codifix.net')).json(),{ok:true});assert.equal(lookups,1);
  await assert.rejects(createPublicFetch({resolve,transport:transportResponse(302,'{}')})('https://auth.codifix.net'));
});
test('oversized remote bodies are rejected before parsing',async()=>{
  const fetch=createPublicFetch({resolve:async()=>[{address:'8.8.8.8',family:4}],transport:transportResponse(200,'x'.repeat(262145))});
  await assert.rejects(fetch('https://auth.codifix.net'));
});
