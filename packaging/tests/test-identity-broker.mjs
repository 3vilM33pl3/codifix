// SPDX-License-Identifier: AGPL-3.0-or-later
// Exercise the delivered archive in a temporary directory, never install it.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,readdirSync,readFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
assert.equal(process.argv.length,3,'Usage: node packaging/tests/test-identity-broker.mjs ARCHIVE');
const archive=resolve(process.argv[2]);
const stage=mkdtempSync(join(tmpdir(),'codifix broker # candidate '));
const run=(cmd,args,options={})=>{
  const r=spawnSync(cmd,args,{cwd:stage,encoding:'utf8',...options});
  assert.equal(r.status,0,`${cmd}: ${r.stderr}\n${r.stdout}`);return r.stdout;
};
try {
  run('tar',['-xzf',archive,'-C',stage]);
  const entries=readdirSync(stage);assert.equal(entries.length,1);
  const pkg=join(stage,entries[0]);
  run('sha256sum',['-c','SHA256SUMS'],{cwd:pkg});
  const manifest=JSON.parse(readFileSync(join(pkg,'candidate-manifest.json')));
  assert.equal(manifest.status,'disabled-non-production-candidate');
  assert.equal(manifest.production_activation_allowed,false);
  assert.equal(manifest.sdk,'0.5.13');assert.equal(manifest.node_required,'>=22');
  assert.match(manifest.source_revision,/^[a-f0-9]{40}$/);
  for(const file of ['LICENSE','THIRD-PARTY-NOTICES.txt','dependency-provenance.json','audit-report.json','codifix-identity.service.example','identity-broker.env.example']) assert.ok(readFileSync(join(pkg,file)).length);
  assert.ok(!readdirSync(pkg).some(name=>['test','.env','.git'].includes(name)));
  const launch=join(pkg,'launch.mjs');
  assert.match(run(process.execPath,[launch,'--check']),/dependencies load/);
  const disabled=spawnSync(process.execPath,[launch],{cwd:stage,encoding:'utf8',env:{...process.env,CODIFIX_IDENTITY_CANDIDATE_ALLOW:'0'}});
  assert.notEqual(disabled.status,0);assert.match(disabled.stderr,/Disabled non-production candidate/);
  const oldNode=spawnSync(process.execPath,['--input-type=module','-e',`Object.defineProperty(process.versions,'node',{value:'20.0.0'});await import(${JSON.stringify(pathToFileURL(launch).href)});`],{cwd:stage,encoding:'utf8'});
  assert.notEqual(oldNode.status,0);assert.match(oldNode.stderr,/Node.js 22 or newer/);
  const audit=JSON.parse(readFileSync(join(pkg,'audit-report.json')));
  assert.equal(audit.summary.production_activation_allowed,false);
  assert.equal(audit.summary.advisory,'https://github.com/advisories/GHSA-mrhx-c7xc-m3w5');
  console.log(`Broker archive verified at ${manifest.source_revision}; dependencies, checksums, alternate CWD, disabled startup and Node floor pass. Known advisory remains unresolved (${audit.summary.affected_entries} entries).`);
} finally {rmSync(stage,{recursive:true,force:true});}
