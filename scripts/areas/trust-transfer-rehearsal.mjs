#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Local-only candidate proof. Never accepts a CA/live restore destination.
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { mkdir, readFile, writeFile, open, chmod } from 'node:fs/promises';
import { constants } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { privateInput, inspectContainer } from './restore-descriptor.mjs';
import { validateContract, validateTarget, verifyDatabase, stagingSession } from './verify-staging.mjs';
import { expandTrust, TRUST_SCHEMA } from './trust-expectation.mjs';
import { validateTrustDataToc } from './trust-transfer-toc.mjs';

const sha=/^[0-9a-f]{64}$/;
const IMAGE='sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180';
const TOOL='19d12e328a44b94654dc4136fe9e10158056bf4992c79b637eb5c0fd64870901';
const SOURCE='c6ce8327a1dea7f6988750cdc327ab40573938b696cf42ca358a068e9428d42e';
const clean={PATH:'/usr/bin:/bin',LC_ALL:'C',LANG:'C',TZ:'UTC'};
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
// Execute inside the same restore transaction before any pre-data statement.
// A new target must really be empty, not merely contain zero application rows.
export const FRESH_RESTORE_GUARD=`DO $memory_trust_guard$ BEGIN
IF current_database()<>'memory_staging_trust'
OR EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND backend_type='client backend')
OR EXISTS(SELECT 1 FROM pg_namespace WHERE nspname NOT IN ('public','information_schema') AND nspname !~ '^pg_')
OR EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_depend d JOIN pg_namespace n ON d.refobjid=n.oid WHERE d.refclassid='pg_namespace'::regclass AND n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_extension WHERE extname<>'plpgsql')
OR EXISTS(SELECT 1 FROM pg_event_trigger) OR EXISTS(SELECT 1 FROM pg_largeobject_metadata)
OR EXISTS(SELECT 1 FROM pg_foreign_server) OR EXISTS(SELECT 1 FROM pg_foreign_data_wrapper)
OR EXISTS(SELECT 1 FROM pg_default_acl) OR EXISTS(SELECT 1 FROM pg_publication)
OR EXISTS(SELECT 1 FROM pg_subscription WHERE subdbid=(SELECT oid FROM pg_database WHERE datname=current_database()))
THEN RAISE EXCEPTION 'Fresh isolated Trust restore target required'; END IF;
END $memory_trust_guard$;\n`;
function fail(){throw Error('Trust transfer rehearsal failed; candidate remains local and inactive; private diagnostics suppressed');}
function run(cmd,args,input,maxBuffer=32*1024*1024){
  const r=spawnSync(cmd,args,{input,env:clean,encoding:null,timeout:120_000,maxBuffer});
  if(r.status!==0)fail();return r.stdout;
}
const docker=(id,args,input)=>run('docker',['exec','-i',id,'env','-i','PATH=/usr/bin:/bin','LC_ALL=C','PGOPTIONS=-c timezone=UTC',...args],input);
const sql=(id,db,query)=>docker(id,['psql','-X','-qAt','-v','ON_ERROR_STOP=1','-U','postgres','-d',db],query).toString('utf8').trim();
async function artifact(path,bytes){await writeFile(path,bytes,{mode:0o600,flag:'wx'});return{path,sha256:hash(bytes),bytes:Buffer.byteLength(bytes)};}
async function json(path,value){return artifact(path,JSON.stringify(value,null,2)+'\n');}
async function bound(path,digest,limit,json=false){const b=await privateInput(path,limit,json);if(b.sha256!==digest)fail();return b.value;}
async function pinnedCopy(source,destination,digest,limit,executable=false){
  const f=await open(source,constants.O_RDONLY|constants.O_NOFOLLOW);
  try{const s=await f.stat();if(!s.isFile()||s.uid!==process.getuid()||(s.mode&0o002)!==0||s.size>limit)fail();
    const bytes=await f.readFile();if(hash(bytes)!==digest)fail();await artifact(destination,bytes);
    if(executable)await chmod(destination,0o700);
  }finally{await f.close();}return destination;
}

