#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Read-only metadata inspection. Never clears a fence or invokes a provider.
import path from 'node:path';
import { inspectCampaignDispatch } from './campaign-dispatch.mjs';

try {
  if (process.argv.length !== 3) throw Error('expected one private dispatch directory');
  const report = await inspectCampaignDispatch(path.resolve(process.argv[2]));
  console.log(JSON.stringify(report, null, 2));
  if (report.entries.some(entry => entry.status === 'uncertain')) process.exitCode = 2;
} catch {
  console.error('Dispatch evidence could not be validated. Retain it unchanged; do not retry provider work.');
  process.exitCode = 1;
}
