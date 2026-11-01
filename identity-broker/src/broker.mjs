// SPDX-License-Identifier: AGPL-3.0-or-later
import { randomUUID } from 'node:crypto';
import { publicUrl } from './network.mjs';
export const COLLECTION = 'dev.codifix.agency.v1.capability';
const UUID=/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
export function bindingKey(binding) {
  if (!binding || Object.keys(binding).sort().join(',') !== 'attempt_id,home_instance_id,principal_id,session_binding' || !['attempt_id','home_instance_id','principal_id'].every(k=>UUID.test(binding[k])) || !/^[a-f0-9]{64}$/.test(binding.session_binding)) throw new Error('invalid binding');
  return JSON.stringify([binding.attempt_id,binding.home_instance_id,binding.principal_id,binding.session_binding]);
}
export class EphemeralStore {
  constructor({now=()=>Date.now(),ttl=600000,max=64}={}) { this.entries=new Map(); this.now=now; this.ttl=ttl; this.max=max; }
  sweep() { for(const [key,item] of this.entries) if(item.expires <= this.now()) this.entries.delete(key); }
  async get(key) { this.sweep(); return this.entries.get(key)?.value; }
  async set(key,value) { this.sweep(); if(!this.entries.has(key) && this.entries.size>=this.max) throw new Error('pending state limit'); this.entries.set(key,{value,expires:this.now()+this.ttl}); }
  async del(key) { this.entries.delete(key); }
}
export function metadata(origin) {
  const url=publicUrl(origin);
  if(url.pathname!=='/' || url.search) throw new Error('public_origin must be an HTTPS origin');
  return {client_id:`${url.origin}/v1/network/oauth/client-metadata.json`,client_name:'Codifix identity link',client_uri:url.origin,redirect_uris:[`${url.origin}/v1/network/oauth/callback`],grant_types:['authorization_code','refresh_token'],scope:'atproto',response_types:['code'],application_type:'web',token_endpoint_auth_method:'none',dpop_bound_access_tokens:true};
}
function pdsFromDocument(doc,did) {
  if(doc.id!==did) throw new Error('DID document mismatch');
  const services=(doc.service||[]).filter(s=>(s.id==='#atproto_pds' || s.id===`${did}#atproto_pds`) && s.type==='AtprotoPersonalDataServer');
  if(services.length!==1) throw new Error('exactly one PDS required');
  const url=publicUrl(services[0].serviceEndpoint);
  if(url.pathname!=='/' || url.search) throw new Error('PDS origin required');
  return url.origin;
}
function identifier(input) {
  if(typeof input!=='string' || input.length>2048 || input.trim()!==input || input.includes('?') || input.includes('#')) throw new Error('invalid identifier');
  if(input.startsWith('did:plc:') && /^did:plc:[a-z2-7]{24}$/.test(input)) return input;
  if(input.startsWith('did:web:')) { const host=input.slice(8); publicUrl(`https://${host}`); if(host.includes(':') || host.includes('%') || host.includes('/')) throw new Error('invalid did:web'); return input; }
  if(/^(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/.test(input) && input.length<=253) { publicUrl(`https://${input}`); return input.toLowerCase(); }
  throw new Error('handle or supported DID required');
}
export class IdentityBroker {
  constructor({client,stateStore,sessionStore,fetch,now=()=>Date.now()}) { Object.assign(this,{client,stateStore,sessionStore,fetch,now}); this.attempts=new Map(); }
  sweep() { for(const [id,item] of this.attempts) if(item.expires<=this.now()) this.attempts.delete(id); }
  async resolve(input) {
    const normalized=identifier(input);
    const info=await this.client.identityResolver.resolve(normalized,{noCache:true,signal:AbortSignal.timeout(20000)});
    if(normalized.startsWith('did:') && info.did!==normalized) throw new Error('resolved DID mismatch');
    if(!normalized.startsWith('did:') && info.handle!==normalized) throw new Error('handle/DID mismatch');
    return {did:info.did,handle:info.handle==='handle.invalid'?null:info.handle,pds:pdsFromDocument(info.didDoc,info.did)};
  }
  async begin(binding,loginHint) {
    const key=bindingKey(binding); this.sweep();
    const prior=this.attempts.get(binding.attempt_id);
    if(prior) { if(prior.key!==key || prior.loginHint!==loginHint) throw new Error('attempt binding mismatch'); if(prior.start) return prior.start; throw new Error('attempt already in progress'); }
    if(this.attempts.size>=32) throw new Error('pending attempt limit');
    const item={key,loginHint,expires:this.now()+600000,appState:randomUUID(),status:'starting'};
    this.attempts.set(binding.attempt_id,item);
    try {
      item.expected=await this.resolve(loginHint);
      const url=await this.client.authorize(item.expected.did,{scope:'atproto',state:item.appState,signal:AbortSignal.timeout(20000)});
      publicUrl(url.toString());
      item.start={authorization_url:url.toString(),expires_at:new Date(item.expires).toISOString()}; item.status='pending';
      return item.start;
    } catch { item.status='failed'; item.result={status:'failed',code:'authorization_failed'}; throw new Error('authorization failed'); }
  }
  bound(binding) { const key=bindingKey(binding); this.sweep(); const item=this.attempts.get(binding.attempt_id); if(!item || item.key!==key) throw new Error('unknown or expired link attempt'); return item; }
  async poll(binding) { const item=this.bound(binding); return item.result || {status:'pending'}; }
  async acknowledge(binding) { const item=this.bound(binding); if(!item.result) throw new Error('attempt not complete'); this.attempts.delete(binding.attempt_id); return {acknowledged:true}; }
  async callback(params) {
    // Inspect the SDK's own CSRF state before calling it; no user identity comes from callback input.
    const state=params.get('state');
    if(!state || [...params].length>8 || [...params.keys()].some(key=>params.getAll(key).length!==1 || !['state','code','iss','error','error_description','error_uri'].includes(key))) throw new Error('invalid OAuth callback');
    const saved=await this.stateStore.get(state);
    this.sweep();
    const item=[...this.attempts.values()].find(i=>i.appState===saved?.appState);
    if(!item || item.status!=='pending') throw new Error('unknown or replayed callback');
    item.status='verifying'; let session;
    try {
      const result=await this.client.callback(params); session=result.session;
      if(result.state!==item.appState || session.did!==item.expected.did) throw new Error('OAuth subject mismatch');
      const token=await session.getTokenInfo(false);
      const verified=await this.resolve(session.did);
      if(token.sub!==verified.did || token.scope!=='atproto' || new URL(token.aud).origin!==verified.pds || token.iss!==session.serverMetadata.issuer) throw new Error('OAuth issuer/resource/scope mismatch');
      publicUrl(token.iss);
      let cleanup='revoked';
      try { await session.signOut(); } catch { cleanup='discarded_revocation_failed'; }
      await this.sessionStore.del(session.did);
      item.result={status:'complete',...verified,verified_at:new Date(this.now()).toISOString(),cleanup};
      item.status='complete';
      return {ok:true};
    } catch {
      if(session) { try { await session.signOut(); } catch {} await this.sessionStore.del(session.did); }
      item.result={status:'failed',code:'verification_failed'}; item.status='failed';
      throw new Error('identity verification failed');
    } finally { await this.stateStore.del(state); }
  }
  async discover(input) {
    let account=input, rkey;
    if(typeof input==='string' && input.startsWith('at://')) {
      const parts=input.slice(5).split('/');
      if(parts.length!==3 || parts[1]!==COLLECTION || !/^[a-zA-Z0-9._~:-]{1,512}$/.test(parts[2]) || ['.','..'].includes(parts[2])) throw new Error('invalid capability URI');
      [account,,rkey]=parts;
    }
    const identity=await this.resolve(account);
    const url=new URL(`${identity.pds}/xrpc/com.atproto.repo.${rkey?'getRecord':'listRecords'}`);
    url.searchParams.set('repo',identity.did); url.searchParams.set('collection',COLLECTION);
    if(rkey) url.searchParams.set('rkey',rkey); else url.searchParams.set('limit','100');
    const response=await this.fetch(url);
    if(!response.ok) throw new Error('capability read rejected');
    const body=await response.json();
    const records=rkey?[body]:body.records;
    if(!Array.isArray(records) || records.length>100) throw new Error('invalid capability list');
    const prefix=`at://${identity.did}/${COLLECTION}/`;
    return records.map(record=>{
      if(typeof record.uri!=='string' || !record.uri.startsWith(prefix) || !/^[a-zA-Z0-9._~:-]{1,512}$/.test(record.uri.slice(prefix.length)) || (rkey && record.uri!==prefix+rkey) || typeof record.cid!=='string' || record.cid.length<10 || record.cid.length>256 || record.value?.$type!==COLLECTION) throw new Error('capability provenance mismatch');
      publicUrl(record.value.agentCardUrl); // Validate, but never visit or authenticate to an advertised endpoint.
      return {...identity,uri:record.uri,cid:record.cid,fetched_at:new Date(this.now()).toISOString(),profile:record.value};
    });
  }
}
