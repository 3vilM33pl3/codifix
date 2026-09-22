// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { minimizeTrust, expandTrust, TRUST_SCHEMA } from './trust-expectation.mjs';
const id=n=>`00000000-0000-0000-0000-${String(n).padStart(12,'0')}`;
const h='a'.repeat(64);
function fixture(){
  const rows=new Map(['public.projects','public.memory_canonical_state','public.memory_entries'].map((t,i)=>[t+'\0'+id(i+1),{source:h,target:h,transformed:false}]));
  const e={rows,histories:new Set([id(2)]),projects:new Set([id(1)]),provenance:new Map(),versions:1,tombstones:1};
  const state={sourceMigration:60,prepared:{source_instance:id(10),snapshot_id:id(11),inventory_sha256:h,manifest_sha256:h,
    partitions:[{area:'trust',partition_sha256:h},{area:'general',partition_sha256:'b'.repeat(64)}]},
    expected:new Map([['trust',e],['general',{raw:'NEVER INCLUDE OTHER AREA'}]])};
  return minimizeTrust(state,{area:'trust',operation_id:id(12)},{area:'trust',instance_id:id(13)},
    {contract_sha256:h,prepared_file_sha256:h,manifest_file_sha256:h,archive_sha256:h});
}
test('only Trust expectations survive serialization, with immutable identity and tombstones',()=>{
  const v=fixture(),s=expandTrust(JSON.parse(JSON.stringify(v)));
  assert.equal(s.expected.size,1);assert.equal(s.expected.get('trust').rows.size,3);
  assert.equal(s.expected.get('trust').tombstones,1);assert.equal(v.target_instance,id(13));
  assert.equal(v.required_schema_sha256,TRUST_SCHEMA);assert.ok(!Object.hasOwn(v,'schema_sha256'));
  assert.ok(!JSON.stringify(v).includes('GENERAL'));assert.ok(!JSON.stringify(v).includes('NEVER'));
  assert.equal(v.activation_authorized,false);assert.equal(v.current_source_verified,false);
});
test('changed profile, identities, authority, rows and hidden fields fail closed',()=>{
  for(const change of [v=>v.extra='hidden',v=>v.area='general',v=>v.required_schema_sha256=h,v=>v.target_instance=v.source_instance,
    v=>v.activation_authorized=true,v=>v.current_source_verified=true,v=>v.bindings.local_path='/secret',
    v=>v.rows[0].key='public.raw_captures\0'+id(1),v=>v.rows[0].target='b'.repeat(64),v=>v.rows.push(v.rows[0]),
    v=>v.rows.reverse(),v=>v.rows[0].raw_body='SECRET',v=>v.histories.push(id(44)),v=>v.versions++,v=>v.source_migration=61,
    v=>v.source_migration=1,v=>v.source_migration=51,v=>v.source_migration='60',
    v=>{v.schema_sha256=v.required_schema_sha256;delete v.required_schema_sha256;}]){
    const v=fixture();change(v);assert.throws(()=>expandTrust(v));
  }
});
function withProvenance(){
  const v=fixture();v.rows.push({key:'public.memory_sources\0'+id(20),source:h,target:'b'.repeat(64),transformed:true});
  v.rows.sort((a,b)=>Buffer.compare(Buffer.from(a.key),Buffer.from(b.key)));
  v.provenance=[{source_id:id(20),decision_id:id(21),decision_sha256:h,canonical_id:id(2),memory_version_id:id(3),
    source_sha256:h,original_version_sha256:h,archived_task_id:id(22),archived_task_sha256:h,target_sha256:'b'.repeat(64)}];
  return v;
}
test('reference-only evidence preserves receipt bindings, never source text',()=>{
  const v=withProvenance();
  assert.equal(expandTrust(v).expected.get('trust').provenance.size,1);
  for(const change of [x=>x.provenance[0].excerpt='hidden',x=>x.provenance[0].target_sha256=h,x=>x.provenance[0].canonical_id=id(99),
    x=>x.provenance=[],x=>x.rows.find(r=>r.key.includes('memory_sources')).transformed=false,
    x=>x.provenance[0].original_version_sha256='c'.repeat(64)]){
    const x=structuredClone(v);change(x);assert.throws(()=>expandTrust(x));
  }
});

