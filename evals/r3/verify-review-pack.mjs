#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Offline verification of sources, split sizes and reference Rust graders.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
const root = path.resolve(process.argv[2]);
const manifest = JSON.parse(fs.readFileSync(path.join(root,'campaign.json'),'utf8'));
assert.equal(manifest.label_status,'draft');
assert.equal(manifest.smoke_only,true);
for (const [track,split,count,clusters] of [['knowledge','development',40,10],['knowledge','held_out',100,25],['coding','development',4,4],['coding','held_out',12,12]]) {
  const rows = manifest.cases.filter(c=>c.track===track && c.split===split);
  assert.equal(rows.length,count); assert.equal(new Set(rows.map(c=>c.cluster)).size,clusters);
}
const tmp = fs.mkdtempSync(path.join(os.tmpdir(),'memory-r3-grader-'));
let checked = 0;
try {
  for (const c of manifest.cases.filter(c=>c.track==='coding')) {
    fs.copyFileSync(path.join(root,`private/reference-solutions/${c.id}.rs`),path.join(tmp,'submission.rs'));
    for (let step=1;step<=3;step++) {
      fs.copyFileSync(path.join(root,`private/graders/${c.id}/step-${step}.rs`),path.join(tmp,'grader.rs'));
      const compile=spawnSync('rustc',['--edition=2024','--test',path.join(tmp,'grader.rs'),'-o',path.join(tmp,'grade')],{encoding:'utf8'});
      assert.equal(compile.status,0,`${c.id}/${step}: ${compile.stderr}`);
      const run=spawnSync(path.join(tmp,'grade'),[],{encoding:'utf8'});
      assert.equal(run.status,0,`${c.id}/${step}: ${run.stdout} ${run.stderr}`); checked++;
    }
  }
} finally { fs.rmSync(tmp,{recursive:true,force:true}); }
console.log(JSON.stringify({reference_graders_passed:checked,labels:'draft',official_evidence:false}));
