#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
import fs from 'node:fs/promises';
import path from 'node:path';
import { packagePaidRuntime, regular } from './paid-runtime.mjs';
try {
  if (process.argv.length !== 4) throw Error('arguments');
  const root = await fs.realpath(process.argv[2]);
  const runtime = await packagePaidRuntime(root, JSON.parse(await regular(path.resolve(process.argv[3]), 16384)));
  console.log(JSON.stringify({ packaged: true, labels: 'draft', approval_created: false, provider_called: false,
    node: path.join(root, runtime.binaries.node.path), launcher: path.join(root, 'runtime/launch-paid.mjs') }));
} catch {
  console.error('Runtime packaging failed. Retain the draft pack; no approval or provider call was made.');
  process.exitCode = 1;
}