test('all UUID/hash fields reject coercible arrays and boxed strings',()=>{
  const checks=[];
  for(const key of ['source_instance','snapshot_id','target_instance','operation_id','inventory_sha256','manifest_sha256','partition_sha256'])
    checks.push(v=>[v,key]);
  for(const key of ['contract_sha256','prepared_file_sha256','manifest_file_sha256','archive_sha256'])checks.push(v=>[v.bindings,key]);
  for(const key of ['histories','projects'])checks.push(v=>[v[key],0]);
  for(const key of ['source','target'])checks.push(v=>[v.rows[0],key]);
  for(const key of Object.keys(withProvenance().provenance[0]))checks.push(v=>[v.provenance[0],key]);
  for(const locate of checks)for(const wrap of [value=>[value],value=>new String(value)]){
    const v=withProvenance(),[object,key]=locate(v);object[key]=wrap(object[key]);
    assert.throws(()=>expandTrust(v));
  }
  // Different JS types cannot bypass the source/target identity separation.
  const v=fixture();v.source_instance=[v.target_instance];assert.throws(()=>expandTrust(v));
});

test('duplicate provenance decision identities are rejected before verification',()=>{
  const v=withProvenance();
  v.rows.push({key:'public.memory_sources\0'+id(23),source:h,target:'b'.repeat(64),transformed:true});
  v.rows.sort((a,b)=>Buffer.compare(Buffer.from(a.key),Buffer.from(b.key)));
  v.provenance.push({...v.provenance[0],source_id:id(23)});
  assert.throws(()=>expandTrust(v));
  v.provenance[1].decision_id=id(24);
  assert.equal(expandTrust(v).expected.get('trust').provenance.size,2);
});

test('single-column row keys must be non-nil UUIDs',()=>{
  for(const table of ['public.memory_entries','public.memory_relations','public.memory_sources'])
    for(const key of ['arbitrary text',id(0),'["'+id(3)+'"]',id(3)+'\0suffix']){
      const v=fixture();v.rows.push({key:table+'\0'+key,source:h,target:h,transformed:table==='public.memory_sources'});
      v.rows.sort((a,b)=>Buffer.compare(Buffer.from(a.key),Buffer.from(b.key)));
      if(table==='public.memory_entries')v.versions++;
      assert.throws(()=>expandTrust(v));
    }
});

test('composite tag keys retain exact private text and reference an existing version',()=>{
  const tag='PRIVATE tag "review" \\ newline\n accents café';
  const key=`[${JSON.stringify(id(3))}, ${JSON.stringify(tag)}]`;
  const v=fixture();v.rows.push({key:'public.memory_tags\0'+key,source:h,target:h,transformed:false});
  v.rows.sort((a,b)=>Buffer.compare(Buffer.from(a.key),Buffer.from(b.key)));
  assert.ok(expandTrust(v).expected.get('trust').rows.has('public.memory_tags\0'+key));
  // These are private expectations, not content-free status metadata.
  assert.ok(JSON.stringify(v).includes('PRIVATE'));
  for(const bad of [id(3),JSON.stringify([id(3),tag]),`["${id(3)}", null]`,`["${id(3)}", 1]`,
    `["${id(0)}", "tag"]`,`["${id(99)}", "tag"]`,`[["${id(3)}"], "tag"]`,`["${id(3)}", "tag", "extra"]`,
    `["${id(3)}", "\\u0000"]`,`["${id(3)}", "\\ud800"]`]){
    const x=structuredClone(v);x.rows.find(r=>r.key.startsWith('public.memory_tags\0')).key='public.memory_tags\0'+bad;
    assert.throws(()=>expandTrust(x));
  }
});

test('only explicitly supported source heads are decoded',()=>{
  for(let head=52;head<=60;head++){
    const v=fixture();v.source_migration=head;assert.equal(expandTrust(v).sourceMigration,head);
  }
});
