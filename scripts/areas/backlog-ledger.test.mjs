// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp,writeFile,readFile,rm,stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { itemDisposition,runBacklogLedger,parseArgs } from './backlog-ledger.mjs';
const hash=v=>createHash('sha256').update(v).digest('hex');
const id=n=>`00000000-0000-0000-0000-${String(n).padStart(12,'0')}`;
function row(table,key,value,destination='protected_archive',canonical_id=null){const row_json=JSON.stringify(value);return{event:'original_row',record:{table,key},row_json,source_sha256:hash(row_json),destination,canonical_id};}

test('pending dispositions preserve privacy and do not approve proposals or copy opaque keys',()=>{
  const examples=[
    ['public.raw_captures',{curated_at:null},'retain_pending_curation'],
    ['public.sessions',{},'retain_pending_ownership_review'],
    ['public.tasks',{},'retain_pending_ownership_review'],
    ['public.memory_replacement_proposals',{status:'pending'},'retain_pending_owner_decision'],
    ['public.memory_proposals',{status:'pending'},'retain_pending_owner_decision'],
    ['public.memory_entries',{},'preserve_protected_history'],
    ['public.memory_relations',{},'retain_pending_sharing_review'],
  ];
  for(const[table,value,action]of examples){const item=itemDisposition(row(table,'PRIVATE_KEY', {...value,body:'PRIVATE_BODY',path:'/PRIVATE_PATH'}));assert.equal(item.action,action);assert.ok(!JSON.stringify(item).includes('PRIVATE'));assert.equal(item.record_key_sha256,hash('PRIVATE_KEY'));}
  assert.equal(itemDisposition(row('public.memory_chunks',id(10),{},'archive_and_rebuild')).action,'retain_rebuild_backlog');
  assert.equal(itemDisposition(row('public.background_jobs',id(11),{},'archive_and_retire')).action,'retire_historical_runtime');
  assert.equal(itemDisposition(row('public.memory_proposals',id(12),{status:'PRIVATE_STATUS'})).action,'preserve_archived_decision');
  const broken=row('public.sessions',id(4),{});broken.row_json='{} ';assert.throws(()=>itemDisposition(broken));
  assert.throws(()=>parseArgs(['--contract','/x']));
});

