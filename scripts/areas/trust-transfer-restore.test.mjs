// SPDX-License-Identifier: AGPL-3.0-or-later
// Opt-in synthetic PostgreSQL evidence, never a CA/live restore executor.
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { SCHEMA60_TABLES, SCHEMA60_SEQUENCES } from './verify-staging.mjs';
import { validateTrustDataToc } from './trust-transfer-toc.mjs';
import { TRUST_SCHEMA } from './trust-expectation.mjs';
import { FRESH_RESTORE_GUARD } from './trust-transfer-rehearsal.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const id = n => `00000000-0000-0000-0000-${String(n).padStart(12,'0')}`;
const clean = { PATH:'/usr/bin:/bin', LC_ALL:'C', LANG:'C', TZ:'UTC' };

test('synthetic Trust restore uses trusted schema, exact data TOC and atomic fresh-target guard', {
  skip:process.env.CODIFIX_AREA_TEST_DOCKER!=='1', timeout:240_000,
}, async t => {
  const root=await mkdtemp(join(tmpdir(),'memory-trust-transfer-test-'));
  const socket=join(root,'socket');await mkdir(socket,{mode:0o777});
  const run=(command,args,options={})=>{
    const result=spawnSync(command,args,{env:clean,timeout:120_000,maxBuffer:32*1024*1024,...options});
    if(result.status!==0)throw Error(`Synthetic Trust ${command} operation failed; diagnostics suppressed`);
    return result.stdout;
  };
  const image=run('docker',['image','inspect','pgvector/pgvector:pg17','--format','{{.Id}}']).toString().trim();
  assert.match(image,/^sha256:[0-9a-f]{64}$/);
  const name=`memory-four-area-staging-transfer-test-${randomUUID()}-trust`;
  const container=run('docker',['run','--pull=never','--detach','--name',name,'--network','none','--read-only',
    '--memory','1g','--cpus','2','--tmpfs','/var/lib/postgresql/data:rw,size=512m','--tmpfs','/tmp:rw',
    '--mount',`type=bind,source=${socket},target=/var/run/postgresql`,
    '--env','POSTGRES_HOST_AUTH_METHOD=trust',image]).toString().trim();
  assert.match(container,/^[0-9a-f]{64}$/);
  t.after(async()=>{
    spawnSync('docker',['exec',container,'chmod','0777','/var/run/postgresql'],{env:clean,timeout:10_000,stdio:'ignore'});
    run('docker',['rm','--force',container]);
    const remaining=run('docker',['ps','--all','--filter',`id=${container}`,'--format','{{.ID}}']).toString().trim();
    assert.equal(remaining,'','Only this generated synthetic container must be removed');
    await rm(root,{recursive:true,force:true});
    t.diagnostic(`cleanup_passed=true container=${container}; generated fixture directory removed`);
  });
  const docker=(args,input)=>run('docker',['exec','-i',container,'env','-i','PATH=/usr/bin:/bin','LC_ALL=C',
    'PGOPTIONS=-c timezone=UTC',...args],input===undefined?{}:{input});
  const psqlArgs=db=>['psql','-X','-qAt','-v','ON_ERROR_STOP=1','-U','postgres','-d',db];
  const sql=(db,query)=>docker(psqlArgs(db),query).toString().trim();
  let ready=false;
  for(let attempt=0;attempt<100;attempt++){
    const main=spawnSync('docker',['exec',container,'cat','/proc/1/comm'],{env:clean,encoding:'utf8',timeout:5000});
    if(main.stdout?.trim()==='postgres'&&spawnSync('docker',['exec',container,'pg_isready','-U','postgres'],
      {env:clean,timeout:5000,stdio:'ignore'}).status===0){ready=true;break;}
    await delay(100);
  }
  assert.equal(ready,true);
  const inspection=JSON.parse(run('docker',['inspect','--format','{{json .}}',container]));
  assert.equal(inspection.HostConfig.NetworkMode,'none');assert.equal(inspection.HostConfig.ReadonlyRootfs,true);
  assert.equal(inspection.HostConfig.Privileged,false);assert.deepEqual(inspection.HostConfig.PortBindings,{});
  assert.equal(inspection.Image,image);
  const source='memory_transfer_source_fixture', other='memory_transfer_other_fixture', target='memory_staging_trust';
  for(const database of [source,other,target])docker(['createdb','-U','postgres',database]);
  const targetFile=join(root,'source-target.json');
  const targetBytes=JSON.stringify({format_version:1,instance_id:id(90),area:'trust',
    connection:{kind:'unix',directory:socket,port:5432},database:source,username:'postgres',password:null});
  await writeFile(targetFile,targetBytes,{mode:0o600,flag:'wx'});
  const binary=process.env.CODIFIX_AREA_IMPORT_BINARY??new URL('../../target/debug/codifix-area-import',import.meta.url).pathname;
  const plan=JSON.parse(run(binary,['schema-plan']));
  assert.equal(plan.schema_sha256,TRUST_SCHEMA);assert.equal(plan.migration_head,60);
  run(binary,['initialize-schema','--target',targetFile,'--target-sha256',hash(targetBytes),
    '--schema-sha256',TRUST_SCHEMA,'--execute','--writers-stopped','--fresh-database']);
  const dump=options=>docker(['pg_dump','-U','postgres','-d',source,'--format=custom','--no-owner','--no-privileges',...options]);
  // Capture known migration-only DDL BEFORE the source-only poison function.
  const trustedSchema=dump(['--schema-only']);
  const canary='OTHER_AREA_PRIVATE_CANARY_5e09fa';
  sql(other,`CREATE TABLE public.other_area_canary(payload text); INSERT INTO public.other_area_canary VALUES('${canary}');`);
  sql(source,`INSERT INTO projects(id,slug,name,root_path,created_at)
    VALUES('${id(1)}','trust-fixture','Synthetic Trust','/synthetic/trust','2026-09-22T12:00:00Z');
    INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,
      importance,confidence,created_at,updated_at,search_document)
    VALUES('${id(3)}','${id(1)}','${id(2)}',1,false,'Synthetic retained Trust version','Synthetic summary','documentation','project',
      3,0.75,'2026-09-22T12:00:00Z','2026-09-22T12:00:00Z',to_tsvector('simple','synthetic trust')),
      ('${id(4)}','${id(1)}','${id(2)}',2,true,'','','documentation','project',
      3,0.75,'2026-09-22T12:01:00Z','2026-09-22T12:01:00Z',to_tsvector('simple',''));
    UPDATE memory_canonical_state SET status='archived',archived_at='2026-09-22T12:02:00Z',
      confidence=0.25,updated_at='2026-09-22T12:02:00Z' WHERE canonical_id='${id(2)}';
    INSERT INTO memory_tags(memory_entry_id,tag) VALUES('${id(3)}','synthetic private tag');
    UPDATE loop_global_state SET kill_switch_enabled=true;
    CREATE FUNCTION public.source_only_poison() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN RAISE EXCEPTION 'SOURCE_ONLY_DDL_EXECUTED'; END $$;
    CREATE TRIGGER source_only_poison AFTER INSERT ON memory_entries FOR EACH ROW EXECUTE FUNCTION public.source_only_poison();`);
  assert.equal(sql(source,"SELECT count(*) FROM pg_proc WHERE proname='source_only_poison';"),'1');
  const data=dump(['--data-only']);
  const list=run('pg_restore',['--list'],{input:data}).toString();
  const toc=validateTrustDataToc(list);
  assert.equal(toc.table_count,114);assert.equal(toc.sequence_count,4);
  t.diagnostic(`actual_pg17_toc_tables=${toc.table_count} sequences=${toc.sequence_count}; server=${sql(source,'SHOW server_version;')}`);
  const selection=join(root,'data-selection.list');await writeFile(selection,toc.selection_list,{mode:0o600,flag:'wx'});
  const pre=run('pg_restore',['--section=pre-data','--no-owner','--no-privileges','--file=-'],{input:trustedSchema});
  const middle=run('pg_restore',['--data-only','--no-owner','--no-privileges','--file=-',`--use-list=${selection}`],{input:data});
  const post=run('pg_restore',['--section=post-data','--no-owner','--no-privileges','--file=-'],{input:trustedSchema});
  assert.ok(!middle.includes(Buffer.from(canary)),'Another database must not enter the Trust data archive');
  assert.ok(!Buffer.concat([pre,middle,post]).includes(Buffer.from('source_only_poison')));
  const sourceFullToc=run('pg_restore',['--list'],{input:dump([])}).toString();
  assert.match(sourceFullToc,/source_only_poison/);assert.throws(()=>validateTrustDataToc(sourceFullToc));

  const tableStateSql=SCHEMA60_TABLES.map(table=>`SELECT json_build_object('table','${table}',
    'count',count(*),'sha256',encode(public.digest(COALESCE(string_agg(to_jsonb(r)::text,E'\\n' ORDER BY to_jsonb(r)::text),''),'sha256'),'hex'))
    FROM ONLY ${table} r;`).join('\n');
  const sequenceStateSql=SCHEMA60_SEQUENCES.map(sequence=>`SELECT json_build_object('sequence','${sequence.name}',
    'last_value',last_value::text,'is_called',is_called) FROM ${sequence.name};`).join('\n');
  const state=db=>sql(db,"BEGIN READ ONLY; SET LOCAL timezone='UTC'; SET LOCAL extra_float_digits=3;\n"
    +tableStateSql+'\n'+sequenceStateSql+'\nCOMMIT;');
  const sourceBefore=state(source);
  const statements=Buffer.concat([Buffer.from("SET statement_timeout='120s'; SET lock_timeout='5s';\n"+FRESH_RESTORE_GUARD),pre,middle,post]);
  const restoreResult=input=>spawnSync('docker',['exec','-i',container,'env','-i','PATH=/usr/bin:/bin','LC_ALL=C',
    'PGOPTIONS=-c timezone=UTC',...psqlArgs(target),'-1'],{input,env:clean,timeout:120_000,maxBuffer:8*1024*1024});

  // Inject the fault only after all schema/data/post-data commands have run,
  // before psql's implicit COMMIT. Fresh schema/extension objects must roll back.
  const interrupted=restoreResult(Buffer.concat([statements,Buffer.from("\nDO $$ BEGIN RAISE EXCEPTION 'SYNTHETIC_PRECOMMIT_FAILURE'; END $$;\n")]));
  assert.notEqual(interrupted.status,0);assert.match(interrupted.stderr.toString(),/SYNTHETIC_PRECOMMIT_FAILURE/);
  sql(target,FRESH_RESTORE_GUARD);
  assert.equal(sql(target,"SELECT count(*) FROM pg_extension WHERE extname<>'plpgsql';"),'0');
  assert.equal(sql(target,"SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public';"),'0');

  const restored=restoreResult(statements);
  assert.equal(restored.status,0,'Trusted pre-data/data/post-data must restore without canonical or seed collisions');
  assert.equal(state(target),sourceBefore,'All 114 logical table hashes and all four sequence states must match');
  assert.equal(state(source),sourceBefore,'Read-only export must not alter source rows or sequences');
  assert.equal(sql(target,"SELECT count(*) FROM pg_proc WHERE proname='source_only_poison';"),'0');
  assert.equal(sql(target,"SELECT count(*) FROM pg_trigger WHERE tgname='source_only_poison';"),'0');
  assert.equal(sql(target,"SELECT count(*) FROM pg_trigger WHERE tgname='memory_entries_sync_state';"),'1');
  assert.equal(sql(target,"SELECT count(*) FROM memory_entries;"),'2');
  assert.equal(sql(target,"SELECT count(*) FROM memory_canonical_state;"),'1');
  assert.equal(sql(target,"SELECT status||':'||confidence::text FROM memory_canonical_state;"),'archived:0.25');
  assert.equal(sql(target,"SELECT count(*) FROM pg_index WHERE NOT indisvalid; SELECT count(*) FROM pg_constraint WHERE NOT convalidated;"),'0\n0');
  assert.equal(sql(target,"SELECT to_regclass('public.other_area_canary') IS NULL;"),'t');
  assert.equal(sql(other,"SELECT payload FROM public.other_area_canary;"),canary);

  // The actual shared restore guard rejects reuse before any further DDL/data.
  const beforeRejected=state(target),rejected=restoreResult(statements);
  assert.notEqual(rejected.status,0);assert.match(rejected.stderr.toString(),/Fresh isolated Trust restore target required/);
  assert.equal(state(target),beforeRejected,'Rejected nonempty restore must leave every row and sequence unchanged');
  t.diagnostic('source_only_ddl_executed=false; canonical_hashes_preserved=true; rollback_before_commit=true; nonempty_target_denied=true');
});
