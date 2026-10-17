// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted driver for an external structured model callback. Commands execute
// only in existing network-none containers or explicitly pinned Windows
// AppContainer execution; the model receives no host tools,
// credentials, hidden probes or grader feedback. Paid adapters remain separate.
import { createHash } from 'node:crypto';
import { TextDecoder } from 'node:util';
import { runOfflineContainer, validateInput } from './container.mjs';
import { dispatchModel } from './model-dispatch.mjs';
import { runOfflineExecution, validateExecutionInput, validateExecutionOutput } from './execution.mjs';

const nativeAgents = new WeakMap();
export const isNativeModelAgent = (agent, execution) => execution?.kind === 'windows-native-v1'
  && nativeAgents.get(agent) === execution.profile_sha256;

export function createModelAgent(options) {
  const { mediator, invoke, prepareRequest, runNative = runOfflineExecution, ...plain } = options;
  const { taskId, model, allowance, maxCalls = 12, taskTimeoutMs = 60000, modelTimeoutMs,
    requireFullModelWindow = false, deadlineMs, nativeExecution } = structuredClone(plain);
  if (!mediator || typeof mediator.dispatch !== 'function'
      || (typeof invoke === 'function') === (typeof prepareRequest === 'function')
      || typeof runNative !== 'function' || typeof taskId !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(taskId)
      || !model || Object.keys(model).sort().join(',') !== 'provider,reasoning,resolved_id'
      || Object.values(model).some(v => typeof v !== 'string' || !/^[A-Za-z0-9._-]{1,128}$/.test(v))
      || !Number.isInteger(maxCalls) || maxCalls < 1 || maxCalls > 128
      || !Number.isInteger(taskTimeoutMs) || taskTimeoutMs < 1000 || taskTimeoutMs > 600000
      || typeof requireFullModelWindow !== 'boolean' || (requireFullModelWindow && modelTimeoutMs === undefined)
      || (deadlineMs !== undefined && (!Number.isFinite(deadlineMs) || deadlineMs < 0))
      || (modelTimeoutMs !== undefined && (!Number.isInteger(modelTimeoutMs)
        || modelTimeoutMs < 1000 || modelTimeoutMs > 600000))) throw Error('invalid model agent contract');
  if (nativeExecution !== undefined) validateExecutionInput({ execution: nativeExecution,
    argv: [nativeExecution?.profile?.node?.path], files: {}, exportPaths: [], timeoutMs: 1000 });
  let calls = 0, usedMs = 0, busy = false;
  const agent = async input => {
    if (busy) throw Error('sequence agent already running');
    const { stepId, ...container } = structuredClone(input);
    const native = container.execution?.kind === 'windows-native-v1';
    if (native) {
      if (nativeExecution === undefined || container.execution.profile_sha256 !== nativeExecution.profile_sha256) throw Error('native model adapter profile mismatch; no fallback');
      validateExecutionInput(container);
    } else {
      if (container.execution !== undefined || nativeExecution !== undefined) throw Error('model adapter backend mismatch; no fallback');
      validateInput(container);
    }
    if (typeof stepId !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(stepId)
        || !container.exportPaths) throw Error('explicit step and submission selection required');
    busy = true;
    const started = performance.now();
    let files = container.files, previous = null;
    const usage = { provider_requests: 0, new_provider_requests: 0, input_tokens: 0, output_tokens: 0, microusd: 0 };
    const commandExecutions = [];
    const remainingTime = () => Math.floor(Math.min(taskTimeoutMs - usedMs - (performance.now() - started),
      deadlineMs === undefined ? Infinity : deadlineMs - performance.now()));
    const executeNative = async (command, kind) => {
      validateExecutionInput(command);
      const output = structuredClone(await runNative(structuredClone(command)));
      validateExecutionOutput(command, output);
      commandExecutions.push({ kind, exitCode: output.exitCode, elapsedMs: output.elapsedMs,
        execution: structuredClone(output.execution) });
      return output;
    };
    const result = async exitCode => {
      if (native) {
        const remaining = remainingTime();
        if (remaining < 1000) throw Error('native model finalization deadline exhausted; no execution receipt fabricated');
        const output = await executeNative({ ...container, files,
          argv: [nativeExecution.profile.node.path, '-e', `process.exit(${exitCode})`],
          timeoutMs: Math.min(container.timeoutMs, remaining) }, 'final-export');
        const expectedFiles = Object.fromEntries(container.exportPaths.filter(p => Object.hasOwn(files, p))
          .map(p => [p, Buffer.from(files[p]).toString('base64')]));
        const normalized = value => JSON.stringify(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)));
        if (output.exitCode !== exitCode || output.stdout !== '' || output.stderr !== ''
            || normalized(output.files) !== normalized(expectedFiles)) throw Error('native model final export failed');
        if (remainingTime() < 0) throw Error('native model total deadline exceeded during final export');
        return { ...output, elapsedMs: performance.now() - started, usage: { ...usage }, commandExecutions };
      }
      return { version: 1, image: container.image, exitCode,
      elapsedMs: performance.now() - started, stdout: '', stderr: '', usage: { ...usage },
      files: Object.fromEntries(container.exportPaths.filter(p => Object.hasOwn(files, p))
        .map(p => [p, Buffer.from(files[p]).toString('base64')])) };
    };
    try {
      for (let round = 0; ; round++) {
        const remaining = remainingTime() - (native ? 1000 : 0);
        if (calls >= maxCalls || remaining < 1000
            || (requireFullModelWindow && remaining < modelTimeoutMs + container.timeoutMs)) return await result(1);
        const payload = { model, step_id: stepId, files, previous,
          tools: { protocol: native ? 'isolated-windows-command-v1' : 'isolated-command-v1',
            command: native ? { kind: 'command', tool: 'node', args: ['arguments'] } : { kind: 'command', argv: ['absolute-executable', 'arguments'] },
            finish: { kind: 'finish' }, network: 'none', persistence: 'selected-submission-files-only' } };
        const requestId = 'call-' + createHash('sha256').update(JSON.stringify([taskId, stepId, round])).digest('hex');
        calls++;
        const receipt = await dispatchModel({ mediator, prepareRequest, taskId, requestId, payload, allowance, withReceipt: true,
          // Provider generation and isolated shell execution have independent
          // deadlines. Neither may exceed the remaining whole-task allowance.
          timeoutMs: Math.min(remaining, modelTimeoutMs ?? container.timeoutMs), invoke });
        const increments = { provider_requests: 1, new_provider_requests: receipt.invoked ? 1 : 0, ...receipt.usage };
        for (const key of Object.keys(usage)) {
          if (!Number.isSafeInteger(increments[key]) || increments[key] < 0
              || !Number.isSafeInteger(usage[key] + increments[key])) throw Error('invalid model usage accounting');
          usage[key] += increments[key];
        }
        const action = receipt.output;
        if (!action || Object.getPrototypeOf(action) !== Object.prototype) return await result(1);
        if (action.kind === 'finish' && Object.keys(action).join(',') === 'kind') return await result(0);
        if (action.kind !== 'command' || Object.keys(action).sort().join(',') !== (native ? 'args,kind,tool' : 'argv,kind')) return await result(1);
        if (native && (action.tool !== 'node' || !Array.isArray(action.args))) return await result(1);
        const commandRemaining = native ? remainingTime() - 1000
          : Math.floor(taskTimeoutMs - usedMs - (performance.now() - started));
        if (commandRemaining < 1000) return await result(1);
        const command = { ...container, files,
          argv: native ? [nativeExecution.profile.node.path, ...action.args] : action.argv,
          timeoutMs: Math.min(container.timeoutMs, commandRemaining) };
        try { if (native) validateExecutionInput(command); else validateInput(command); } catch { return await result(1); }
        const output = native ? await executeNative(command, 'model-command') : await runOfflineContainer(command);
        const decoder = new TextDecoder('utf-8', { fatal: true });
        const selected = Object.fromEntries(Object.entries(output.files).map(([p, bytes]) => [p, decoder.decode(Buffer.from(bytes, 'base64'))]));
        // Recreate fixed public context on each command; only allowlisted source
        // files persist, exactly as in the existing sequence boundary.
        files = { ...Object.fromEntries(Object.entries(container.files).filter(([p]) => !container.exportPaths.includes(p))), ...selected };
        previous = { exit_code: output.exitCode, stdout: output.stdout.slice(0, 32768), stderr: output.stderr.slice(0, 32768),
          output_truncated: output.stdout.length > 32768 || output.stderr.length > 32768 };
      }
    } finally { usedMs += performance.now() - started; busy = false; }
  };
  if (nativeExecution !== undefined) nativeAgents.set(agent, nativeExecution.profile_sha256);
  return agent;
}
