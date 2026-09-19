#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// File-only accounting: preservation is not searchable migration or activation.
import { constants } from 'node:fs';
import { open } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { privateInput } from './restore-descriptor.mjs';
import { streamLines, privateArtifact } from './inventory.mjs';
import { SCHEMA60_TABLES } from './verify-staging.mjs';
import { archiveFraming } from './archive-framing.mjs';

const AREAS = ['general', 'homelab', 'trust', 'workshop'];
const HISTORICAL = ['_sqlx_migrations','curation_runs','memory_chunks','memory_entries','memory_relations','memory_sources','memory_tags','projects','raw_captures','sessions','tasks']
  .map(name => 'mab_full_20260516_150014.' + name);
const TABLES = new Set([...SCHEMA60_TABLES, ...HISTORICAL]);
const sha = /^[0-9a-f]{64}$/;
const hash = value => createHash('sha256').update(value).digest('hex');
const fail = () => { throw Error('Materialized source accounting failed; private diagnostics suppressed'); };
const classes = ['knowledge','test','evaluation','ambiguous','mixed'];
const same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
function count() { return { exact_area_copy:0, protected_archive:0, archive_and_rebuild:0, archive_and_retire:0, archive_and_project_provenance:0 }; }
function disposition(d) {
  if (d?.kind === 'area' && AREAS.includes(d.area)) return d.area;
  if (d?.kind === 'protected_archive' && typeof d.reason === 'string' && d.reason.trim()) return null;
  fail();
}
export function accountingState(prepared, manifest, ledger) {
  if (prepared.format_version !== 1 || manifest.format_version !== 2 || ledger.format !== 'memory-area-review-ledger-v1'
    || !same([...manifest.areas].sort(),AREAS) || !Array.isArray(ledger.decisions) || prepared.partitions.length !== 4) fail();
  for (const k of ['source_instance','snapshot_id','inventory_sha256']) if (prepared[k] !== manifest[k] || prepared[k] !== ledger[k]) fail();
  const decisions = new Map(), protectedClasses = Object.fromEntries(classes.map(k=>[k,0]));
  for (const d of ledger.decisions) {
    const area = disposition(d.disposition);
    if (decisions.has(d.canonical_id) || d.explicitly_approved !== true || !classes.includes(d.review?.classification)
      || !Array.isArray(d.approved_archive_reference_source_ids)) fail();
    decisions.set(d.canonical_id,{area,sourceIds:new Set(d.approved_archive_reference_source_ids)});
    if (!area) { protectedClasses[d.review.classification]++; if (d.approved_archive_reference_source_ids.length) fail(); }
  }
  if (manifest.lineages.length !== decisions.size) fail();
  const seenLineages = new Set();
  for (const h of manifest.lineages) {
    if (!decisions.has(h.canonical_id) || seenLineages.has(h.canonical_id) || decisions.get(h.canonical_id).area !== disposition(h.disposition)) fail();
    seenLineages.add(h.canonical_id);
  }
  const tables = new Map();
  for (const t of manifest.table_policies) {
    if (!TABLES.has(t.table) || tables.has(t.table) || !['owned','project_provenance','rebuild_derived','historical_archive','retire_runtime'].includes(t.strategy)) fail();
    tables.set(t.table,{table:t.table,strategy:t.strategy,original_rows:0,source_dispositions:count(),target_copy_rows:Object.fromEntries(AREAS.map(a=>[a,0]))});
  }
  if (tables.size !== prepared.source_tables) fail();
  const areas = new Map(AREAS.map(a=>[a,{histories:0,versions:0,tombstones:0,projects:0,projected_source_references:0,relations:0,materialized_rows:0}]));
  const projects = new Map(), projections = new Map(), lineageVersions = new Map(), lineageStates = new Set();
  for (const d of decisions.values()) if (d.area) areas.get(d.area).histories++;
  for (const p of manifest.projects) {
    if (!areas.has(p.area)) fail();
    const key=p.source.id, destinations=projects.get(key)??new Set();
    if (destinations.has(p.area)) fail();destinations.add(p.area);projects.set(key,destinations);areas.get(p.area).projects++;
  }
  for (const p of manifest.provenance_projections) {
    const b=p.binding,d=decisions.get(b.canonical_id);
    if (p.content.kind!=='archive_reference_only' || b.source.record.table!=='public.memory_sources' || !d || d.area!==b.destination_area
      || !d.sourceIds.has(b.source.record.key) || projections.has(b.source.record.key)) fail();
    projections.set(b.source.record.key,{area:b.destination_area,source_sha256:b.source.row_sha256});
  }
  if ([...decisions.values()].reduce((n,d)=>n+d.sourceIds.size,0)!==projections.size) fail();
  return {prepared,tables,areas,decisions,projects,projections,lineageVersions,lineageStates,protectedClasses,
    originalRows:0,lastRecord:null,sourceDispositions:count(),seenProjects:new Set(),seenProjections:new Set(),
    publicVersions:0,publicTombstones:0,protectedVersions:0,protectedTombstones:0,
    backlog:{raw_captures:0,uncurated_captures:0,replacement_proposals:0,pending_replacement_proposals:0,loop_proposals:0,pending_loop_proposals:0,
      protected_cross_area_relations:0,protected_relations_with_archived_endpoint:0,other_protected_relations:0}};
}
function route(destination) {
  if (['protected_archive','archive_and_rebuild','archive_and_retire'].includes(destination)) return {kind:destination,area:null};
  if (destination && typeof destination==='object' && !Array.isArray(destination) && Object.keys(destination).length===1) {
    if (AREAS.includes(destination.area)) return {kind:'exact_area_copy',area:destination.area};
    if (AREAS.includes(destination.archive_and_project_provenance)) return {kind:'archive_and_project_provenance',area:destination.archive_and_project_provenance};
  }
  fail();
}
export function accountOriginal(state,e) {
  const table=state.tables.get(e.record?.table);
  if(e.event!=='original_row'||!table||typeof e.record.key!=='string'||typeof e.row_json!=='string'||!sha.test(e.source_sha256)||hash(e.row_json)!==e.source_sha256)fail();
  const key=e.record.table+'\0'+e.record.key;
  if(state.lastRecord && Buffer.compare(Buffer.from(state.lastRecord),Buffer.from(key))>=0)fail();state.lastRecord=key;
  const r=route(e.destination);state.originalRows++;table.original_rows++;table.source_dispositions[r.kind]++;state.sourceDispositions[r.kind]++;
  const destinations=new Set();if(r.kind==='exact_area_copy')destinations.add(r.area);
  if(e.record.table==='public.projects' && state.projects.has(e.record.key)){
    state.seenProjects.add(e.record.key);for(const area of state.projects.get(e.record.key))destinations.add(area);
  }
  for(const area of destinations){state.areas.get(area).materialized_rows++;table.target_copy_rows[area]++;}
  if(r.kind==='archive_and_project_provenance'){
    const p=state.projections.get(e.record.key);
    if(e.record.table!=='public.memory_sources'||!p||p.area!==r.area||p.source_sha256!==e.source_sha256||state.seenProjections.has(e.record.key))fail();
    state.seenProjections.add(e.record.key);state.areas.get(r.area).projected_source_references++;state.areas.get(r.area).materialized_rows++;
  }
  // Only these small source fields are interpreted. No body, question, prompt,
  // arbitrary status, path, project name or review narrative enters the report.
  if(e.record.table==='public.memory_entries'){
    const v=JSON.parse(e.row_json),d=state.decisions.get(e.canonical_id);
    if(!d||v.canonical_id!==e.canonical_id||typeof v.is_tombstone!=='boolean'||!Number.isSafeInteger(v.version_no)||v.version_no<1)fail();
    const versions=state.lineageVersions.get(e.canonical_id)??new Set();if(versions.has(v.version_no))fail();versions.add(v.version_no);state.lineageVersions.set(e.canonical_id,versions);
    state.publicVersions++;if(v.is_tombstone)state.publicTombstones++;
    if(d.area){if(r.kind!=='exact_area_copy'||r.area!==d.area)fail();const a=state.areas.get(d.area);a.versions++;if(v.is_tombstone)a.tombstones++;}
    else {if(r.kind!=='protected_archive')fail();state.protectedVersions++;if(v.is_tombstone)state.protectedTombstones++;}
  } else if(e.record.table==='public.memory_canonical_state'){
    const d=state.decisions.get(e.canonical_id);if(!d||state.lineageStates.has(e.canonical_id))fail();state.lineageStates.add(e.canonical_id);
    if(d.area && (r.kind!=='exact_area_copy'||r.area!==d.area))fail();if(!d.area && r.kind!=='protected_archive')fail();
  } else if(e.record.table==='public.memory_relations'){
    const v=JSON.parse(e.row_json),left=state.decisions.get(v.src_canonical_id)?.area,right=state.decisions.get(v.dst_canonical_id)?.area;
    if(r.kind==='exact_area_copy'){if(left!==r.area||right!==r.area)fail();state.areas.get(r.area).relations++;}
    else if(r.kind==='protected_archive'){
      if(left&&right&&left!==right)state.backlog.protected_cross_area_relations++;
      else if(!left||!right)state.backlog.protected_relations_with_archived_endpoint++;
      else state.backlog.other_protected_relations++;
    } else fail();
  } else if(e.record.table==='public.raw_captures'){
    const v=JSON.parse(e.row_json);if(!Object.hasOwn(v,'curated_at')||r.kind!=='protected_archive')fail();state.backlog.raw_captures++;if(v.curated_at===null)state.backlog.uncurated_captures++;
  } else if(['public.memory_replacement_proposals','public.memory_proposals'].includes(e.record.table)){
    const v=JSON.parse(e.row_json);if(typeof v.status!=='string'||r.kind!=='protected_archive')fail();
    const loop=e.record.table==='public.memory_proposals';state.backlog[loop?'loop_proposals':'replacement_proposals']++;
    if(v.status==='pending')state.backlog[loop?'pending_loop_proposals':'pending_replacement_proposals']++;
  }
}
export function finishAccounting(state) {
  if(state.originalRows!==state.prepared.source_rows || Object.values(state.sourceDispositions).reduce((a,b)=>a+b,0)!==state.originalRows
    || state.seenProjects.size!==state.projects.size || state.seenProjections.size!==state.projections.size
    || state.lineageStates.size!==state.decisions.size || state.lineageVersions.size!==state.decisions.size)fail();
  for(const [area,a]of state.areas){const p=state.prepared.partitions.filter(p=>p.area===area);
    if(p.length!==1||a.materialized_rows!==p[0].rows+p[0].projections||a.projected_source_references!==p[0].projections)fail();}
  const protectedHistories=[...state.decisions.values()].filter(d=>!d.area).length;
  const actions=[
    {code:'review_uncurated_captures',count:state.backlog.uncurated_captures,action:'Review protected local raw captures in the source backlog; do not auto-curate or infer ownership.'},
    {code:'review_pending_replacement_proposals',count:state.backlog.pending_replacement_proposals,action:'Resolve with the original owner before selecting newer versions; no proposal was auto-approved.'},
    {code:'review_pending_loop_proposals',count:state.backlog.pending_loop_proposals,action:'Keep loop proposals archival until an explicit human workflow decides their ownership and disposition.'},
    {code:'review_protected_histories',count:protectedHistories,action:'Consult the private decision ledger. Test/evaluation material is preservation evidence, not a requirement to make it searchable.'},
    {code:'review_cross_area_relations',count:state.backlog.protected_cross_area_relations,action:'Original edges remain archival; any future exchange requires explicit federation policy, not copied relations.'},
    {code:'rebuild_derived_rows',count:state.sourceDispositions.archive_and_rebuild,action:'Derived source rows are archived. Review independent reindex/rebuild work before activation; do not restore background workers automatically.'},
    {code:'revalidate_source_delta',count:null,action:'The source remains writable. Create a fresh snapshot and revalidate all full-history/dependency hashes before any future cutover.'}
  ];
  return {source_rows:state.originalRows,source_tables:state.tables.size,every_original_row_accounted:true,
    source_dispositions:state.sourceDispositions,source_public_knowledge:{histories:state.decisions.size,versions:state.publicVersions,tombstones:state.publicTombstones},
    materialized_areas:Object.fromEntries(state.areas),protected_knowledge:{histories:protectedHistories,versions:state.protectedVersions,tombstones:state.protectedTombstones,by_review_classification:state.protectedClasses},
    protected_backlog:state.backlog,actions,table_accounting:[...state.tables.values()].sort((a,b)=>a.table.localeCompare(b.table)),
    limits:{all_original_rows_preserved_in_archive:true,original_evidence_searchable:false,raw_captures_searchable:false,
      proposal_approval_performed:false,database_import_verified:false,application_activation_verified:false,cutover_ready:false}};
}
async function bound(b,maxBytes,json){
  if(!b||!same(Object.keys(b).sort(),['path','sha256'])||!isAbsolute(b.path??'')||!sha.test(b.sha256))fail();
  const r=await privateInput(b.path,maxBytes,json);if(r.sha256!==b.sha256)fail();return r.value;
}
export async function accountArchive(binding,state){
  await bound(binding,Infinity,false);const input=await open(binding.path,constants.O_RDONLY|constants.O_NOFOLLOW),bytes=createHash('sha256');
  const framing=archiveFraming(state.tables.keys(),state.prepared.source_rows);
  let header=false,complete=false,size=0;
  try{
    async function* hashed(){for await(const chunk of input.createReadStream({autoClose:false})){bytes.update(chunk);size+=chunk.length;yield chunk;}}
    for await(const line of streamLines(hashed())){
      if(!line)fail();const e=JSON.parse(line);
      if(!header){if(e.event!=='header'||e.format_version!==1||e.serialization_id!=='postgresql-to-jsonb-utf8-utc-v1')fail();
        for(const k of ['source_instance','snapshot_id','inventory_sha256','manifest_sha256'])if(e[k]!==state.prepared[k])fail();header=true;
      }else if(complete)fail();else if(e.event==='complete'){
        framing.complete(e);complete=true;
      }else if(framing.consume(e))accountOriginal(state,e);
    }
    if(!complete||size!==state.prepared.archive_bytes||bytes.digest('hex')!==binding.sha256)fail();return finishAccounting(state);
  }finally{await input.close();}
}
export async function runAccounting(contractPath,contractSha,output){
  const c=await bound({path:contractPath,sha256:contractSha},64*1024,true);
  if(!same(Object.keys(c).sort(),['archive','kind','ledger','manifest','prepared','schema_version'])||c.schema_version!==1||c.kind!=='materialized_source_accounting')fail();
  if(new Set(['archive','ledger','manifest','prepared'].map(k=>c[k]?.path)).size!==4)fail();
  const prepared=await bound(c.prepared,1024*1024,true),manifest=await bound(c.manifest,256*1024*1024,true),ledger=await bound(c.ledger,64*1024*1024,true);
  if(prepared.manifest_file_sha256!==c.manifest.sha256||prepared.archive_sha256!==c.archive.sha256)fail();
  const report=await accountArchive(c.archive,accountingState(prepared,manifest,ledger));
  for(const b of [c.prepared,c.manifest,c.ledger,{path:contractPath,sha256:contractSha}])await bound(b,Infinity,false);
  Object.assign(report,{schema_version:1,status:'materialized_source_accounting_only',observed_at:new Date().toISOString(),contract_sha256:contractSha,
    source_instance:prepared.source_instance,snapshot_id:prepared.snapshot_id,inventory_sha256:prepared.inventory_sha256,manifest_sha256:prepared.manifest_sha256,
    prepared_sha256:c.prepared.sha256,manifest_file_sha256:c.manifest.sha256,ledger_sha256:c.ledger.sha256,archive_sha256:c.archive.sha256});
  return privateArtifact(output,async write=>{await write(JSON.stringify(report,null,2)+'\n');return {status:report.status,source_rows:report.source_rows,
    materialized_areas:report.materialized_areas,protected_knowledge:report.protected_knowledge,protected_backlog:report.protected_backlog,cutover_ready:false};});
}
export function parseArgs(args){
  const options={};for(let i=0;i<args.length;i+=2){if(!['--contract','--contract-sha256','--output'].includes(args[i])||!args[i+1]||options[args[i]])fail();options[args[i]]=args[i+1];}
  if(Object.keys(options).length!==3||!isAbsolute(options['--contract'])||!isAbsolute(options['--output'])||!sha.test(options['--contract-sha256']))fail();return options;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
  try{const a=parseArgs(process.argv.slice(2));console.log(JSON.stringify(await runAccounting(a['--contract'],a['--contract-sha256'],a['--output'])));}
  catch{console.error('Materialized source accounting failed. No private rows, paths or proposal bodies were disclosed.');process.exitCode=1;}
}
