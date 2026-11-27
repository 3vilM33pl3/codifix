#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
if(Number(process.versions.node.split('.')[0])<22) throw new Error('Node.js 22 or newer is required');
if(process.argv.length===3 && process.argv[2]==='--check') {
  await import('@atproto/oauth-client-node');
  console.log(`Codifix identity broker dependencies load on Node ${process.versions.node}; disabled non-production candidate`);
} else {
  if(process.argv.length!==2) throw new Error('Usage: launch.mjs [--check]');
  if(process.env.CODIFIX_IDENTITY_CANDIDATE_ALLOW!=='1') throw new Error('Disabled non-production candidate: explicit isolated-test opt-in required (see README)');
  await import('./src/main.mjs');
}