/** Exact compiled migration identity, independently recomputed from the frozen release source. */
export async function sourceSchema(sourceArchive,out){
  await bound(sourceArchive,SOURCE,512*1024*1024);
  const names=run('tar',['-tf',sourceArchive,'migrations']).toString('utf8').trim().split('\n');
  const files=names.filter(n=>n!=='migrations/').sort();
  if(files.length!==60||files.some((n,i)=>!new RegExp('^migrations/'+String(i+1).padStart(4,'0')+'_[a-z0-9_]+\\.sql$').test(n)))fail();
  await mkdir(join(out,'frozen-source'),{mode:0o700});
  run('tar',['-xf',sourceArchive,'--directory',join(out,'frozen-source'),'--no-same-owner','migrations']);
  const compatibility='\nCREATE EXTENSION IF NOT EXISTS vector;\n\nDROP INDEX IF EXISTS idx_memory_chunks_embedding_hnsw;\n\nALTER TABLE memory_chunks\n    DROP COLUMN IF EXISTS embedding;\n\nALTER TABLE memory_chunks\n    ADD COLUMN IF NOT EXISTS embedding vector;\n';
  const digest=createHash('sha256'),checksums=[];
  for(const [i,file]of files.entries()){
    const bytes=await readFile(join(out,'frozen-source',file)),sum=createHash('sha384').update(bytes).digest();
    const version=Buffer.alloc(8);version.writeBigInt64BE(BigInt(i+1));
    digest.update(version);digest.update(sum);digest.update(hash(i===3?compatibility:bytes));checksums.push(sum.toString('hex'));
  }
  if(digest.digest('hex')!==TRUST_SCHEMA)fail();return checksums;
}

