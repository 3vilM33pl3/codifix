#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// File-only per-row disposition review. No content approval, execution or credentials.
import { createHash } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { privateInput } from './restore-descriptor.mjs';
import { privateArtifact } from './inventory.mjs';
import { accountingState, accountArchive } from './account-staging.mjs';

const hash = v => createHash('sha256').update(v).digest('hex');
const sha = /^[0-9a-f]{64}$/;
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const areas = ['general','homelab','trust','workshop'];
const fail = () => { throw Error('Backlog disposition ledger invalid; private diagnostics suppressed'); };
async function bound(binding,limit,json=true) {
  if (!binding || Object.keys(binding).sort().join(',')!=='path,sha256' || !isAbsolute(binding.path??'') || !sha.test(binding.sha256)) fail();
  const read = await privateInput(binding.path,limit,json);
  if (read.sha256!==binding.sha256) fail();
  return read.value;
}

/** Only allowlisted metadata is emitted; opaque record keys are replaced by hashes. */
export function itemDisposition(e) {
  const table=e.record?.table,key=e.record?.key;
  if (e.event!=='original_row' || typeof table!=='string' || !/^[a-z0-9_]+\.[a-z0-9_]+$/.test(table)
    || typeof key!=='string' || !sha.test(e.source_sha256) || typeof e.row_json!=='string' || hash(e.row_json)!==e.source_sha256) fail();
  let action,area=null,pending=false;
  const d=e.destination;
  if (d && typeof d==='object' && Object.keys(d).length===1 && areas.includes(d.area)) {
    action='selected_for_import';area=d.area;
  } else if (d && typeof d==='object' && Object.keys(d).length===1 && areas.includes(d.archive_and_project_provenance)) {
    action='selected_reviewed_reference';area=d.archive_and_project_provenance;
  } else if (d==='archive_and_retire') action='retire_historical_runtime';
  else if (d==='archive_and_rebuild') { action='retain_rebuild_backlog';pending=true; }
  else if (d==='protected_archive') {
    action='preserve_protected_archive';
    if (table==='public.raw_captures') {
      const row=JSON.parse(e.row_json);if(!Object.hasOwn(row,'curated_at'))fail();
      pending=row.curated_at===null;action=pending?'retain_pending_curation':'preserve_archived_capture';
    } else if (['public.sessions','public.tasks'].includes(table)) { action='retain_pending_ownership_review';pending=true; }
    else if (['public.memory_replacement_proposals','public.memory_proposals'].includes(table)) {
      const row=JSON.parse(e.row_json);if(typeof row.status!=='string')fail();
      pending=row.status==='pending';action=pending?'retain_pending_owner_decision':'preserve_archived_decision';
    } else if (table==='public.memory_entries') action='preserve_protected_history';
    else if (table==='public.memory_relations') { action='retain_pending_sharing_review';pending=true; }
  } else fail();
  const item={event:'disposition',table,record_key_sha256:hash(key),source_sha256:e.source_sha256,action,pending};
  if (uuid.test(key) || /^[0-9]{1,20}$/.test(key)) item.record_id=key;
  if (area) item.area=area;
  if (e.canonical_id!==null && e.canonical_id!==undefined) {
    if (!uuid.test(e.canonical_id)) fail();item.canonical_id=e.canonical_id;
  }
  return item;
}

export async function runBacklogLedger(contractPath,contractSha,output) {
  const c=await bound({path:contractPath,sha256:contractSha},64*1024);
  if (Object.keys(c).sort().join(',')!=='archive,kind,ledger,manifest,prepared,reviewer,schema_version'
    || c.schema_version!==1 || c.kind!=='materialized_backlog_dispositions'
    || Object.keys(c.reviewer??{}).sort().join(',')!=='id,kind'
    || c.reviewer.kind!=='agent' || !/^[a-z0-9][a-z0-9_-]{0,63}$/.test(c.reviewer.id??'')) fail();
  if (new Set(['archive','ledger','manifest','prepared'].map(k=>c[k]?.path)).size!==4) fail();
  const prepared=await bound(c.prepared,1024*1024),manifest=await bound(c.manifest,256*1024*1024),ledger=await bound(c.ledger,64*1024*1024);
  if (prepared.manifest_file_sha256!==c.manifest.sha256 || prepared.archive_sha256!==c.archive.sha256) fail();
  const state=accountingState(prepared,manifest,ledger);
  return privateArtifact(output,async write=>{
    let buffered='',bufferedBytes=0,count=0;const streamHash=createHash('sha256'),actions={},tables=new Map();
    async function emit(value) {
      const bytes=JSON.stringify(value)+'\n';streamHash.update(bytes);buffered+=bytes;bufferedBytes+=Buffer.byteLength(bytes);
      if (bufferedBytes>=1024*1024) { await write(buffered);buffered='';bufferedBytes=0; }
    }
    await emit({event:'header',schema_version:1,kind:'memory-backlog-disposition-ledger',contract_sha256:contractSha,
      source_instance:prepared.source_instance,snapshot_id:prepared.snapshot_id,inventory_sha256:prepared.inventory_sha256,
      manifest_sha256:prepared.manifest_sha256,archive_sha256:c.archive.sha256,ownership_ledger_sha256:c.ledger.sha256,
      reviewer:c.reviewer,review_basis:'approved_rc2_preservation_policy_and_hash_bound_ownership_decisions',
      content_review_performed:false,proposal_approval_performed:false,work_enqueued:false,import_verified:false});
    const accounting=await accountArchive(c.archive,state,async e=>{
      const item=itemDisposition(e);
      if(item.table==='public.projects'&&state.projects.has(e.record.key)) {
        item.selected_project_areas=[...state.projects.get(e.record.key)].sort();
        item.action='preserve_archive_and_select_project_copies';
      }
      count++;actions[item.action]=(actions[item.action]??0)+1;
      let table=tables.get(item.table);if(!table){table={count:0,hash:createHash('sha256')};tables.set(item.table,table);}
      table.count++;table.hash.update(JSON.stringify(item)+'\n');await emit(item);
    });
    for (const b of [c.prepared,c.manifest,c.ledger,{path:contractPath,sha256:contractSha}]) await bound(b,Infinity,false);
    if (count!==prepared.source_rows) fail();
    const complete={event:'complete',row_count:count,source_tables:prepared.source_tables,actions,
      table_dispositions:[...tables].map(([table,v])=>({table,rows:v.count,dispositions_sha256:v.hash.digest('hex')})),
      every_source_item_accounted:true,body_sha256:streamHash.digest('hex'),protected_backlog:accounting.protected_backlog};
    await write(buffered+JSON.stringify(complete)+'\n');
    return {status:'backlog_dispositions_recorded_only',source_rows:count,actions,body_sha256:complete.body_sha256,
      content_review_performed:false,proposal_approval_performed:false,work_enqueued:false,import_verified:false};
  });
}

export function parseArgs(args) {
  const options={};for(let i=0;i<args.length;i+=2){if(!['--contract','--contract-sha256','--output'].includes(args[i])||!args[i+1]||options[args[i]])fail();options[args[i]]=args[i+1];}
  if(Object.keys(options).length!==3||!isAbsolute(options['--contract'])||!isAbsolute(options['--output'])||!sha.test(options['--contract-sha256']))fail();return options;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
  try {const a=parseArgs(process.argv.slice(2));console.log(JSON.stringify(await runBacklogLedger(a['--contract'],a['--contract-sha256'],a['--output'])));}
  catch {console.error('Backlog disposition ledger failed; no private rows, paths or proposal bodies disclosed.');process.exitCode=1;}
}
