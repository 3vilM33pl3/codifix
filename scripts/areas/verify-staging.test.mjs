// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { readFile, mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { SCHEMA60_TABLES, validateContract, validateTarget, makeExpected, consumeOriginal, finishArchive,
  projectedSource, targetSql, verifyDatabase, readArchive, parseArgs } from './verify-staging.mjs';

const id = n => `00000000-0000-0000-0000-${String(n).padStart(12,'0')}`;
const hash = v => createHash('sha256').update(v).digest('hex');
const areas = ['general','homelab','trust','workshop'];
const file = name => ({path:'/protected/'+name,sha256:hash(name)});
function contract() { return {schema_version:1,kind:'four_inactive_staging',prepared:file('prepared'),manifest:file('manifest'),archive:file('archive'),
  targets:areas.map((area,n)=>({area,target:file(area),container_id:hash(area),container_name:`memory-four-area-staging-fixture-${area}`,
    image_id:'sha256:'+hash('image'),operation_id:id(100+n)}))}; }
function fixture() {
  const c=contract(), spec=c.targets[0], target={format_version:1,instance_id:id(900),area:'general',connection:{kind:'unix',directory:'/protected/socket',port:5432},database:'memory_staging_general',username:'postgres',password:null};
  const prepared={format_version:1,source_instance:id(901),snapshot_id:id(902),inventory_sha256:hash('inventory'),manifest_sha256:hash('manifest'),source_rows:0,
    partitions:areas.map(area=>({area,partition_sha256:hash(area),rows:0,projections:0}))};
  const b={source_instance:prepared.source_instance,snapshot_id:prepared.snapshot_id,inventory_sha256:prepared.inventory_sha256,destination_area:'general',canonical_id:id(2),
    memory_version:{record:{table:'public.memory_entries',key:id(3)},row_sha256:hash('version')},source:{record:{table:'public.memory_sources',key:id(4)},row_sha256:''},
    archived_task:{record:{table:'public.tasks',key:id(5)},row_sha256:hash('private task')}};
  const manifest={format_version:2,source_instance:prepared.source_instance,snapshot_id:prepared.snapshot_id,inventory_sha256:prepared.inventory_sha256,areas,
    lineages:[{canonical_id:id(2),disposition:{kind:'area',area:'general'}}],projects:[{area:'general',source:{id:id(1)}}],records:[],
    provenance_projections:[{format_version:1,decision_id:id(6),binding:b,content:{kind:'archive_reference_only'},operator_attestation:{decision_sha256:hash('decision')}}]};
  const source={id:id(4),memory_entry_id:id(3),task_id:id(5),file_path:'/PRIVATE',git_commit:'PRIVATE',source_kind:'file',excerpt:'PRIVATE',created_at:'2026-09-19T10:00:00+00:00',
    symbol_name:null,symbol_kind:null,line_start:null,line_end:null,content_hash:null,target_memory_id:null};
  const rows=[['public.projects',id(1),{id:id(1),slug:'fixture'},null,'protected_archive'],
    ['public.memory_canonical_state',id(2),{canonical_id:id(2)},id(2),{area:'general'}],
    ['public.memory_entries',id(3),{id:id(3),canonical_id:id(2),project_id:id(1),version_no:1,is_tombstone:true},id(2),{area:'general'}],
    ['public.memory_sources',id(4),source,id(2),{archive_and_project_provenance:'general'}]]
    .map(([table,key,row,canonical_id,destination])=>{const row_json=JSON.stringify(row);return {event:'original_row',record:{table,key},canonical_id,destination,row_json,source_sha256:hash(row_json)};})
    .sort((a,b)=>Buffer.compare(Buffer.from(a.record.table+'\0'+a.record.key),Buffer.from(b.record.table+'\0'+b.record.key)));
  b.source.row_sha256=rows.find(r=>r.record.table==='public.memory_sources').source_sha256;
  prepared.source_rows=rows.length;prepared.source_tables=4;prepared.partitions[0].rows=3;prepared.partitions[0].projections=1;
  const state=makeExpected(prepared,manifest);for(const r of rows)consumeOriginal(state,r);finishArchive(state);state.sourceMigration=60;
  return {c,spec,target,prepared,manifest,rows,state,source};
}
const catalog=()=>SCHEMA60_TABLES.map(table=>({schema:'public',name:table.slice(7),kind:'r',is_partition:false,primary_key:table.endsWith('memory_tags')?['memory_entry_id','tag']:table.endsWith('memory_canonical_state')?['canonical_id']:['id']}));
function events(f) {
  const e=f.state.expected.get('general'),counts=new Map(SCHEMA60_TABLES.map(t=>[t,0]));
  for(const [table,count]of Object.entries({'public._sqlx_migrations':60,'public.supervisor_authorities':4,'public.supervisor_journal_heads':4,'public.timeline_position':1,'public.loop_global_state':1,
    'public.area_import_receipts':1,'public.area_import_rows':e.rows.size,'public.area_import_provenance':e.provenance.size}))counts.set(table,count);
  for(const key of e.rows.keys()){const table=key.split('\0')[0];counts.set(table,counts.get(table)+1);}
  const rows=[{event:'catalog',tables:catalog()},{event:'safety',database:f.target.database,read_only:'on',clients:0,migrations:60,migration_head:60,failed_migrations:0,invalid_indexes:0,invalid_constraints:0,unsafe_seeds:0,broken_links:0},
    ...[...counts].map(([table,count])=>({event:'count',table,count})),
    {event:'area_import_receipts',row:{singleton:true,id:f.spec.operation_id,target_instance:f.target.instance_id,area:'general',source_instance:f.prepared.source_instance,snapshot_id:f.prepared.snapshot_id,
      inventory_sha256:f.prepared.inventory_sha256,manifest_sha256:f.prepared.manifest_sha256,partition_sha256:f.prepared.partitions[0].partition_sha256,row_count:e.rows.size,projection_count:e.provenance.size,source_migration:60,target_migration:60}}];
  for(const [key,v]of e.rows){const [table,k]=key.split('\0');rows.push({event:'row',table,key:k,sha256:v.target},{event:'area_import_rows',row:{import_id:f.spec.operation_id,table_name:table,record_key:k,source_sha256:v.source,target_sha256:v.target,transformed:v.transformed}});}
  for(const v of e.provenance.values())rows.push({event:'area_import_provenance',row:{import_id:f.spec.operation_id,...v}});
  rows.push({event:'complete'});return rows;
}
function io(rows,code=0){return {sent:[],aborted:false,async *lines(){for(const e of rows)yield JSON.stringify(e);},done:Promise.resolve(code),send(sql){this.sent.push(sql);},abort(){this.aborted=true;}};}
function fake(rows,code=0){const x=io(rows,code);x.lines=x.lines();return x;}

