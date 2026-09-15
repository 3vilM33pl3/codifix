// SPDX-License-Identifier: AGPL-3.0-or-later
// Fixture verification only: never use held-out references as model calibration.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { gradeRustCase } from './isolation/rust.mjs';
import { runOfflineSequence } from './isolation/sequence.mjs';

const image = process.env.MEMORY_R3_RUST_IMAGE;
if (process.env.MEMORY_R3_REQUIRE_RUST === '1' && !image) throw Error('required Rust image missing');

test('held-out framed receiver: chunks, poison boundaries and close/reset lifecycle', { skip: !image, timeout: 180000 }, async () => {
  const root = new URL('./representative/held-out/framed-stream/', import.meta.url);
  const files = {};
  for (const name of ['main', 'command', 'hex', 'frame', 'decoder']) {
    files[`src/${name}.rs`] = await fs.readFile(new URL(`public/src/${name}.rs`, root), 'utf8');
  }
  const steps = JSON.parse(await fs.readFile(new URL('private/steps.json', root), 'utf8'));
  assert.equal(steps.length, 3);
  assert.equal(new Set(steps.flatMap(step => step.probes.map(probe => probe.id))).size, 10);
  const reference = { ...files, 'src/decoder.rs': await fs.readFile(new URL('private/decoder.rs', root), 'utf8') };
  const complete = await gradeRustCase({ image, files: reference, probes: steps.flatMap(step => step.probes) });
  assert.equal(complete.success, true, JSON.stringify(complete.probes));
  const sequence = await runOfflineSequence({ image, initialFiles: files, submissionPaths: Object.keys(files), timeoutMs: 30000,
    steps: steps.map((step, index) => ({ id: step.id,
      argv: ['/usr/local/bin/node', '-e', index === 0
        ? `require('fs').writeFileSync('src/decoder.rs',${JSON.stringify(reference['src/decoder.rs'])})` : ''],
      grader: { kind: 'rust-blackbox', probes: steps.slice(0, index + 1).flatMap(stage => stage.probes) },
    })),
  });
  assert.equal(sequence.success, true);
  for (const step of steps) {
    assert.equal((await gradeRustCase({ image, files, probes: step.probes })).success, false, step.id);
    assert.equal((await gradeRustCase({ image, files: { 'src/main.rs': 'fn main() {}' }, probes: step.probes })).success, false);
  }
});
