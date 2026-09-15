// SPDX-License-Identifier: AGPL-3.0-or-later
// Driver component, not official grading or a replacement for mem-eval results.
import { createHash } from 'node:crypto';
import { TextDecoder } from 'node:util';
import { runOfflineContainer, validateInput } from './container.mjs';

const digest = files => createHash('sha256').update(JSON.stringify(
  Object.entries(files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0),
)).digest('hex');

export function validateSequence({ image, initialFiles, submissionPaths, steps, timeoutMs = 10000 }) {
  if (!Array.isArray(steps) || steps.length !== 3) throw Error('require three sequence steps');
  if (!Array.isArray(submissionPaths) || !submissionPaths.length || new Set(submissionPaths).size !== submissionPaths.length) throw Error('invalid submission selection');
  if (!initialFiles || submissionPaths.some(p => typeof p !== 'string' || !Object.hasOwn(initialFiles, p))) throw Error('submission files must be declared in the initial fixture');
  const ids = new Set();
  for (const step of steps) {
    if (!step || typeof step.id !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(step.id) || ids.has(step.id)) throw Error('invalid step identity');
    ids.add(step.id);
    validateInput({ image, argv: step.argv, files: initialFiles, timeoutMs });
    if (!step.grader || !step.grader.files || Object.getPrototypeOf(step.grader.files) !== Object.prototype || !Object.keys(step.grader.files).length) throw Error('hidden grader required');
    validateInput({ image, argv: step.grader.argv, files: gradeFiles(initialFiles, submissionPaths, step.grader.files), timeoutMs });
  }
}

function gradeFiles(submission, paths, checks) {
  // Explicit disjoint namespaces: a submitted grader/check.mjs cannot overwrite
  // the private grader/check.mjs. No host filesystem locations are accepted.
  return Object.fromEntries([
    ...paths.filter(p => Object.hasOwn(submission, p)).map(p => [`submission/${p}`, submission[p]]),
    ...Object.entries(checks).map(([p, bytes]) => [`grader/${p}`, bytes]),
  ]);
}

function selectedSubmission(result, paths) {
  const decoder = new TextDecoder('utf-8', { fatal: true });
  return Object.fromEntries(paths.filter(p => Object.hasOwn(result.files, p)).map(p => {
    return [p, decoder.decode(Buffer.from(result.files[p], 'base64'))];
  }));
}

export async function runOfflineSequence(options) {
  validateSequence(options); // Validate every future step before any execution.
  // Copy the reviewed JSON contract so asynchronous caller mutation cannot change
  // a later grader, command or allowlist while the current step is running.
  const { image, initialFiles, submissionPaths, steps, timeoutMs = 10000 } = structuredClone(options);
  let submission = Object.fromEntries(submissionPaths.map(p => [p, initialFiles[p]]));
  const diagnostics = [];
  const started = performance.now();
  for (const step of steps) {
    const publicFiles = Object.fromEntries(Object.entries(initialFiles).filter(([p]) => !submissionPaths.includes(p)));
    const agentFiles = { ...publicFiles, ...submission };
    const agent = await runOfflineContainer({ image, files: agentFiles, argv: step.argv, timeoutMs, exportPaths: submissionPaths });
    const selected = selectedSubmission(agent, submissionPaths);
    const grader = await runOfflineContainer({ image,
      files: gradeFiles(selected, submissionPaths, step.grader.files), argv: step.grader.argv, timeoutMs, exportPaths: [] });
    // Only allowlisted agent bytes move forward. Never carry grader output files,
    // stdout, hidden test contents or grading feedback into another agent step.
    diagnostics.push({ id: step.id, agentExitCode: agent.exitCode, graderExitCode: grader.exitCode,
      passed: agent.exitCode === 0 && grader.exitCode === 0 && submissionPaths.every(p => Object.hasOwn(selected, p)),
      agentInputSha256: digest(agentFiles), submissionSha256: digest(selected),
      graderInputSha256: digest(gradeFiles(selected, submissionPaths, step.grader.files)),
      agentElapsedMs: agent.elapsedMs, graderElapsedMs: grader.elapsedMs });
    submission = selected;
  }
  return { version: 1, kind: 'offline-sequence-smoke', officialEvidence: false,
    success: diagnostics.every(s => s.passed), elapsedMs: performance.now() - started,
    initialSha256: digest(initialFiles), image, steps: diagnostics, submission };
}
