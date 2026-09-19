// SPDX-License-Identifier: AGPL-3.0-or-later
import { pathToFileURL } from 'node:url';
import { setTimeout } from 'node:timers/promises';
import { check, OperatorError } from './files.mjs';
import { loadRefreshPolicy, RefreshMaintainer, REFRESH_DEFAULTS } from './refresh.mjs';
import { HttpsTransport } from './transport.mjs';

export async function main(args, emit = value => process.stdout.write(`${JSON.stringify(value)}\n`)) {
  if (args.length === 1 && args[0] === '--help') return { usage: [
    'node refresh-cli.mjs review /private/refresh-policy.json',
    'node refresh-cli.mjs authorize /private/refresh-policy.json EXACT_REVIEW_SHA256',
    'node refresh-cli.mjs recovery-review /private/refresh-policy.json',
    'node refresh-cli.mjs recover /private/refresh-policy.json EXACT_REVIEW_SHA256',
    'node refresh-cli.mjs once /private/refresh-policy.json',
    'node refresh-cli.mjs run /private/refresh-policy.json'],
  defaults: REFRESH_DEFAULTS, note: 'Existing active account only. Review is read-only. No enrollment SDK, password fallback or automatic policy extension.' };
  const [command, path, confirmation] = args;
  check(['review', 'authorize', 'recovery-review', 'recover', 'once', 'run'].includes(command)
    && args.length === (['authorize', 'recover'].includes(command) ? 3 : 2) && !path.startsWith('-'), 'invalid_arguments');
  const policy = await loadRefreshPolicy(path);
  const maintainer = new RefreshMaintainer(policy);
  if (command === 'review') return maintainer.preview();
  if (command === 'recovery-review') return maintainer.recoveryPreview();
  if (command === 'recover') {
    check(/^[a-f0-9]{64}$/u.test(confirmation), 'exact_review_digest_required');
    return maintainer.recover(confirmation);
  }
  if (command === 'authorize') {
    check(/^[a-f0-9]{64}$/u.test(confirmation), 'exact_review_digest_required');
    return maintainer.authorize(confirmation);
  }
  // No pinned enrollment signer is loaded; only fixed-origin HTTPS is available.
  maintainer.transport = new HttpsTransport();
  if (command === 'once') return maintainer.tick();
  const stop = new AbortController();
  const shutdown = () => stop.abort();
  process.once('SIGTERM', shutdown); process.once('SIGINT', shutdown);
  try {
    while (!stop.signal.aborted) {
      const status = await maintainer.tick(); emit(status);
      // Intervention is durable. The process exits, and the supplied unit does
      // not restart a deliberate refusal. Operators must investigate/review it.
      if (status.state === 'intervention') return { stopped: true, intervention_required: true };
      try { await setTimeout(status.retry_at === null ? REFRESH_DEFAULTS.poll_seconds * 1000
        : Math.max(1, Math.min(REFRESH_DEFAULTS.poll_seconds * 1000, status.retry_at * 1000 - Date.now())), undefined, { signal: stop.signal }); }
      catch (error) { if (error.name !== 'AbortError') throw error; }
    }
    return { stopped: true, intervention_required: false };
  } finally { process.removeListener('SIGTERM', shutdown); process.removeListener('SIGINT', shutdown); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).then(result => process.stdout.write(`${JSON.stringify(result)}\n`)).catch(error => {
    process.stderr.write(`${JSON.stringify({ error: error instanceof OperatorError ? error.code : 'refresh_maintenance_failed',
      intervention_required: true, remote_effects_rolled_back: false })}\n`);
    // Deliberate config/lock/auth refusal is not an unlimited automatic retry.
    process.exitCode = 78;
  });
}
