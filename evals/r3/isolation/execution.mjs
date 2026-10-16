// SPDX-License-Identifier: AGPL-3.0-or-later
// Explicit backend seam. Existing consumers retain their unchanged Linux path.
// Never infer Windows from the host or translate a POSIX command automatically.
import { runOfflineContainer, validateInput } from './container.mjs';
import { runNativeWindows, validateNativeInput } from './native-windows.mjs';

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