test('fixed schema60 table catalog stays aligned with the reviewed Rust allowlist',async()=>{
  const code=(await readFile(new URL('../../crates/mem-area/src/policy.rs',import.meta.url),'utf8')).split('pub(crate) const HISTORICAL_TABLES')[0];
  assert.deepEqual(SCHEMA60_TABLES,[...new Set([...code.matchAll(/"(public\.[a-z0-9_]+)"/g)].map(m=>m[1]))].sort());
  assert.equal(SCHEMA60_TABLES.length,114);
});
test('contract pins four independent exact identities and refuses extra fields or reused targets',()=>{
  assert.equal(validateContract(contract()).targets.length,4);
  for(const mutate of [c=>c.extra=true,c=>c.targets[1].container_id=c.targets[0].container_id,c=>c.targets[0].container_name='memory-production',c=>c.targets[0].operation_id=id(0),c=>c.archive.sha256='bad']){
    const c=contract();mutate(c);assert.throws(()=>validateContract(c));
  }
  assert.throws(()=>parseArgs(['--contract','/x','--output','/y']));
});
test('target requires exact isolated read-only-root container and pinned Unix socket mount',()=>{
  const f=fixture(),metadata={Id:f.spec.container_id,Name:'/'+f.spec.container_name,Image:f.spec.image_id,State:{Running:true},HostConfig:{NetworkMode:'none',ReadonlyRootfs:true},NetworkSettings:{Networks:{none:{}},Ports:{}},Mounts:[{Type:'bind',Source:f.target.connection.directory,Destination:'/var/run/postgresql'}]};
  validateTarget(f.spec,f.target,metadata,f.prepared.source_instance);
  for(const mutate of [m=>m.Image='sha256:'+hash('other'),m=>m.HostConfig.NetworkMode='bridge',m=>m.HostConfig.PortBindings={'5432/tcp':[]},m=>m.HostConfig.CapAdd=['SYS_ADMIN'],m=>m.Mounts[0].Source='/other']){
    const m=structuredClone(metadata);mutate(m);assert.throws(()=>validateTarget(f.spec,f.target,m,f.prepared.source_instance));
  }
});
test('expected rows preserve full originals and make only the exact reference-only projection',()=>{
  const f=fixture(),e=f.state.expected.get('general');assert.equal(e.rows.size,4);assert.equal(e.versions,1);assert.equal(e.tombstones,1);
  const projection=projectedSource(f.source);assert.ok(!projection.includes('PRIVATE'));assert.equal(JSON.parse(projection).task_id,null);assert.equal(JSON.parse(projection).source_kind,'note');
  assert.throws(()=>projectedSource({...f.source,extra:'unknown'}));
  const m=structuredClone(f.manifest);m.provenance_projections[0].content={kind:'reviewed_excerpt',text:'not this profile'};assert.throws(()=>makeExpected(f.prepared,m));
});
test('changed bytes, missing originals, duplicate archive rows and cross-area relations fail closed',()=>{
  const f=fixture();for(const row of [{...f.rows[0],row_json:'{}'},f.rows.at(-1)])assert.throws(()=>consumeOriginal(f.state,row));
  const state=makeExpected(f.prepared,f.manifest);for(const r of f.rows.slice(1))consumeOriginal(state,r);assert.throws(()=>finishArchive(state));
  const s=makeExpected(f.prepared,f.manifest),row={event:'original_row',record:{table:'public.memory_relations',key:id(10)},canonical_id:null,destination:{area:'general'},row_json:JSON.stringify({src_canonical_id:id(2),dst_canonical_id:id(999)})};row.source_sha256=hash(row.row_json);s.relationOwners.set(id(10),'general');assert.throws(()=>consumeOriginal(s,row));
});
test('read-only verifier covers every row, receipt, projection, table and inert seed',async()=>{
  const f=fixture(),x=fake(events(f)),result=await verifyDatabase(x,f.spec,f.target,f.state);
  assert.equal(result.verified_rows,4);assert.equal(result.authority_and_runtime_inactive,true);assert.equal(result.histories,1);
  assert.match(x.sent[0],/REPEATABLE READ READ ONLY/);assert.ok(x.sent.every(s=>! /\b(?:INSERT|UPDATE|DELETE|ALTER|CREATE|nextval|setval)\b/i.test(s)));
  assert.ok(!x.sent.join('').includes('PRIVATE'));
});
test('tampering, authority activation, writer connections, missing rows and bad receipts invalidate verification',async()=>{
  const mutators=[r=>r.find(e=>e.event==='row').sha256=hash('changed'),r=>r.find(e=>e.event==='count'&&e.table==='public.auth_service_tokens').count=1,
    r=>r.find(e=>e.event==='safety').unsafe_seeds=1,r=>r.find(e=>e.event==='safety').clients=1,
    r=>r.find(e=>e.event==='area_import_receipts').row.manifest_sha256=hash('wrong'),
    r=>r.find(e=>e.event==='area_import_provenance').row.decision_id=id(999),
    r=>r.splice(r.findIndex(e=>e.event==='row'),1),r=>r.push({event:'complete'})];
  for(const mutate of mutators){const f=fixture(),r=events(f);mutate(r);const x=fake(r);await assert.rejects(verifyDatabase(x,f.spec,f.target,f.state));assert.equal(x.aborted,true);}
  const f=fixture();await assert.rejects(verifyDatabase(fake(events(f),1),f.spec,f.target,f.state));
});
test('schema drift and foreign/partitioned tables fail before generating target queries',()=>{
  for(const rows of [catalog().slice(1),[...catalog(),{schema:'unsafe',name:'other',kind:'r'}],catalog().map((t,n)=>n===0?{...t,kind:'f'}:t)])assert.throws(()=>targetSql(rows));
});
test('private complete archive binds source identity and exact bytes; partial archives fail',async t=>{
  const root=await mkdtemp(join(tmpdir(),'memory-staging-verify-'));t.after(()=>rm(root,{recursive:true,force:true}));
  const f=fixture(),header={event:'header',format_version:1,serialization_id:'postgresql-to-jsonb-utf8-utc-v1',source_migration_version:60};
  for(const k of ['source_instance','snapshot_id','inventory_sha256','manifest_sha256'])header[k]=f.prepared[k];
  const content=[header,...f.rows,{event:'complete',row_count:4,table_count:4}].map(v=>JSON.stringify(v)+'\n').join('');
  const path=join(root,'archive.ndjson');await writeFile(path,content,{mode:0o600});f.prepared.archive_bytes=Buffer.byteLength(content);
  const state=makeExpected(f.prepared,f.manifest);await readArchive({path,sha256:hash(content)},state);assert.equal(state.expected.get('general').rows.size,4);
  await assert.rejects(readArchive({path,sha256:hash('wrong')},makeExpected(f.prepared,f.manifest)));
  const partial=content.slice(0,content.lastIndexOf('{"event":"complete"'));await writeFile(path,partial);f.prepared.archive_bytes=Buffer.byteLength(partial);
  await assert.rejects(readArchive({path,sha256:hash(partial)},makeExpected(f.prepared,f.manifest)));
});
