// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash,randomUUID } from 'node:crypto';
import { NodeOAuthClient } from '@atproto/oauth-client-node';
import { IdentityBroker,EphemeralStore,metadata } from '../src/broker.mjs';

function fixture({wrongIssuer=false,wrongResource=false}={}) {
  const did='did:web:alice.codifix.net',pds='https://pds.codifix.net',issuer='https://auth.codifix.net';
  const states=new EphemeralStore(),sessions=new EphemeralStore();
  let par,exchanged=false,revoked=0;
  const requests=[];
  const fetch=async(input,init)=>{
    const req=new Request(input,init),url=new URL(req.url);requests.push(url.href);
    if(url.href==='https://alice.codifix.net/.well-known/did.json') return Response.json({id:did,service:[{id:'#atproto_pds',type:'AtprotoPersonalDataServer',serviceEndpoint:pds}]});
    if(url.href===`${pds}/.well-known/oauth-protected-resource`) return Response.json({resource:pds,authorization_servers:[wrongResource&&exchanged?'https://other.codifix.net':issuer]});
    if(url.pathname==='/.well-known/oauth-authorization-server') return Response.json({issuer:url.origin,authorization_endpoint:`${url.origin}/authorize`,token_endpoint:`${url.origin}/token`,revocation_endpoint:`${url.origin}/revoke`,pushed_authorization_request_endpoint:`${url.origin}/par`,require_pushed_authorization_requests:true,client_id_metadata_document_supported:true,authorization_response_iss_parameter_supported:true,token_endpoint_auth_methods_supported:['none'],response_types_supported:['code'],grant_types_supported:['authorization_code','refresh_token'],code_challenge_methods_supported:['S256'],dpop_signing_alg_values_supported:['ES256'],scopes_supported:['atproto']});
    if(url.pathname==='/par') {
      assert.ok(req.headers.get('DPoP'),'PAR must have DPoP proof');par=new URLSearchParams(await req.text());
      assert.equal(par.get('scope'),'atproto');assert.equal(par.get('code_challenge_method'),'S256');
      return Response.json({request_uri:'urn:ietf:params:oauth:request_uri:fixture',expires_in:60},{status:201});
    }
    if(url.pathname==='/token') {
      assert.ok(req.headers.get('DPoP'),'token exchange must have DPoP proof');const body=new URLSearchParams(await req.text());
      assert.equal(createHash('sha256').update(body.get('code_verifier')).digest('base64url'),par.get('code_challenge'));
      assert.equal(body.get('redirect_uri'),'https://codifix.net/v1/network/oauth/callback');exchanged=true;
      return Response.json({access_token:'synthetic-access',refresh_token:'synthetic-refresh',token_type:'DPoP',scope:'atproto',sub:did,expires_in:300});
    }
    if(url.pathname==='/revoke') {revoked++;return Response.json({});}
    throw new Error('unexpected fixture request');
  };
  const client=new NodeOAuthClient({clientMetadata:metadata('https://codifix.net'),stateStore:states,sessionStore:sessions,fetch});
  const broker=new IdentityBroker({client,stateStore:states,sessionStore:sessions,fetch});
  const binding={attempt_id:randomUUID(),principal_id:randomUUID(),home_instance_id:randomUUID(),session_binding:'b'.repeat(64)};
  return {broker,binding,did,sessions,requests,params:()=>new URLSearchParams({code:'fixture-code',state:par.get('state'),iss:wrongIssuer?'https://evil.codifix.net':issuer}),counts:()=>({exchanged,revoked})};
}
test('real pinned SDK completes PAR, PKCE and DPoP exchange and discards credentials',async()=>{
  const f=fixture();await f.broker.begin(f.binding,f.did);await f.broker.callback(f.params());
  assert.equal((await f.broker.poll(f.binding)).status,'complete');assert.equal((await f.broker.poll(f.binding)).cleanup,'revoked');
  assert.equal(await f.sessions.get(f.did),undefined);assert.equal(f.counts().exchanged,true);assert.ok(f.counts().revoked>=2);
  await assert.rejects(f.broker.callback(f.params()));
});
test('real SDK rejects wrong response issuer before token exchange',async()=>{
  const f=fixture({wrongIssuer:true});await f.broker.begin(f.binding,f.did);await assert.rejects(f.broker.callback(f.params()));
  assert.equal(f.counts().exchanged,false);assert.equal((await f.broker.poll(f.binding)).status,'failed');
});
test('real SDK rejects changed authoritative PDS issuer during token exchange',async()=>{
  const f=fixture({wrongResource:true});await f.broker.begin(f.binding,f.did);await assert.rejects(f.broker.callback(f.params()));
  assert.equal((await f.broker.poll(f.binding)).status,'failed');assert.equal(await f.sessions.get(f.did),undefined);
});
