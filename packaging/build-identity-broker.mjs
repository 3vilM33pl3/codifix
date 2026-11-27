#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {cpSync,existsSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync,chmodSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const run=(command,args,cwd=root)=>{
  const result=spawnSync(command,args,{cwd,encoding:'utf8',maxBuffer:16*1024*1024});
  assert.equal(result.status,0,`${command} failed: ${result.stderr}\n${result.stdout}`);
  return result.stdout.trim();
};
assert.ok(Number(process.versions.node.split('.')[0])>=22,'Node >=22 required');
assert.ok(process.argv.length===3,'Usage: node packaging/build-identity-broker.mjs OUTPUT_DIRECTORY');
assert.equal(run('git',['status','--porcelain','--untracked-files=no']),'','Commit tracked source before packaging');
const revision=run('git',['rev-parse','HEAD']);
const epoch=run('git',['show','-s','--format=%ct','HEAD']);
const version=readFileSync(join(root,'Cargo.toml'),'utf8').match(/^version = "([^"]+)"/m)[1];
assert.match(version,/^\d+\.\d+\.\d+-rc\.\d+$/,'Broker archive restricted to a release candidate');
const output=resolve(process.argv[2]);mkdirSync(output,{recursive:true});
const name=`codifix-identity-broker_${version}_${revision.slice(0,8)}`;
const artifact=join(output,name+'.tar.gz');assert.ok(!existsSync(artifact),'Refusing to overwrite artifact');
const stage=mkdtempSync(join(tmpdir(),'codifix-broker-package-'));
try {
  // Copy only immutable tracked source. Untracked or ignored files (including
  // credentials) in a developer's source directories must never enter artifacts.
  const source=join(stage,'source');mkdirSync(source);
  run('git',['archive',`--output=${join(stage,'source.tar')}`,revision,'identity-broker','packaging/identity-broker','LICENSE']);
  run('tar',['-xf',join(stage,'source.tar'),'-C',source]);
  const pkg=join(stage,name);mkdirSync(pkg);
  for(const file of ['package.json','package-lock.json','src','scripts','audit-policy.json','README.md']) cpSync(join(source,'identity-broker',file),join(pkg,file),{recursive:true});
  for(const file of ['launch.mjs','codifix-identity.service.example','identity-broker.env.example']) cpSync(join(source,'packaging/identity-broker',file),join(pkg,file));
  cpSync(join(source,'LICENSE'),join(pkg,'LICENSE'));chmodSync(join(pkg,'launch.mjs'),0o755);
  run('npm',['ci','--omit=dev','--ignore-scripts','--no-audit','--no-fund'],pkg);
  run(process.execPath,['scripts/audit.mjs',join(pkg,'audit-report.json')],pkg);
  run(process.execPath,['launch.mjs','--check'],pkg);
  const lock=JSON.parse(readFileSync(join(pkg,'package-lock.json')));
  const dependencies=Object.entries(lock.packages).filter(([path])=>path).map(([path,entry])=>({path,version:entry.version,resolved:entry.resolved,integrity:entry.integrity,license:entry.license}));
  writeFileSync(join(pkg,'dependency-provenance.json'),JSON.stringify(dependencies,null,2)+'\n');
  writeFileSync(join(pkg,'THIRD-PARTY-NOTICES.txt'),'Locked production dependencies are included under node_modules with their distributed license and notice files.\nSee dependency-provenance.json for package versions, registry sources, integrity hashes and license declarations.\n\n'+dependencies.map(d=>`${d.path} ${d.version}: ${d.license??'See distributed package license'}`).join('\n')+'\n');
  const sha=file=>createHash('sha256').update(readFileSync(file)).digest('hex');
  writeFileSync(join(pkg,'candidate-manifest.json'),JSON.stringify({schema:1,status:'disabled-non-production-candidate',core_version:version,broker_version:lock.version,source_revision:revision,node_required:'>=22',validated_node:process.versions.node,sdk:'0.5.13',lock_sha256:sha(join(pkg,'package-lock.json')),production_activation_allowed:false,known_advisory:'GHSA-mrhx-c7xc-m3w5'},null,2)+'\n');
  const files=(dir,prefix='')=>readdirSync(dir,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?files(join(dir,entry.name),prefix+entry.name+'/'):entry.isFile()?[prefix+entry.name]:[]);
  writeFileSync(join(pkg,'SHA256SUMS'),files(pkg).sort().map(path=>`${sha(join(pkg,path))}  ${path}`).join('\n')+'\n');
  run('tar',['--sort=name',`--mtime=@${epoch}`,'--owner=0','--group=0','--numeric-owner','-cf',join(stage,'broker.tar'),name],stage);
  const compressed=spawnSync('gzip',['-n','-c',join(stage,'broker.tar')],{maxBuffer:128*1024*1024});assert.equal(compressed.status,0);writeFileSync(artifact,compressed.stdout,{flag:'wx'});
  writeFileSync(artifact+'.sha256',`${sha(artifact)}  ${name}.tar.gz\n`,{flag:'wx'});
  console.log(artifact);
} finally {rmSync(stage,{recursive:true,force:true});}
