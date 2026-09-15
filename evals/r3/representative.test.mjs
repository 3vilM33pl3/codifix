// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { gradeRustCase } from './isolation/rust.mjs';
import { runOfflineSequence } from './isolation/sequence.mjs';

const image = process.env.MEMORY_R3_RUST_IMAGE;
if (process.env.MEMORY_R3_REQUIRE_RUST === '1' && !image) throw Error('required Rust image missing');
const root = new URL('./representative/development/durable-importer/', import.meta.url);
async function fixture() {
  const files = {};
  for (const name of ['main', 'input', 'cursor', 'store', 'replay']) {
    files[`src/${name}.rs`] = await fs.readFile(new URL(`public/src/${name}.rs`, root), 'utf8');
  }
  const steps = JSON.parse(await fs.readFile(new URL('private/steps.json', root), 'utf8'));
  return { files, steps };
}

test('representative importer declares three cumulative stages and distinct private probes', async () => {
  const { files, steps } = await fixture();
  assert.equal(Object.keys(files).length, 5);
  assert.equal(steps.length, 3);
  const ids = steps.flatMap(step => step.probes.map(p => p.id));
  assert.equal(new Set(ids).size, ids.length);
  assert.equal(ids.length, 10);
  for (const source of Object.values(files)) assert(!source.includes('expectedStdout'));
});

test('representative importer: executable reference and broken controls', { skip: !image, timeout: 120000 }, async () => {
  const { files, steps } = await fixture();
  const reference = { ...files, 'src/replay.rs': await fs.readFile(new URL('private/replay.rs', root), 'utf8') };
  const probes = steps.flatMap(step => step.probes);
  const correct = await gradeRustCase({ image, files: reference, probes });
  assert.equal(correct.success, true);
  // Deterministic reference agent, not an evaluated model. Earlier acceptance
  // checks remain active at later stages, and the private probes stay host-side.
  const sequence = await runOfflineSequence({ image, initialFiles: files,
    submissionPaths: Object.keys(files), timeoutMs: 30000,
    steps: steps.map((step, index) => ({ id: step.id,
      argv: ['/usr/local/bin/node', '-e', index === 0
        ? `require('fs').writeFileSync('src/replay.rs',${JSON.stringify(reference['src/replay.rs'])})`
        : ''],
      grader: { kind: 'rust-blackbox', probes: steps.slice(0, index + 1).flatMap(s => s.probes) },
    })),
  });
  assert.equal(sequence.success, true);
  assert.equal(sequence.steps.length, 3);
  const initial = await gradeRustCase({ image, files, probes: [steps[0].probes[0], steps[0].probes[1]] });
  assert.deepEqual(initial.probes.map(p => p.passed), [true, false]);
  for (const step of steps) {
    const broken = await gradeRustCase({ image, files, probes: step.probes });
    assert.equal(broken.success, false, step.id);
    const noOp = await gradeRustCase({ image, files: { 'src/main.rs': 'fn main() {}' }, probes: [step.probes[0]] });
    assert.equal(noOp.success, false, `${step.id} no-op must fail`);
  }
});
