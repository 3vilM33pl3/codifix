// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';

const root=fileURLToPath(new URL('../',import.meta.url));
const policy=JSON.parse(readFileSync(new URL('../audit-policy.json',import.meta.url)));
export function checkAudit(report,lock) {
  assert.equal(lock.packages['node_modules/@atproto/oauth-client-node'].version,policy.sdk,'SDK pin changed: review policy');
  assert.equal(lock.packages['node_modules/zod'].version,policy.zod,'Zod pin changed: review policy');
  assert.equal(report.auditReportVersion,2,'Unsupported or missing audit report');
  assert.ok(!report.error,'Audit service failed');
  assert.ok(report.vulnerabilities && report.metadata?.vulnerabilities,'Incomplete audit report');
  const vulnerabilities=report.vulnerabilities;
  const advisoryRoots=(name,seen=new Set())=>{
    assert.ok(!seen.has(name),'Cyclic audit finding');
    const item=vulnerabilities[name];
    assert.ok(item && policy.affectedPackages.includes(name),`Unexpected vulnerable package: ${name}`);
    assert.equal(item.name,name);
    assert.equal(item.severity,'high',`Unexpected severity: ${name}`);
    assert.ok(Array.isArray(item.via) && item.via.length>0,'Missing advisory provenance');
    const next=new Set([...seen,name]);
    return item.via.flatMap(via=>{
      if(typeof via==='string') return advisoryRoots(via,next);
      assert.equal(via.url,policy.advisory,`Unexpected advisory: ${via.url}`);
      assert.equal(via.name,'zod');
      assert.equal(via.severity,'high');
      assert.equal(via.range,'<4.6.6');
      return [via.url];
    });
  };
  for(const name of Object.keys(vulnerabilities)) assert.ok(advisoryRoots(name).length);
  assert.equal(report.metadata.vulnerabilities.total,Object.keys(vulnerabilities).length,'Inconsistent audit total');
  for(const level of ['info','low','moderate','critical']) assert.equal(report.metadata.vulnerabilities[level],0,`Unexpected ${level} finding`);
  assert.equal(report.metadata.vulnerabilities.high,Object.keys(vulnerabilities).length);
  return {status:policy.status,advisory:policy.advisory,affected_entries:Object.keys(vulnerabilities).length,
    sdk:policy.sdk,zod:policy.zod,production_activation_allowed:false,
    note:'Known upstream advisory remains unresolved. JSON transport hardening does not fix it.'};
}

if(process.argv[1] && fileURLToPath(import.meta.url)===resolve(process.argv[1])) {
  const result=spawnSync('npm',['audit','--omit=dev','--json'],{cwd:root,encoding:'utf8',maxBuffer:4*1024*1024});
  assert.ok(!result.error && [0,1].includes(result.status),'npm audit failed to run');
  const report=JSON.parse(result.stdout);
  const summary=checkAudit(report,JSON.parse(readFileSync(`${root}/package-lock.json`)));
  if(process.argv[2]) writeFileSync(process.argv[2],JSON.stringify({checked_at:new Date().toISOString(),summary,report},null,2)+'\n');
  console.log(JSON.stringify(summary,null,2));
}
