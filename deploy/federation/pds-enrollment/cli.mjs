// SPDX-License-Identifier: AGPL-3.0-or-later
import { pathToFileURL } from 'node:url';
import { check, OperatorError } from './files.mjs';
import { EnrollmentOperator, loadConfig } from './operator.mjs';
import { HttpsTransport, pinnedSigner } from './transport.mjs';

export async function main(args) {
  if (args.length === 1 && args[0] === '--help') {
    return { usage: ['node cli.mjs /private/config.json [phase]', 'node cli.mjs execute /private/config.json PHASE EXACT_REVIEW_SHA256'],
      phases: ['bootstrap', 'create', 'discover-key', 'handoff', 'activate', 'reconcile', 'refresh'],
      default: 'offline review only; no writes, secret reads, SDK loading or network',
      warning: 'No live gateway readiness is established. Preserve state and reconcile interrupted operations.' };
  }
  const execute = args[0] === 'execute';
  check(execute ? args.length === 4 : args.length >= 1 && args.length <= 2, 'invalid_arguments');
  const [path, phase, confirmation] = execute ? args.slice(1) : args;
  check(!path.startsWith('-') && (!execute || /^[a-f0-9]{64}$/u.test(confirmation)), 'invalid_arguments');
  const config = await loadConfig(path);
  const operator = new EnrollmentOperator(config);
  if (!execute) return operator.preview(phase);
  // Reject wrong confirmation before loading the SDK or reading credentials.
  const review = await operator.preview(phase);
  check(review.review_sha256 === confirmation, 'exact_review_digest_required');
  operator.signer = await pinnedSigner(); operator.transport = new HttpsTransport();
  return operator.execute(phase, confirmation);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).then(result => { process.stdout.write(`${JSON.stringify(result)}\n`); }).catch(error => {
    // Never print stack, config, paths, URLs with payloads, token or upstream body.
    process.stderr.write(`${JSON.stringify({ error: error instanceof OperatorError ? error.code : 'operator_failed_reconcile_before_retry',
      gateway_ready: false, remote_effects_rolled_back: false })}\n`);
    process.exitCode = 1;
  });
}
