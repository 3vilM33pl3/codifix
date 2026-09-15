// SPDX-License-Identifier: AGPL-3.0-or-later
// Fixture verification only: never use held-out references as model calibration.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { gradeRustCase } from './isolation/rust.mjs';
import { runOfflineSequence } from './isolation/sequence.mjs';

const image = process.env.MEMORY_R3_RUST_IMAGE;
if (process.env.MEMORY_R3_REQUIRE_RUST === '1' && !image) throw Error('required Rust image missing');

for (const [family, modules, replacement] of [
  ['framed-stream', ['main', 'command', 'hex', 'frame', 'decoder'], 'decoder'],
  ['interval-calendar', ['main', 'command', 'display', 'interval', 'calendar'], 'calendar'],
]) test(`held-out ${family}: reference, cumulative stages and broken controls`, { skip: !image, timeout: 180000 }, async () => {
  const root = new URL(`./representative/held-out/${family}/`, import.meta.url);
  const files = {};
  for (const name of modules) {
    files[`src/${name}.rs`] = await fs.readFile(new URL(`public/src/${name}.rs`, root), 'utf8');
  }
  const steps = JSON.parse(await fs.readFile(new URL('private/steps.json', root), 'utf8'));
  assert.equal(steps.length, 3);
  assert.equal(new Set(steps.flatMap(step => step.probes.map(probe => probe.id))).size, 10);
  const source = `src/${replacement}.rs`;
  const reference = { ...files, [source]: await fs.readFile(new URL(`private/${replacement}.rs`, root), 'utf8') };
  const complete = await gradeRustCase({ image, files: reference, probes: steps.flatMap(step => step.probes) });
  assert.equal(complete.success, true, JSON.stringify(complete.probes));
  const sequence = await runOfflineSequence({ image, initialFiles: files, submissionPaths: Object.keys(files), timeoutMs: 30000,
    steps: steps.map((step, index) => ({ id: step.id,
      argv: ['/usr/local/bin/node', '-e', index === 0
        ? `require('fs').writeFileSync(${JSON.stringify(source)},${JSON.stringify(reference[source])})` : ''],
      grader: { kind: 'rust-blackbox', probes: steps.slice(0, index + 1).flatMap(stage => stage.probes) },
    })),
  });
  assert.equal(sequence.success, true);
  for (const step of steps) {
    assert.equal((await gradeRustCase({ image, files, probes: step.probes })).success, false, step.id);
    assert.equal((await gradeRustCase({ image, files: { 'src/main.rs': 'fn main() {}' }, probes: step.probes })).success, false);
  }
});
