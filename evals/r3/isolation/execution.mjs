// SPDX-License-Identifier: AGPL-3.0-or-later
// Explicit backend seam. Existing consumers retain their unchanged Linux path.
// Never infer Windows from the host or translate a POSIX command automatically.
import { runOfflineContainer, validateInput, validateResult } from './container.mjs';
import { nativeCommandExitCode, runNativeWindows, validateNativeInput } from './native-windows.mjs';

function backend(options) {
  if (options.execution === undefined) return 'linux';
  if (options.execution?.kind === 'windows-native-v1') return 'windows';
  if (options.execution?.kind === 'linux-container-v1'
      && Object.keys(options.execution).join(',') === 'kind') return 'linux';
  throw Error('unknown or implicit evaluation backend');
}
export function validateExecutionInput(options) {
  if (backend(options) === 'windows') validateNativeInput(options);
  else validateInput(options);
}
// Validate the complete adapter response, keeping Linux image identity separate
// from a native helper/profile receipt. This does not attest native OS evidence.
export function validateExecutionOutput(options, result) {
  if (!result || !Number.isFinite(result.elapsedMs) || result.elapsedMs < 0) throw Error('invalid execution timing');
  const { image, execution, privateOperation, elapsedMs, ...payload } = result;
  if (backend(options) === 'linux') {
    if (execution !== undefined || privateOperation !== undefined || image !== options.image) throw Error('mixed execution identity');
    validateResult(payload);
  } else {
    if (image !== undefined || execution?.kind !== 'windows-native-v1'
        || execution.profile_sha256 !== options.execution.profile_sha256
        || Object.keys(execution).sort().join(',') !== 'descriptor_sha256,disk_hard_limit_enforced,kind,profile_sha256,receipt_sha256'
        || execution.disk_hard_limit_enforced !== false
        || ![execution.descriptor_sha256, execution.receipt_sha256].every(value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value))) throw Error('invalid native execution identity');
    // Payload bounds are the same; Windows DWORD exits have a wider range than
    // Unix shell statuses. Validate that status independently, never mask it in
    // returned results or diagnostics.
    if (nativeCommandExitCode(payload.exitCode) !== payload.exitCode) throw Error('native result status must be normalized');
    if (!payload.files || ![Object.prototype, null].includes(Object.getPrototypeOf(payload.files))) throw Error('invalid native result file map');
    validateResult({ ...payload, exitCode: 0, files: { ...payload.files } });
  }
  if (options.exportPaths && Object.keys(payload.files).some(name => !options.exportPaths.includes(name))) throw Error('unsolicited execution export');
  return payload;
}
export function createExecutionRunner({ linux = runOfflineContainer, windows = runNativeWindows } = {}) {
  return async options => {
    validateExecutionInput(options);
    const frozen = structuredClone(options);
    if (backend(frozen) === 'windows') return windows(frozen);
    delete frozen.execution;
    return linux(frozen);
  };
}
export const runOfflineExecution = createExecutionRunner();