async function fixture(root){
  const rows=[row('public.projects',id(3),{root_path:'/PRIVATE_PATH'}),
    row('public.memory_entries',id(4),{canonical_id:id(4),version_no:1,is_tombstone:false,canonical_text:'PRIVATE_CONTENT'},{area:'general'},id(4)),
    row('public.memory_canonical_state',id(4),{}, {area:'general'},id(4)),
    row('public.raw_captures',id(5),{curated_at:null,payload:'PRIVATE_CAPTURE'}),
    row('public.sessions',id(6),{body:'PRIVATE_SESSION'}),
    row('public.memory_replacement_proposals',id(7),{status:'pending',body:'PRIVATE_PROPOSAL'}),
    row('public.memory_proposals',id(8),{status:'pending',body:'PRIVATE_PROPOSAL'}),
    row('public.memory_chunks',id(9),{body:'PRIVATE_CHUNK'},'archive_and_rebuild'),
    row('public.background_jobs',id(10),{body:'PRIVATE_JOB'},'archive_and_retire')];
  rows.sort((a,b)=>Buffer.compare(Buffer.from(a.record.table+'\0'+a.record.key),Buffer.from(b.record.table+'\0'+b.record.key)));
  const names=[...new Set(rows.map(r=>r.record.table))].sort();const areas=['general','homelab','trust','workshop'];
  const ledger={format:'memory-area-review-ledger-v1',source_instance:id(1),snapshot_id:id(2),inventory_sha256:hash('inventory'),decisions:[{canonical_id:id(4),explicitly_approved:true,disposition:{kind:'area',area:'general'},review:{classification:'knowledge'},approved_archive_reference_source_ids:[]}]};
  const manifest={format_version:2,source_instance:id(1),snapshot_id:id(2),inventory_sha256:ledger.inventory_sha256,areas,
    lineages:ledger.decisions.map(d=>({canonical_id:d.canonical_id,disposition:d.disposition})),projects:[{area:'general',source:{id:id(3)}}],provenance_projections:[],
    table_policies:names.map(table=>({table,strategy:table==='public.background_jobs'?'retire_runtime':table==='public.memory_chunks'?'rebuild_derived':'owned'}))};
  const prepared={format_version:1,source_instance:id(1),snapshot_id:id(2),inventory_sha256:ledger.inventory_sha256,manifest_sha256:hash('manifest'),source_tables:names.length,source_rows:rows.length,
    partitions:areas.map(area=>({area,rows:area==='general'?3:0,projections:0}))};
  const events=[{event:'header',format_version:1,serialization_id:'postgresql-to-jsonb-utf8-utc-v1',...Object.fromEntries(['source_instance','snapshot_id','inventory_sha256','manifest_sha256'].map(k=>[k,prepared[k]]))},
    ...names.flatMap(table=>{const items=rows.filter(r=>r.record.table===table);return[{event:'table_start',table,row_count:items.length,columns:[],foreign_keys:[]},...items,{event:'table_end',table,row_count:items.length}];}),
    {event:'complete',row_count:rows.length,table_count:names.length}];
  const archive=events.map(v=>JSON.stringify(v)+'\n').join('');const bindings={};
  for(const[name,text]of Object.entries({archive,manifest:JSON.stringify(manifest),ledger:JSON.stringify(ledger)})){const path=join(root,name+'.json');await writeFile(path,text,{mode:0o600});bindings[name]={path,sha256:hash(text)};}
  Object.assign(prepared,{archive_sha256:bindings.archive.sha256,archive_bytes:Buffer.byteLength(archive),manifest_file_sha256:bindings.manifest.sha256});
  const body=JSON.stringify(prepared);bindings.prepared={path:join(root,'prepared.json'),sha256:hash(body)};await writeFile(bindings.prepared.path,body,{mode:0o600});
  const c={kind:'materialized_backlog_dispositions',schema_version:1,reviewer:{kind:'agent',id:'codex-rc2'},...bindings};const cp=join(root,'contract.json');await writeFile(cp,JSON.stringify(c),{mode:0o600});return{c,cp,sha:hash(JSON.stringify(c)),rows,archive};
}

test('streamed complete ledger binds every item, reviewer and archive; tampering cannot publish',async t=>{
  const root=await mkdtemp(join(tmpdir(),'memory-backlog-'));t.after(()=>rm(root,{recursive:true,force:true}));const f=await fixture(root),out=join(root,'dispositions.ndjson');
  const summary=await runBacklogLedger(f.cp,f.sha,out);assert.equal(summary.source_rows,f.rows.length);
  assert.equal(summary.actions.retain_pending_owner_decision,2);assert.equal(summary.work_enqueued,false);
  const body=await readFile(out,'utf8'),events=body.trimEnd().split('\n').map(JSON.parse);assert.ok(!body.includes('PRIVATE'));assert.ok(!body.includes(root));
  assert.equal((await stat(out)).mode&0o777,0o600);assert.equal(events[0].reviewer.kind,'agent');assert.equal(events.length,f.rows.length+2);
  assert.equal(events.at(-1).body_sha256,hash(body.slice(0,body.lastIndexOf('{"event":"complete"'))));
  assert.equal(events.at(-1).every_source_item_accounted,true);
  await assert.rejects(runBacklogLedger(f.cp,f.sha,out));
  await writeFile(f.c.archive.path,f.archive+'\n');const denied=join(root,'denied.ndjson');await assert.rejects(runBacklogLedger(f.cp,f.sha,denied));await assert.rejects(stat(denied));
});
