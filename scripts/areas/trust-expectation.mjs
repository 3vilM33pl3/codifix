#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Read-only verification expectations, NOT a portable import/deserialization API.
import { isAbsolute, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { privateInput, inspectContainer } from './restore-descriptor.mjs';
import { privateArtifact } from './inventory.mjs';
import { validateContract, validateTarget, makeExpected, readArchive, verifyDatabase, stagingSession } from './verify-staging.mjs';

export const TRUST_SCHEMA = '2ce8b07f9c8fc252b0162d158ccb8e57082c2186306cdc489c9343ec91a115dc';
const sha = /^[0-9a-f]{64}$/;
const uuid = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const isSha = value => typeof value === 'string' && sha.test(value);
const isUuid = value => typeof value === 'string' && uuid.test(value);
const rowTables = new Set(['public.projects','public.memory_entries','public.memory_canonical_state',
  'public.memory_tags','public.memory_relations','public.memory_sources']);
const provenanceFields = ['source_id','decision_id','decision_sha256','canonical_id','memory_version_id',
  'source_sha256','original_version_sha256','archived_task_id','archived_task_sha256','target_sha256'];
function fail() { throw Error('Trust expectation rejected; no private contents or database diagnostics disclosed'); }
function exact(v, fields) {
  if (!v || typeof v !== 'object' || Array.isArray(v) || Object.keys(v).length !== fields.length
    || fields.some(k=>!Object.hasOwn(v,k))) fail();
}
function sorted(values) { return [...values].sort((a,b)=>Buffer.compare(Buffer.from(a),Buffer.from(b))); }
function ids(values) {
  if (!Array.isArray(values) || values.length>100_000 || values.some(v=>!isUuid(v))
    || JSON.stringify(sorted(new Set(values)))!==JSON.stringify(values)) fail();
  return new Set(values);
}

function validRowKey(table, key) {
  if (table !== 'public.memory_tags') return isUuid(key);
  let pair;
  try { pair = JSON.parse(key); } catch { return false; }
  // This schema's only composite key is (memory_entry_id UUID, tag TEXT).
  // Match PostgreSQL JSONB's array-of-strings spacing without accepting a
  // second textual encoding of the same logical key. Tag text remains private.
  return Array.isArray(pair) && pair.length === 2 && isUuid(pair[0])
    && typeof pair[1] === 'string' && !pair[1].includes('\0')
    && Buffer.from(pair[1], 'utf8').toString('utf8') === pair[1]
    && key === `[${JSON.stringify(pair[0])}, ${JSON.stringify(pair[1])}]`;
}

/**
 * Minimize already archive-derived expectations; no source row bodies remain.
 * This is still a PRIVATE artifact: composite memory_tags keys contain tag text.
 * Counts/history/tombstone claims come from the verified full archive; the
 * minimal artifact cannot independently reconstruct those original row fields.
 */
export function minimizeTrust(state, spec, target, bindings) {
  if (spec.area!=='trust' || target.area!=='trust') fail();
  const e=state.expected.get('trust'), p=state.prepared;
  const value={format:'memory-trust-expectation-v1',area:'trust',required_schema_sha256:TRUST_SCHEMA,
    source_instance:p.source_instance,snapshot_id:p.snapshot_id,target_instance:target.instance_id,
    operation_id:spec.operation_id,source_migration:state.sourceMigration,target_migration:60,
    inventory_sha256:p.inventory_sha256,manifest_sha256:p.manifest_sha256,
    partition_sha256:p.partitions.find(x=>x.area==='trust').partition_sha256,
    bindings:{...bindings},histories:sorted(e.histories),projects:sorted(e.projects),versions:e.versions,tombstones:e.tombstones,
    rows:sorted(e.rows.keys()).map(key=>({key,...e.rows.get(key)})),
    provenance:sorted(e.provenance.keys()).map(key=>e.provenance.get(key)),
    evidence_mode:'archive_reference_only',activation_authorized:false,current_source_verified:false};
  expandTrust(value); return value;
}

/**
 * Reconstruct only read-only verifier expectations. NEVER an importer input.
 * Before decoding, the consumer MUST authenticate the exact private artifact
 * bytes against an independently trusted digest/transfer binding. Self-reported
 * hashes, structural validity or a successful decode do not supply authenticity.
 *
 * In particular, counts/tombstones/history membership are archive-derived claims,
 * not independently re-proved from the hashes here. required_schema_sha256 names
 * the required release schema; it is not an observation of target functions,
 * triggers or migration checksums. Those need separate schema verification.
 */
export function expandTrust(v) {
  exact(v,['format','area','required_schema_sha256','source_instance','snapshot_id','target_instance','operation_id',
    'source_migration','target_migration','inventory_sha256','manifest_sha256','partition_sha256','bindings',
    'histories','projects','versions','tombstones','rows','provenance','evidence_mode','activation_authorized','current_source_verified']);
  if (v.format!=='memory-trust-expectation-v1' || v.area!=='trust' || v.required_schema_sha256!==TRUST_SCHEMA
    || v.target_migration!==60 || !Number.isSafeInteger(v.source_migration) || v.source_migration<52 || v.source_migration>60
    || v.evidence_mode!=='archive_reference_only' || v.activation_authorized!==false || v.current_source_verified!==false
    || v.source_instance===v.target_instance) fail();
  for(const k of ['source_instance','snapshot_id','target_instance','operation_id']) if(!isUuid(v[k])) fail();
  for(const k of ['inventory_sha256','manifest_sha256','partition_sha256']) if(!isSha(v[k])) fail();
  exact(v.bindings,['contract_sha256','prepared_file_sha256','manifest_file_sha256','archive_sha256']);
  if(Object.values(v.bindings).some(x=>!isSha(x))) fail();
  const histories=ids(v.histories),projects=ids(v.projects),rows=new Map(),provenance=new Map();
  if(!Array.isArray(v.rows)||v.rows.length>100_000||!Array.isArray(v.provenance)||v.provenance.length>100_000
    ||!Number.isSafeInteger(v.versions)||v.versions<0||!Number.isSafeInteger(v.tombstones)||v.tombstones<0||v.tombstones>v.versions) fail();
  let previous='';
  for(const r of v.rows) {
    exact(r,['key','source','target','transformed']);
    if(typeof r.key!=='string'||Buffer.byteLength(r.key)>8192||r.key.split('\0').length!==2
      ||Buffer.compare(Buffer.from(previous),Buffer.from(r.key))>=0||!isSha(r.source)||!isSha(r.target)) fail();
    const [table,key]=r.key.split('\0');
    if(!rowTables.has(table)||!validRowKey(table,key)||r.transformed!==(table==='public.memory_sources')
      ||(!r.transformed&&r.source!==r.target)) fail();
    rows.set(r.key,{source:r.source,target:r.target,transformed:r.transformed}); previous=r.key;
  }
  previous='';
  const decisionIds = new Set();
  for(const p of v.provenance) {
    exact(p,provenanceFields);
    for(const k of provenanceFields) if(!(k.endsWith('sha256')?isSha:isUuid)(p[k])) fail();
    const row=rows.get('public.memory_sources\0'+p.source_id);
    const version=rows.get('public.memory_entries\0'+p.memory_version_id);
    if(previous>=p.source_id||!row||row.source!==p.source_sha256||row.target!==p.target_sha256
      ||!histories.has(p.canonical_id)||!version||version.source!==p.original_version_sha256
      ||decisionIds.has(p.decision_id)) fail();
    decisionIds.add(p.decision_id);provenance.set(p.source_id,p);previous=p.source_id;
  }
  const count=table=>[...rows.keys()].filter(k=>k.startsWith(table+'\0')).length;
  if(count('public.memory_sources')!==provenance.size||count('public.memory_entries')!==v.versions
    ||count('public.memory_canonical_state')!==histories.size||count('public.projects')!==projects.size
    ||[...histories].some(id=>!rows.has('public.memory_canonical_state\0'+id))
    ||[...projects].some(id=>!rows.has('public.projects\0'+id))) fail();
  for(const key of rows.keys())if(key.startsWith('public.memory_tags\0')){
    const versionId=JSON.parse(key.split('\0')[1])[0];
    if(!rows.has('public.memory_entries\0'+versionId))fail();
  }
  return {sourceMigration:v.source_migration,prepared:{source_instance:v.source_instance,snapshot_id:v.snapshot_id,
    inventory_sha256:v.inventory_sha256,manifest_sha256:v.manifest_sha256,
    partitions:[{area:'trust',partition_sha256:v.partition_sha256,rows:rows.size-provenance.size,projections:provenance.size}]},
    expected:new Map([['trust',{histories,projects,rows,provenance,versions:v.versions,tombstones:v.tombstones}]])};
}

async function bound(binding,limit,json=true) {
  const value=await privateInput(binding.path,limit,json);if(value.sha256!==binding.sha256)fail();return value.value;
}

export async function prepareExpectation(contractFile,contractSha,output) {
  if(!isSha(contractSha)||typeof contractFile!=='string'||!isAbsolute(contractFile)
    ||typeof output!=='string'||!isAbsolute(output))fail();
  const c=validateContract(await bound({path:contractFile,sha256:contractSha},64*1024));
  const p=await bound(c.prepared,1024*1024),m=await bound(c.manifest,256*1024*1024);
  if(p.manifest_file_sha256!==c.manifest.sha256||p.archive_sha256!==c.archive.sha256)fail();
  const state=makeExpected(p,m),spec=c.targets.find(t=>t.area==='trust');
  const target=await bound(spec.target,8192);
  validateTarget(spec,target,inspectContainer(spec.container_id),p.source_instance);
  await readArchive(c.archive,state);
  const verified=await verifyDatabase(stagingSession(spec.container_id,target.database),spec,target,state);
  validateTarget(spec,target,inspectContainer(spec.container_id),p.source_instance);
  for(const b of [c.prepared,c.manifest,spec.target,{path:contractFile,sha256:contractSha}])await bound(b,Infinity,false);
  const value=minimizeTrust(state,spec,target,{contract_sha256:contractSha,prepared_file_sha256:c.prepared.sha256,
    manifest_file_sha256:c.manifest.sha256,archive_sha256:c.archive.sha256});
  return privateArtifact(output,async write=>{await write(JSON.stringify(value,null,2)+'\n');
    return {status:'trust_expectations_verified_not_transferred',histories:verified.histories,versions:verified.versions,
      verified_rows:verified.verified_rows,projected_sources:verified.projected_sources,activation_authorized:false};});
}

if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const a=process.argv.slice(2),o={};
    for(let i=0;i<a.length;i+=2){if(!['--contract','--contract-sha256','--output'].includes(a[i])||!a[i+1]||o[a[i]])fail();o[a[i]]=a[i+1];}
    if(Object.keys(o).length!==3)fail();
    console.log(JSON.stringify(await prepareExpectation(o['--contract'],o['--contract-sha256'],o['--output'])));
  }catch{console.error('Trust expectation preparation failed. Keep the full archive local and the Trust target inactive. No private diagnostics disclosed.');process.exitCode=1;}
}
