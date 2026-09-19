// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { accountingState, accountOriginal, finishAccounting, accountArchive, runAccounting, parseArgs } from './account-staging.mjs';
const hash=v=>createHash('sha256').update(v).digest('hex');
const id=n=>`00000000-0000-0000-0000-${String(n).padStart(12,'0')}`;
const areas=['general','homelab','trust','workshop'];
function fixture(){
  const decisions=[...areas,null].map((area,n)=>({canonical_id:id(n+10),explicitly_approved:true,
    disposition:area?{kind:'area',area}:{kind:'protected_archive',reason:'PRIVATE_REVIEW_REASON'},review:{classification:area?'knowledge':'mixed'},
    approved_archive_reference_source_ids:n===0?[id(90)]:[]}));
  const ledger={format:'memory-area-review-ledger-v1',source_instance:id(1),snapshot_id:id(2),inventory_sha256:hash('inventory'),decisions};
  const original=(table,key,row,canonical_id,destination)=>{const row_json=JSON.stringify(row);return {event:'original_row',record:{table,key},row_json,source_sha256:hash(row_json),canonical_id,destination};};
  const rows=[original('public.projects',id(3),{id:id(3),root_path:'/PRIVATE_PATH'},null,'protected_archive')];
  for(const [n,d]of decisions.entries()){
    const dest=d.disposition.kind==='area'?{area:d.disposition.area}:'protected_archive';
    rows.push(original('public.memory_entries',id(n+30),{id:id(n+30),canonical_id:d.canonical_id,version_no:1,is_tombstone:n===2,canonical_text:'PRIVATE_CONTENT'},d.canonical_id,dest),
      original('public.memory_canonical_state',d.canonical_id,{canonical_id:d.canonical_id},d.canonical_id,dest));
  }
  rows.push(original('public.memory_sources',id(90),{excerpt:'PRIVATE_EVIDENCE'},id(10),{archive_and_project_provenance:'general'}),
    original('public.memory_relations',id(91),{src_canonical_id:id(10),dst_canonical_id:id(10)},null,{area:'general'}),
    original('public.memory_relations',id(92),{src_canonical_id:id(10),dst_canonical_id:id(13)},null,'protected_archive'),
    original('public.raw_captures',id(93),{curated_at:null,payload_json:{secret:'PRIVATE_CAPTURE'}},null,'protected_archive'),
    original('public.raw_captures',id(94),{curated_at:'2026-09-20T00:00:00Z',payload_json:{secret:'PRIVATE_CAPTURE'}},null,'protected_archive'),
    original('public.memory_replacement_proposals',id(95),{status:'pending',body:'PRIVATE_PROPOSAL'},null,'protected_archive'),
    original('public.memory_replacement_proposals',id(96),{status:'PRIVATE_UNKNOWN_STATUS',body:'PRIVATE_PROPOSAL'},null,'protected_archive'),
    original('public.memory_proposals',id(97),{status:'pending',body:'PRIVATE_LOOP'},null,'protected_archive'),
    original('public.memory_proposals',id(98),{status:'approved',body:'PRIVATE_LOOP'},null,'protected_archive'),
    original('public.background_jobs',id(99),{secret:'PRIVATE_JOB'},null,'archive_and_retire'),
    original('public.memory_chunks',id(100),{chunk_text:'PRIVATE_CHUNK'},null,'archive_and_rebuild'));
  rows.sort((a,b)=>Buffer.compare(Buffer.from(a.record.table+'\0'+a.record.key),Buffer.from(b.record.table+'\0'+b.record.key)));
  const tableNames=[...new Set(['public._sqlx_migrations',...rows.map(r=>r.record.table)])];
  const manifest={format_version:2,source_instance:ledger.source_instance,snapshot_id:ledger.snapshot_id,inventory_sha256:ledger.inventory_sha256,areas,
    lineages:decisions.map(d=>({canonical_id:d.canonical_id,disposition:d.disposition})),projects:areas.map(area=>({area,source:{id:id(3),root_path:'/PRIVATE_PATH'}})),
    table_policies:tableNames.map(table=>({table,strategy:table==='public.background_jobs'?'retire_runtime':table==='public.memory_chunks'?'rebuild_derived':'owned'})),
    provenance_projections:[{content:{kind:'archive_reference_only'},binding:{source:{record:{table:'public.memory_sources',key:id(90)},row_sha256:rows.find(r=>r.record.key===id(90)).source_sha256},canonical_id:id(10),destination_area:'general'}}]};
  const prepared={format_version:1,source_instance:ledger.source_instance,snapshot_id:ledger.snapshot_id,inventory_sha256:ledger.inventory_sha256,manifest_sha256:hash('manifest'),source_tables:tableNames.length,source_rows:rows.length,
    partitions:areas.map(area=>({area,rows:area==='general'?4:3,projections:area==='general'?1:0})),max_retained_target_mib:512};
  return {ledger,manifest,prepared,rows};
}
function account(f){const state=accountingState(f.prepared,f.manifest,f.ledger);for(const row of f.rows)accountOriginal(state,row);return finishAccounting(state);}
function archive(f){return [{event:'header',format_version:1,serialization_id:'postgresql-to-jsonb-utf8-utc-v1',...Object.fromEntries(['source_instance','snapshot_id','inventory_sha256','manifest_sha256'].map(k=>[k,f.prepared[k]]))},
  ...f.rows,{event:'complete',row_count:f.prepared.source_rows,table_count:f.prepared.source_tables}].map(v=>JSON.stringify(v)+'\n').join('');}

