// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {checkAudit} from '../scripts/audit.mjs';

const lock=JSON.parse(readFileSync(new URL('../package-lock.json',import.meta.url)));
const report=()=>({auditReportVersion:2,metadata:{vulnerabilities:{info:0,low:0,moderate:0,high:1,critical:0,total:1}},vulnerabilities:{zod:{name:'zod',severity:'high',via:[{name:'zod',severity:'high',range:'<4.6.6',url:'https://github.com/advisories/GHSA-mrhx-c7xc-m3w5'}]}}});
test('candidate audit permits only the explicitly recorded upstream advisory',()=>{
  assert.equal(checkAudit(report(),lock).production_activation_allowed,false);
  for(const mutate of [r=>r.vulnerabilities.zod.via[0].url='https://github.com/advisories/new',r=>r.vulnerabilities.zod.via.push('unknown'),r=>r.vulnerabilities.zod.via=[],r=>r.error={},r=>r.metadata.vulnerabilities.critical=1,r=>r.vulnerabilities.zod.via=['zod']]) {
    const value=report();mutate(value);assert.throws(()=>checkAudit(value,lock));
  }
  const changed=structuredClone(lock);changed.packages['node_modules/@atproto/oauth-client-node'].version='0.5.16';assert.throws(()=>checkAudit(report(),changed));
});
