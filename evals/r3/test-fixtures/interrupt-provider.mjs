// SPDX-License-Identifier: AGPL-3.0-or-later
// TEST-ONLY Node preload. Never packaged in the approved runtime. It cannot
// contact OpenAI: only loopback service traffic may use the real fetch function.
import assert from 'node:assert/strict';
const native = globalThis.fetch.bind(globalThis);
globalThis.fetch = async (url, options) => {
  if (/^http:\/\/127\.0\.0\.1:[0-9]+\//.test(String(url))) return native(url, options);
  assert.equal(url, 'https://api.openai.com/v1/embeddings');
  assert.equal(options.headers.Authorization, 'Bearer synthetic-paid-launcher');
  const signal = new URL(import.meta.url).searchParams.get('signal') === 'SIGKILL' ? 'SIGKILL' : 'SIGTERM';
  setImmediate(() => process.kill(process.pid, signal));
  return new Promise((_resolve, reject) => options.signal.addEventListener('abort', () => reject(Error('synthetic interruption')), { once: true }));
};