test('every source row is counted once; shared project copies do not inflate preservation counts',()=>{
  const f=fixture(),r=account(f);
  assert.equal(r.source_rows,f.rows.length);assert.equal(Object.values(r.source_dispositions).reduce((a,b)=>a+b,0),f.rows.length);
  assert.equal(r.table_accounting.find(t=>t.table==='public.projects').original_rows,1);
  assert.deepEqual(r.table_accounting.find(t=>t.table==='public.projects').target_copy_rows,{general:1,homelab:1,trust:1,workshop:1});
  assert.equal(r.table_accounting.find(t=>t.table==='public._sqlx_migrations').original_rows,0);
  assert.equal(r.materialized_areas.general.materialized_rows,5);assert.equal(r.materialized_areas.trust.tombstones,1);
  assert.equal(r.source_public_knowledge.histories,5);assert.equal(r.protected_knowledge.histories,1);
});
test('protected backlog remains actionable without leaking bodies, paths, status strings or reasons',()=>{
  const r=account(fixture()),b=r.protected_backlog;
  assert.equal(b.raw_captures,2);assert.equal(b.uncurated_captures,1);assert.equal(b.replacement_proposals,2);assert.equal(b.pending_replacement_proposals,1);
  assert.equal(b.loop_proposals,2);assert.equal(b.pending_loop_proposals,1);assert.equal(b.protected_cross_area_relations,1);
  assert.equal(r.source_dispositions.archive_and_rebuild,1);assert.equal(r.source_dispositions.archive_and_retire,1);
  assert.ok(!JSON.stringify(r).includes('PRIVATE'));assert.equal(r.limits.database_import_verified,false);assert.equal(r.limits.raw_captures_searchable,false);
  assert.equal(r.actions.find(a=>a.code==='review_pending_loop_proposals').count,1);
});
test('ledger/manifests must agree and unknown destinations, tables or unsafe evidence are refused',()=>{
  for(const mutate of [f=>f.ledger.decisions[0].disposition.area='trust',f=>f.manifest.table_policies[0].table='secret.unknown',
    f=>f.manifest.provenance_projections[0].content.kind='reviewed_excerpt',f=>f.ledger.decisions[0].explicitly_approved=false]){
    const f=fixture();mutate(f);assert.throws(()=>account(f));
  }
  const f=fixture();f.rows[0].destination='silently_discarded';assert.throws(()=>account(f));
});
test('partial histories, duplicate versions, altered bytes and wrong per-area totals fail closed',()=>{
  for(const mutate of [f=>f.rows.splice(0,1),f=>f.rows[0].row_json='{}',f=>f.prepared.partitions[0].rows++,f=>f.rows.push(f.rows[0])]){
    const f=fixture();mutate(f);assert.throws(()=>account(f));
  }
  const f=fixture(),entry=f.rows.find(e=>e.record.table==='public.memory_entries');entry.destination={area:'trust'};assert.throws(()=>account(f));
});
test('cross-area relations cannot be counted as target knowledge',()=>{
  const f=fixture();f.rows.find(r=>r.record.key===id(92)).destination={area:'general'};assert.throws(()=>account(f));
});
test('file-only report binds exact archive and inputs and is exclusively published',async t=>{
  const root=await mkdtemp(join(tmpdir(),'memory-account-staging-'));t.after(()=>rm(root,{recursive:true,force:true}));const f=fixture(),body=archive(f);
  const bindings={};for(const [name,value]of Object.entries({archive:body,manifest:JSON.stringify(f.manifest),ledger:JSON.stringify(f.ledger)})){
    const path=join(root,name+'.json');await writeFile(path,value,{mode:0o600});bindings[name]={path,sha256:hash(value)};
  }
  f.prepared.archive_sha256=bindings.archive.sha256;f.prepared.archive_bytes=Buffer.byteLength(body);f.prepared.manifest_file_sha256=bindings.manifest.sha256;
  const p=JSON.stringify(f.prepared),preparedPath=join(root,'prepared.json');await writeFile(preparedPath,p,{mode:0o600});bindings.prepared={path:preparedPath,sha256:hash(p)};
  const c=JSON.stringify({schema_version:1,kind:'materialized_source_accounting',...bindings}),cp=join(root,'contract.json'),output=join(root,'report.json');await writeFile(cp,c,{mode:0o600});
  const result=await runAccounting(cp,hash(c),output);assert.equal(result.source_rows,f.rows.length);const report=JSON.parse(await readFile(output));assert.equal(report.ledger_sha256,bindings.ledger.sha256);
  assert.ok(!JSON.stringify(report).includes('PRIVATE'));assert.ok(!JSON.stringify(report).includes(root));
  await assert.rejects(runAccounting(cp,hash(c),output));
  const partial=body.slice(0,body.lastIndexOf('{"event":"complete"'));await writeFile(bindings.archive.path,partial);
  f.prepared.archive_bytes=Buffer.byteLength(partial);
  await assert.rejects(accountArchive({...bindings.archive,sha256:hash(partial)},accountingState(f.prepared,f.manifest,f.ledger)));
  assert.throws(()=>parseArgs(['--contract','/x','--output','/y']));
});
