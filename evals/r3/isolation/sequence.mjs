// SPDX-License-Identifier: AGPL-3.0-or-later
// Driver component, not official grading or a replacement for mem-eval results.
import { createHash } from 'node:crypto';
import { TextDecoder } from 'node:util';
import { runOfflineExecution, validateExecutionInput, validateExecutionOutput } from './execution.mjs';
import { gradeRustCase, validateRustCase } from './rust.mjs';

const digest = files => createHash('sha256').update(JSON.stringify(
  Object.entries(files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0),
)).digest('hex');

export function validateSequence({ image, execution, runAgent, initialFiles, submissionPaths, steps, timeoutMs = 10000 }) {
  const native = execution?.kind === 'windows-native-v1';
  if (native && (image !== undefined || runAgent !== undefined)) throw Error('native sequence requires explicit native commands, not a Linux image/model adapter');
  const boundary = execution === undefined ? { image } : { execution, ...(image !== undefined ? { image } : {}) };
  if (!Array.isArray(steps) || steps.length !== 3) throw Error('require three sequence steps');
  if (!Array.isArray(submissionPaths) || !submissionPaths.length || new Set(submissionPaths).size !== submissionPaths.length) throw Error('invalid submission selection');
  if (!initialFiles || submissionPaths.some(p => typeof p !== 'string' || !Object.hasOwn(initialFiles, p))) throw Error('submission files must be declared in the initial fixture');
  const ids = new Set();
  for (const step of steps) {
    if (!step || typeof step.id !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(step.id) || ids.has(step.id)) throw Error('invalid step identity');
    ids.add(step.id);
    const inputs = step.inputFiles ?? {};
    if (Object.getPrototypeOf(inputs) !== Object.prototype || Object.keys(inputs).some(p => Object.hasOwn(initialFiles, p))) throw Error('step inputs cannot replace fixture or submission files');
    validateExecutionInput({ ...boundary, argv: step.argv, files: { ...initialFiles, ...inputs }, timeoutMs, exportPaths: submissionPaths });
    if (step.grader?.kind === 'rust-blackbox') {
      if (native) throw Error('native Rust-blackbox grading is not integrated; no Linux fallback');
      validateRustCase({ image, files: Object.fromEntries(submissionPaths.map(p => [p, initialFiles[p]])),
        probes: step.grader.probes, timeoutMs });
      continue;
    }
    if (!step.grader || !step.grader.files || Object.getPrototypeOf(step.grader.files) !== Object.prototype || !Object.keys(step.grader.files).length) throw Error('hidden grader required');
    validateExecutionInput({ ...boundary, argv: step.grader.argv, files: gradeFiles(initialFiles, submissionPaths, step.grader.files), timeoutMs, exportPaths: [] });
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

async function runSequence(options, runExecution) {
  validateSequence(options); // Validate every future step before any execution.
  const { runAgent, ...contract } = options;
  if (runAgent !== undefined && typeof runAgent !== 'function') throw Error('invalid trusted agent adapter');
  // Copy the reviewed JSON contract so asynchronous caller mutation cannot change
  // a later grader, command or allowlist while the current step is running.
  const { image, execution, initialFiles, submissionPaths, steps, timeoutMs = 10000 } = structuredClone(contract);
  const native = execution?.kind === 'windows-native-v1';
  const boundary = execution === undefined ? { image } : { execution, ...(image !== undefined ? { image } : {}) };
  let submission = Object.fromEntries(submissionPaths.map(p => [p, initialFiles[p]]));
  const diagnostics = [];
  const started = performance.now();
  for (const step of steps) {
    const publicFiles = Object.fromEntries(Object.entries(initialFiles).filter(([p]) => !submissionPaths.includes(p)));
    const agentFiles = { ...publicFiles, ...(step.inputFiles ?? {}), ...submission };
    const rust = step.grader.kind === 'rust-blackbox';
    const publicInput = { ...boundary, files: agentFiles, argv: step.argv, timeoutMs,
      exportPaths: submissionPaths, profile: rust ? 'rust' : 'text' };
    // Trusted model adapters receive only this step's public workspace. Do not
    // pass the step object: it contains the hidden probes and future context.
    const agent = structuredClone(runAgent ? await runAgent({ ...structuredClone(publicInput), stepId: step.id })
      : await runExecution(publicInput));
    const { usage: agentUsage, ...agentExecution } = agent;
    validateExecutionOutput(publicInput, agentExecution);
    if (runAgent && (!agentUsage || Object.keys(agentUsage).sort().join(',') !== 'input_tokens,microusd,new_provider_requests,output_tokens,provider_requests'
        || Object.values(agentUsage).some(v => !Number.isSafeInteger(v) || v < 0)
        || agentUsage.new_provider_requests > agentUsage.provider_requests)) throw Error('missing or invalid agent usage');
    const selected = selectedSubmission(agent, submissionPaths);
    let grader, graderInputSha256;
    if (rust) {
      graderInputSha256 = digest({ source: selected, probes: step.grader.probes });
      // A deleted required file is a valid failed submission, not malformed
      // campaign configuration. Preserve the missing file for the next agent.
      const result = submissionPaths.every(p => Object.hasOwn(selected, p))
        ? await gradeRustCase({ image, files: selected, probes: step.grader.probes, timeoutMs })
        : { success: false, elapsedMs: 0 };
      grader = { exitCode: result.success ? 0 : 1, elapsedMs: result.elapsedMs };
    } else {
      graderInputSha256 = digest(gradeFiles(selected, submissionPaths, step.grader.files));
      const graderInput = { ...boundary,
        files: gradeFiles(selected, submissionPaths, step.grader.files), argv: step.grader.argv, timeoutMs, exportPaths: [] };
      grader = structuredClone(await runExecution(graderInput));
      validateExecutionOutput(graderInput, grader);
    }
    // Only allowlisted agent bytes move forward. Never carry grader output files,
    // stdout, hidden test contents or grading feedback into another agent step.
    diagnostics.push({ id: step.id, agentExitCode: agent.exitCode, graderExitCode: grader.exitCode,
      passed: agent.exitCode === 0 && grader.exitCode === 0 && submissionPaths.every(p => Object.hasOwn(selected, p)),
      agentInputSha256: digest(agentFiles), submissionSha256: digest(selected),
      graderInputSha256, graderKind: rust ? 'rust-blackbox' : 'command',
      agentElapsedMs: agent.elapsedMs, graderElapsedMs: grader.elapsedMs,
      ...(native ? { agentExecution: agent.execution, graderExecution: grader.execution } : {}),
      ...(runAgent ? { providerUsage: structuredClone(agentUsage) } : {}) });
    submission = selected;
  }
  let providerUsage;
  if (runAgent) {
    providerUsage = Object.fromEntries(Object.keys(diagnostics[0].providerUsage).map(key => [key, 0]));
    for (const step of diagnostics) for (const key of Object.keys(providerUsage)) {
      const sum = providerUsage[key] + step.providerUsage[key];
      if (!Number.isSafeInteger(sum)) throw Error('sequence usage overflow');
      providerUsage[key] = sum;
    }
  }
  return { version: 1, kind: native ? 'native-windows-sequence-smoke' : 'offline-sequence-smoke', officialEvidence: false,
    success: diagnostics.every(s => s.passed), elapsedMs: performance.now() - started,
    initialSha256: digest(initialFiles), ...(native ? { execution: { kind: execution.kind,
      profile_sha256: execution.profile_sha256, disk_hard_limit_enforced: false } } : { image }), steps: diagnostics, submission,
    ...(runAgent ? { providerUsage } : {}) };
}
export function createSequenceRunner({ runExecution = runOfflineExecution } = {}) {
  if (typeof runExecution !== 'function') throw Error('invalid trusted execution adapter');
  return options => runSequence(options, runExecution);
}
export const runOfflineSequence = createSequenceRunner();