export async function rehearse(o){
  for(const k of ['contract','expectation','schema-tool','source-archive','output'])if(!isAbsolute(o[k]??''))fail();
  for(const k of ['contract-sha256','expectation-sha256'])if(!sha.test(o[k]??''))fail();
  const c=validateContract(await bound(o.contract,o['contract-sha256'],64*1024,true));
  const v=await bound(o.expectation,o['expectation-sha256'],32*1024*1024,true),state=expandTrust(v);
  if(v.bindings.contract_sha256!==o['contract-sha256'])fail();
  const spec=c.targets.find(x=>x.area==='trust'),target=await bound(spec.target.path,spec.target.sha256,8192,true);
  if(target.instance_id!==v.target_instance||spec.operation_id!==v.operation_id||spec.image_id!==IMAGE)fail();
  validateTarget(spec,target,inspectContainer(spec.container_id),v.source_instance);
  await mkdir(o.output,{mode:0o700});
  const localTool=await pinnedCopy(o['schema-tool'],join(o.output,'schema-tool'),TOOL,32*1024*1024,true);
  const localSource=await pinnedCopy(o['source-archive'],join(o.output,'frozen-source.tar'),SOURCE,512*1024*1024);
  const plan=JSON.parse(run(localTool,['schema-plan']));if(plan.schema_sha256!==TRUST_SCHEMA||plan.migration_head!==60)fail();
  const checksums=await sourceSchema(localSource,o.output);
  const before=await verifyDatabase(stagingSession(spec.container_id,target.database),spec,target,state);
  const data=docker(spec.container_id,['pg_dump','-U','postgres','-d',target.database,'--data-only','--format=custom','--no-owner','--no-privileges']);
  const dataFile=await artifact(join(o.output,'trust-data.pgdump'),data);
  // Parsing occurs before any private dump contents can reach a restore executor.
  const toc=validateTrustDataToc(run('pg_restore',['--list'],data).toString('utf8'));
  await artifact(join(o.output,'data-selection.list'),toc.selection_list);
  const token=randomUUID(),name=`memory-four-area-staging-transfer-${token}-trust`,socket=join(o.output,'socket');
  await mkdir(socket,{mode:0o700});
  const id=run('docker',['run','--pull=never','--detach','--name',name,'--network','none','--read-only',
    '--mount',`type=bind,src=${socket},dst=/var/run/postgresql`,'--tmpfs','/var/lib/postgresql/data:rw,size=512m',
    '--tmpfs','/tmp:rw','--env','POSTGRES_HOST_AUTH_METHOD=trust',IMAGE]).toString('utf8').trim();
  if(!sha.test(id))fail();
  await json(join(o.output,'sandbox.json'),{id,name,image:IMAGE,kind:'disposable_verification_only',activation_authorized:false});
  let passed=false,phase='sandbox-startup';
  try{
    let ready=false;
    for(let i=0;i<100;i++){
      const r=spawnSync('docker',['exec',id,'cat','/proc/1/comm'],{env:clean,encoding:'utf8',timeout:5000});
      const health=spawnSync('docker',['exec',id,'pg_isready','-U','postgres'],{env:clean,stdio:'ignore',timeout:5000});
      if(r.status===0&&r.stdout.trim()==='postgres'&&health.status===0){ready=true;break;}await delay(100);
    }
    if(!ready)fail();
    phase='empty-database-creation';
    for(const db of ['memory_reference_trust','memory_staging_trust'])docker(id,['createdb','-U','postgres',db]);
    phase='reference-schema-initialization';
    const referenceTarget={...target,database:'memory_reference_trust',connection:{kind:'unix',directory:socket,port:5432}};
    const descriptor=await json(join(o.output,'reference-target.json'),referenceTarget);
    const schemaResult=JSON.parse(run(localTool,['initialize-schema','--target',descriptor.path,'--target-sha256',descriptor.sha256,
      '--schema-sha256',TRUST_SCHEMA,'--execute','--writers-stopped','--fresh-database']));
    if(schemaResult.status!=='fresh_schema_initialized_only_not_imported_or_activated')fail();
    await json(join(o.output,'schema-initialization.json'),schemaResult);
    phase='trusted-schema-export';
    const schema=docker(id,['pg_dump','-U','postgres','-d','memory_reference_trust','--schema-only','--format=custom','--no-owner','--no-privileges']);
    const schemaFile=await artifact(join(o.output,'trusted-schema.pgdump'),schema);
    const pre=run('pg_restore',['--section=pre-data','--no-owner','--no-privileges','--file=-'],schema);
    const middle=run('pg_restore',['--data-only','--no-owner','--no-privileges','--file=-','--use-list='+join(o.output,'data-selection.list')],data);
    const post=run('pg_restore',['--section=post-data','--no-owner','--no-privileges','--file=-'],schema);
    // Every schema statement comes from the independently initialized reference.
    // One psql transaction: no --clean, trigger disabling or service bootstrap.
    const statements=Buffer.concat([Buffer.from("SET statement_timeout='120s'; SET lock_timeout='5s';\n"+FRESH_RESTORE_GUARD),pre,middle,post]);
    phase='atomic-clone-restore';
    docker(id,['psql','-X','-q','-1','-v','ON_ERROR_STOP=1','-U','postgres','-d','memory_staging_trust'],statements);
    phase='independent-clone-verification';
    const clone={...target,connection:{kind:'unix',directory:socket,port:5432}};
    const cloneSpec={...spec,container_id:id,container_name:name};
    validateTarget(cloneSpec,clone,inspectContainer(id),v.source_instance);
    const result=await verifyDatabase(stagingSession(id,clone.database),cloneSpec,clone,state);
    const migrated=sql(id,clone.database,"BEGIN READ ONLY; SELECT encode(checksum,'hex') FROM public._sqlx_migrations ORDER BY version; COMMIT;").split('\n');
    if(JSON.stringify(migrated)!==JSON.stringify(checksums))fail();
    const version=sql(id,clone.database,"SHOW server_version;"),extensions=JSON.parse(sql(id,clone.database,
      "SELECT json_agg(json_build_object('name',extname,'version',extversion) ORDER BY extname) FROM pg_extension;"));
    phase='original-staging-reverification';
    const after=await verifyDatabase(stagingSession(spec.container_id,target.database),spec,target,state);
    validateTarget(spec,target,inspectContainer(spec.container_id),v.source_instance);
    await bound(o.expectation,o['expectation-sha256'],32*1024*1024);await bound(o.contract,o['contract-sha256'],64*1024);
    if(before.verified_rows!==after.verified_rows)fail();
    await json(join(o.output,'restore-result.json'),{format:'memory-trust-transfer-rehearsal-v1',status:'local_trust_restore_verified_not_deliverable',
      observed_at:new Date().toISOString(),expectation_sha256:o['expectation-sha256'],source_snapshot:v.snapshot_id,
      schema_tool_sha256:TOOL,source_archive_sha256:SOURCE,required_schema_sha256:TRUST_SCHEMA,
      source_ddl_executed:false,data:dataFile,schema:schemaFile,toc_tables:toc.table_count,toc_sequences:toc.sequence_count,
      postgres_version:version,extensions,result,migration_checksums_match:true,original_staging_reverified:true,
      encrypted:false,ca_compatibility_verified:false,data_transfer_authorized:false,activation_authorized:false});
    passed=true;return{status:'local_trust_restore_verified_not_deliverable',histories:result.histories,versions:result.versions,
      rows:result.verified_rows,projected_sources:result.projected_sources,source_ddl_executed:false,activation_authorized:false};
  }catch(error){
    await json(join(o.output,'failure.json'),{status:'failed_not_deliverable',phase,observed_at:new Date().toISOString(),private_diagnostics_included:false});
    throw error;
  }finally{
    // Destroy only this newly created ephemeral verification container, retaining
    // evidence and protected candidate files. No caller-supplied deletion target.
    const r=spawnSync('docker',['rm','--force',id],{env:clean,encoding:'utf8',timeout:30_000});
    await json(join(o.output,'cleanup.json'),{container_id:id,removed:r.status===0,verification_passed:passed});
    if(r.status!==0)fail();
  }
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
  try{const a=process.argv.slice(2),o={};for(let i=0;i<a.length;i+=2){const k=a[i].slice(2);
    if(!['contract','contract-sha256','expectation','expectation-sha256','schema-tool','source-archive','output'].includes(k)||a[i]!==`--${k}`||!a[i+1]||o[k])fail();o[k]=a[i+1];}
    if(Object.keys(o).length!==7)fail();console.log(JSON.stringify(await rehearse(o)));
  }catch{console.error('Trust transfer rehearsal failed; keep candidate protected and inactive. No private diagnostics disclosed.');process.exitCode=1;}
}
