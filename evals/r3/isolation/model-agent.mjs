// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted driver for an external structured model callback. Commands execute
// only in existing network-none containers; the model receives no host tools,
// credentials, hidden probes or grader feedback. Paid adapters remain separate.
import { createHash } from 'node:crypto';
import { TextDecoder } from 'node:util';
import { runOfflineContainer, validateInput } from './container.mjs';

export function createModelAgent(options) {
  const { mediator, invoke, ...plain } = options;
  const { taskId, model, allowance, maxCalls = 12, taskTimeoutMs = 60000 } = structuredClone(plain);
  if (!mediator || typeof mediator.dispatch !== 'function' || typeof invoke !== 'function'
      || typeof taskId !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(taskId)
      || !model || Object.keys(model).sort().join(',') !== 'provider,reasoning,resolved_id'
      || Object.values(model).some(v => typeof v !== 'string' || !/^[A-Za-z0-9._-]{1,128}$/.test(v))
      || !Number.isInteger(maxCalls) || maxCalls < 1 || maxCalls > 128
      || !Number.isInteger(taskTimeoutMs) || taskTimeoutMs < 1000 || taskTimeoutMs > 600000) throw Error('invalid model agent contract');
  let calls = 0, usedMs = 0, busy = false;
  return async input => {
    if (busy) throw Error('sequence agent already running');
    const { stepId, ...container } = structuredClone(input);
    validateInput(container);
    if (typeof stepId !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(stepId)
        || !container.exportPaths) throw Error('explicit step and submission selection required');
    busy = true;
    const started = performance.now();
    let files = container.files, previous = null;
    const usage = { provider_requests: 0, new_provider_requests: 0, input_tokens: 0, output_tokens: 0, microusd: 0 };
    const result = exitCode => ({ version: 1, image: container.image, exitCode,
      elapsedMs: performance.now() - started, stdout: '', stderr: '', usage: { ...usage },
      files: Object.fromEntries(container.exportPaths.filter(p => Object.hasOwn(files, p))
        .map(p => [p, Buffer.from(files[p]).toString('base64')])) });
    try {
      for (let round = 0; ; round++) {
        const remaining = Math.floor(taskTimeoutMs - usedMs - (performance.now() - started));
        if (calls >= maxCalls || remaining < 1000) return result(1);
        const payload = { model, step_id: stepId, files, previous,
          tools: { protocol: 'isolated-command-v1', command: { kind: 'command', argv: ['absolute-executable', 'arguments'] },
            finish: { kind: 'finish' }, network: 'none', persistence: 'selected-submission-files-only' } };
        const requestId = 'call-' + createHash('sha256').update(JSON.stringify([taskId, stepId, round])).digest('hex');
        calls++;
        const receipt = await mediator.dispatch({ taskId, requestId, payload, allowance, withReceipt: true,
          timeoutMs: Math.min(remaining, container.timeoutMs), invoke });
        const increments = { provider_requests: 1, new_provider_requests: receipt.invoked ? 1 : 0, ...receipt.usage };
        for (const key of Object.keys(usage)) {
          if (!Number.isSafeInteger(increments[key]) || increments[key] < 0
              || !Number.isSafeInteger(usage[key] + increments[key])) throw Error('invalid model usage accounting');
          usage[key] += increments[key];
        }
        const action = receipt.output;
        if (!action || Object.getPrototypeOf(action) !== Object.prototype) return result(1);
        if (action.kind === 'finish' && Object.keys(action).join(',') === 'kind') return result(0);
        if (action.kind !== 'command' || Object.keys(action).sort().join(',') !== 'argv,kind') return result(1);
        const commandRemaining = Math.floor(taskTimeoutMs - usedMs - (performance.now() - started));
        if (commandRemaining < 1000) return result(1);
        const command = { ...container, files, argv: action.argv, timeoutMs: Math.min(container.timeoutMs, commandRemaining) };
        try { validateInput(command); } catch { return result(1); }
        const output = await runOfflineContainer(command);
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
}
