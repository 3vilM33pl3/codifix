// Operator-only continuation of the explicitly approved local Workshop preview.
// Fixed resources, no discovery, new credentials, provider calls or other areas.
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const root='/home/olivier/.local/state/memory-workshop-preview-20260918';
const repository='/home/olivier/Projects/memory';
const built='/data/tmp/memory-four-area-target/debug';
const app='memory-workshop-preview-app-20260918';
const phase=process.argv[2];
if(!['preserve','upgrade','start'].includes(phase)||process.argv.length!==3)throw Error('Choose preserve, upgrade or start explicitly');
const d=JSON.parse(fs.readFileSync(`${root}/deployment.json`));
if(d.database!=='memory_workshop_preview_20260918'||d.instance_id!=='72d1a39c-29fe-4f8d-94ee-b3a8dba51f85')throw Error('Wrong preview identity');
const backup=`${root}/before-interaction-59`;
const retained='memory-workshop-preview-app-before-interaction-59';
const digest=b=>createHash('sha256').update(b).digest('hex');
function run(program,args,options={}){
  const result=spawnSync(program,args,{maxBuffer:32*1024*1024,...options});
  if(result.status!==0)throw Error(`${program.split('/').pop()} failed; private diagnostics suppressed. Inspect before retrying.`);
  return result.stdout;
}
if(phase==='preserve'){
  if(fs.existsSync(backup))throw Error('Backup already exists; inspect, do not overwrite');
  run('/usr/bin/docker',['inspect',app]);
  fs.mkdirSync(backup,{mode:0o700});
  run('/usr/bin/docker',['stop',app]);
  try {
    const dump=run('/usr/bin/docker',['exec',d.container,'pg_dump','-U','postgres','-d',d.database,'-Fc']);
    fs.writeFileSync(`${backup}/database.dump`,dump,{mode:0o600,flag:'wx'});
    for(const name of ['app.toml','client.toml','deployment.json','bootstrap.json','work.json','artifact-hashes.json'])fs.copyFileSync(`${root}/${name}`,`${backup}/${name}`,fs.constants.COPYFILE_EXCL);
    const tar=run('/usr/bin/tar',['-C',backup,'-cf','-','.']);
    const encrypted=run('/usr/bin/gpg',['--batch','--trust-model','always','--encrypt','--recipient','97D83F9D129AB85B38907FF77AEF04712CEA51D2'],{input:tar});
    fs.writeFileSync(`${backup}.tar.gpg`,encrypted,{mode:0o600,flag:'wx'});
    const offsite='/media/bunnychow/backups/memory-four-area-20260917/workshop-preview-20260918/before-interaction-59.tar.gpg';
    fs.copyFileSync(`${backup}.tar.gpg`,offsite,fs.constants.COPYFILE_EXCL);
    if(digest(fs.readFileSync(offsite))!==digest(encrypted))throw Error('Backup copy mismatch');
    const receipt={sha256:digest(encrypted),bytes:encrypted.length,copy_verified:true,restore_rehearsed:false,includes_provider_key:false};
    fs.writeFileSync(`${backup}/receipt.json`,JSON.stringify(receipt),{mode:0o600,flag:'wx'});
    console.log(JSON.stringify(receipt));
  }catch(error){run('/usr/bin/docker',['start',app]);throw error;}
}
if(phase==='upgrade'){
  const receipt=JSON.parse(fs.readFileSync(`${backup}/receipt.json`));
  if(!receipt.copy_verified||digest(fs.readFileSync(`${backup}.tar.gpg`))!==receipt.sha256)throw Error('Backup not verified');
  const running=JSON.parse(run('/usr/bin/docker',['inspect',app],{encoding:'utf8'}))[0].State.Running;
  if(running)throw Error('Stop the exact preview before migrating');
  const output=run(`${built}/examples/upgrade_workshop_preview`,['--writers-stopped-backup-verified'],{env:{PATH:'/usr/bin:/bin',DATABASE_URL:`postgresql://postgres:${d.admin_password}@127.0.0.1:55460/${d.database}`}});
  const migration=JSON.parse(output);if(migration.migration_head!==59)throw Error('Unexpected schema');
  fs.writeFileSync(`${backup}/upgrade.json`,output,{mode:0o600,flag:'wx'});
  const staging=`${root}/interaction-59`;fs.mkdirSync(staging,{mode:0o700});
  fs.copyFileSync(`${root}/work.json`,`${staging}/work.json`,fs.constants.COPYFILE_EXCL);
  fs.copyFileSync(`${built}/memory`,`${staging}/memory`,fs.constants.COPYFILE_EXCL);
  fs.chmodSync(`${staging}/memory`,0o500);
  fs.cpSync(`${repository}/web/dist`,`${staging}/web`,{recursive:true,errorOnExist:true,force:false});
  fs.writeFileSync(`${staging}/artifact.json`,JSON.stringify({memory_sha256:digest(fs.readFileSync(`${staging}/memory`)),manifest_sha256:digest(fs.readFileSync(`${staging}/work.json`))}),{mode:0o600,flag:'wx'});
  console.log(JSON.stringify(migration));
}
if(phase==='start'){
  const staging=`${root}/interaction-59`;
  const hashes=JSON.parse(fs.readFileSync(`${staging}/artifact.json`));
  if(digest(fs.readFileSync(`${staging}/memory`))!==hashes.memory_sha256||digest(fs.readFileSync(`${staging}/work.json`))!==hashes.manifest_sha256)throw Error('Staged artifact changed');
  const config=fs.readFileSync(`${root}/app.toml`,'utf8');
  if(!config.includes('[supervisor.interaction]')||!config.includes('/interaction/work.json')||!config.includes(hashes.manifest_sha256))throw Error('Explicit interaction config is not installed');
  run('/usr/bin/docker',['update','--restart=no',app]);
  run('/usr/bin/docker',['rename',app,retained]);
  const args=['run','-d','--pull=never','--name',app,'--restart','unless-stopped','--network','memory-workshop-preview-20260918','--user','1000:1000','--cap-drop','ALL','--security-opt','no-new-privileges','--read-only','--pids-limit','128','--memory','1g','--tmpfs','/tmp:rw,noexec,nosuid,size=64m','--workdir','/state'];
  for(const [source,target] of [[`${staging}/memory`,'/memory'],[`${root}/artifacts/loopback-stdio`,'/loopback-stdio'],[`${root}/app.toml`,'/config.toml'],[`${staging}/web`,'/web'],[staging,'/interaction']])args.push('--mount',`type=bind,src=${source},dst=${target},readonly`);
  args.push('--mount',`type=bind,src=${root}/state,dst=/state`);
  for(const value of ['HOME=/state','XDG_CONFIG_HOME=/state/config','XDG_DATA_HOME=/state/data','XDG_STATE_HOME=/state/state','MEMORY_LAYER_PROFILE=prod'])args.push('--env',value);
  args.push('ubuntu@sha256:cd21a4f68a617580279d4b091cb18e3af9fa8a87500665f0ae5f7f757d17d367','/memory','--config','/config.toml','service','run');
  run('/usr/bin/docker',args);
  run('/usr/bin/systemctl',['--user','restart','memory-workshop-preview-relay.service']);
  console.log(JSON.stringify({started:app,old_container_retained:retained,provider_calls:0,provider_key_in_app:false}));
}
